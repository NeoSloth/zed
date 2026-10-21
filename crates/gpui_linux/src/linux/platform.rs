use std::{
    cell::RefCell,
    env,
    io::ErrorKind,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
#[cfg(any(feature = "wayland", feature = "x11"))]
use std::{
    ffi::OsString,
    fs::File,
    io::Read as _,
    os::fd::{AsFd, AsRawFd},
};

#[cfg(any(feature = "wayland", feature = "x11"))]
use anyhow::ensure;
use anyhow::{Context as _, anyhow};
#[cfg(any(feature = "wayland", feature = "x11"))]
use ashpd::{
    desktop::{
        Request,
        inhibit::{InhibitFlags, InhibitOptions, InhibitProxy},
    },
    enumflags2::BitFlags,
};
use calloop::{EventLoop, LoopHandle, LoopSignal, channel::Sender};
use futures::{FutureExt as _, StreamExt as _, channel::oneshot, future::BoxFuture};
use gpui_util::{ResultExt as _, new_std_command};
use smol::lock::Mutex;
#[cfg(any(feature = "wayland", feature = "x11"))]
use xkbcommon::xkb::{self, Keycode, Keysym, State};

use crate::linux::{
    Backend, DisplayConnection, HeadlessConnection, LinuxDispatcher, PriorityQueueCalloopReceiver,
    select_backend,
};
use gpui::{
    Action, ActivityGuard, AnyWindowHandle, BackgroundExecutor, ClipboardItem, CursorStyle,
    ForegroundExecutor, GraphicalEnvironment, Keymap, Menu, MenuItem, OwnedMenu, PathPromptOptions,
    Platform, PlatformDisplay, PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem,
    PlatformWindow, Result, RunnableVariant, Task, ThermalState, WindowAppearance,
    WindowButtonLayout, WindowParams, WindowingModes, WindowingRequest,
};
#[cfg(any(feature = "wayland", feature = "x11"))]
use gpui::{Pixels, Point, px};

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(crate) const SCROLL_LINES: f32 = 3.0;

// Values match the defaults on GTK.
// Taken from https://github.com/GNOME/gtk/blob/main/gtk/gtksettings.c#L320
#[cfg(any(feature = "wayland", feature = "x11"))]
pub(crate) const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(400);
#[cfg(any(feature = "wayland", feature = "x11"))]
pub(crate) const DOUBLE_CLICK_DISTANCE: Pixels = px(5.0);
pub(crate) const KEYRING_LABEL: &str = "zed-github-account";

#[cfg(any(feature = "wayland", feature = "x11"))]
const FILE_PICKER_PORTAL_MISSING: &str =
    "Couldn't open file picker due to missing xdg-desktop-portal implementation.";

#[derive(Default)]
pub(crate) struct PlatformHandlers {
    pub(crate) open_urls: Option<Box<dyn FnMut(Vec<String>)>>,
    pub(crate) quit: Option<Box<dyn FnMut() -> bool>>,
    pub(crate) reopen: Option<Box<dyn FnMut()>>,
    pub(crate) app_menu_action: Option<Box<dyn FnMut(&dyn Action)>>,
    pub(crate) will_open_app_menu: Option<Box<dyn FnMut()>>,
    pub(crate) validate_app_menu_command: Option<Box<dyn FnMut(&dyn Action) -> bool>>,
    pub(crate) keyboard_layout_change: Option<Box<dyn FnMut()>>,
    pub(crate) system_sleep: Option<Box<dyn FnMut()>>,
    pub(crate) system_wake: Option<Box<dyn FnMut()>>,
    pub(crate) displays_changed: Option<Box<dyn FnMut()>>,
}

/// A logind `PrepareForSleep` signal, forwarded from the D-Bus listener to
/// the client's event loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(all(target_os = "linux", any(feature = "wayland", feature = "x11"))),
    allow(dead_code)
)]
pub(crate) enum SystemPowerEvent {
    Sleep,
    Wake,
}

pub(crate) struct LinuxCommon {
    pub(crate) background_executor: BackgroundExecutor,
    pub(crate) foreground_executor: ForegroundExecutor,
    pub(crate) text_system: Arc<dyn PlatformTextSystem>,
    pub(crate) appearance: WindowAppearance,
    pub(crate) auto_hide_scrollbars: bool,
    pub(crate) button_layout: WindowButtonLayout,
    pub(crate) callbacks: PlatformHandlers,
    pub(crate) signal: LoopSignal,
    pub(crate) menus: Vec<OwnedMenu>,
    app_name: Option<String>,
    system_notifications: crate::linux::system_notifications::SystemNotificationState,
    #[cfg_attr(
        not(all(target_os = "linux", any(feature = "wayland", feature = "x11"))),
        allow(dead_code)
    )]
    power_sender: Sender<SystemPowerEvent>,
    power_listener_started: bool,
    /// Runs after each foreground runnable registered by [`LinuxCommon::register_sources`].
    ///
    /// X11 uses this to handle events that a runnable's requests read into xcb's queue, where
    /// they would not wake the event loop.
    pub(crate) after_runnable: Option<Rc<dyn Fn()>>,
}

impl LinuxCommon {
    pub fn new(
        signal: LoopSignal,
    ) -> (
        Self,
        PriorityQueueCalloopReceiver<RunnableVariant>,
        calloop::channel::Channel<SystemPowerEvent>,
    ) {
        let (main_sender, main_receiver) = PriorityQueueCalloopReceiver::new();
        let (power_sender, power_receiver) = calloop::channel::channel();

        #[cfg(any(feature = "wayland", feature = "x11"))]
        let text_system = Arc::new(crate::linux::CosmicTextSystem::new("IBM Plex Sans"));
        #[cfg(not(any(feature = "wayland", feature = "x11")))]
        let text_system = Arc::new(gpui::NoopTextSystem::new());

        let callbacks = PlatformHandlers::default();

        let dispatcher = Arc::new(LinuxDispatcher::new(main_sender));

        let background_executor = BackgroundExecutor::new(dispatcher.clone());

        let common = LinuxCommon {
            background_executor,
            foreground_executor: ForegroundExecutor::new(dispatcher),
            text_system,
            appearance: WindowAppearance::Light,
            auto_hide_scrollbars: false,
            button_layout: WindowButtonLayout::linux_default(),
            callbacks,
            signal,
            menus: Vec::new(),
            app_name: None,
            system_notifications: crate::linux::system_notifications::SystemNotificationState::new(
            ),
            power_sender,
            power_listener_started: false,
            after_runnable: None,
        };

        (common, main_receiver, power_receiver)
    }

    pub(crate) fn start_power_listener(&mut self) {
        if !self.power_listener_started {
            #[cfg(all(target_os = "linux", any(feature = "wayland", feature = "x11")))]
            smol::spawn({
                let power_sender = self.power_sender.clone();
                async move {
                    if let Err(error) = listen_for_system_power_events(power_sender).await {
                        log::debug!("failed to listen for system sleep/wake events: {error:?}");
                    }
                }
            })
            .detach();

            self.power_listener_started = true;
        }
    }

    /// Registers the foreground executor and power listener on a loop shared with display clients.
    ///
    /// Foreground runnables run as idle callbacks, after the loop has dispatched all pending events.
    pub(crate) fn register_sources(
        common: &Rc<RefCell<Self>>,
        loop_handle: &calloop::LoopHandle<'static, ()>,
        main_receiver: PriorityQueueCalloopReceiver<RunnableVariant>,
        power_receiver: calloop::channel::Channel<SystemPowerEvent>,
    ) -> anyhow::Result<()> {
        loop_handle
            .insert_source(main_receiver, {
                let loop_handle = loop_handle.clone();
                let common = common.clone();
                move |event, _, _| {
                    if let calloop::channel::Event::Msg(runnable) = event {
                        let common = common.clone();
                        loop_handle.insert_idle(move |_| {
                            let location = runnable.metadata().location;
                            let spawned = runnable.metadata().spawned;
                            gpui::profiler::update_running_task(spawned, location);
                            runnable.run();
                            gpui::profiler::save_task_timing();

                            let after_runnable = common.borrow().after_runnable.clone();
                            if let Some(after_runnable) = after_runnable {
                                after_runnable();
                            }
                        });
                    }
                }
            })
            .map_err(|error| anyhow!("failed to register foreground executor: {error}"))?;
        loop_handle
            .insert_source(power_receiver, {
                let common = common.clone();
                move |event, _, _| {
                    if let calloop::channel::Event::Msg(event) = event {
                        LinuxCommon::handle_system_power_event(&common, event);
                    }
                }
            })
            .map_err(|error| anyhow!("failed to register power listener: {error}"))?;
        Ok(())
    }

    /// Runs the app's sleep or wake callback, without `common` borrowed, since the callback
    /// can call back into the platform.
    fn handle_system_power_event(common: &RefCell<Self>, event: SystemPowerEvent) {
        fn slot(
            callbacks: &mut PlatformHandlers,
            event: SystemPowerEvent,
        ) -> &mut Option<Box<dyn FnMut()>> {
            match event {
                SystemPowerEvent::Sleep => &mut callbacks.system_sleep,
                SystemPowerEvent::Wake => &mut callbacks.system_wake,
            }
        }
        let callback = slot(&mut common.borrow_mut().callbacks, event).take();
        if let Some(mut callback) = callback {
            callback();
            let mut common = common.borrow_mut();
            let slot = slot(&mut common.callbacks, event);
            // The callback may have replaced itself.
            if slot.is_none() {
                *slot = Some(callback);
            }
        }
    }
}

#[cfg(all(target_os = "linux", any(feature = "wayland", feature = "x11")))]
async fn listen_for_system_power_events(
    power_sender: Sender<SystemPowerEvent>,
) -> anyhow::Result<()> {
    use futures::StreamExt as _;

    let connection = ashpd::zbus::Connection::system().await?;
    let proxy = ashpd::zbus::Proxy::new(
        &connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await?;
    let mut sleep_events = proxy.receive_signal("PrepareForSleep").await?;

    while let Some(message) = sleep_events.next().await {
        let event = if message.body().deserialize::<bool>()? {
            SystemPowerEvent::Sleep
        } else {
            SystemPowerEvent::Wake
        };
        power_sender.send(event).ok();
    }

    Ok(())
}

/// GPUI's platform for Linux and FreeBSD.
///
/// One event loop serves the whole process. The foreground executor and power listener are
/// registered on it once, and the current [`DisplayConnection`] adds and removes its own sources
/// as the app switches between the display modes it allows.
pub(crate) struct LinuxPlatform {
    common: Rc<RefCell<LinuxCommon>>,
    /// The event loop, until `run` takes it.
    event_loop: RefCell<Option<EventLoop<'static, ()>>>,
    loop_handle: LoopHandle<'static, ()>,
    connection: RefCell<DisplayConnection>,
    allowed_modes: WindowingModes,
    /// The mode to start in, applied when `run` starts. Windowed in the process's own
    /// environment when unset.
    initial_windowing: RefCell<Option<WindowingRequest>>,
    /// Taken by the connection made when `run` starts.
    startup_environment: RefCell<Option<StartupEnvironment>>,
    /// A requested switch waiting for the event loop to apply it.
    pending_mode: RefCell<Option<WindowingRequest>>,
    transition_waiter: RefCell<Option<oneshot::Sender<anyhow::Result<()>>>>,
    secret_service: Arc<Mutex<SecretServiceCache>>,
}

/// What a Wayland connection made at startup may use from this process's environment.
///
/// Both are taken from the environment when the platform is created, before it starts any
/// threads, and are only meaningful for the connection made then.
#[derive(Default)]
struct StartupEnvironment {
    #[cfg(feature = "wayland")]
    activation_token: Option<String>,
    /// The socket `WAYLAND_SOCKET` names: a connection to the compositor that the process which
    /// started us already opened.
    #[cfg(feature = "wayland")]
    wayland_socket: Option<std::os::unix::net::UnixStream>,
}

impl LinuxPlatform {
    /// Creates the platform, headless until `run` connects it in its initial mode.
    pub(crate) fn new(allowed_modes: WindowingModes) -> Self {
        let startup = StartupEnvironment {
            #[cfg(feature = "wayland")]
            activation_token: crate::linux::take_startup_activation_token_from_environment(),
            #[cfg(feature = "wayland")]
            wayland_socket: crate::linux::take_wayland_socket_from_environment(),
        };
        let event_loop = EventLoop::try_new().expect("failed to create Linux event loop");
        let (common, main_receiver, power_receiver) = LinuxCommon::new(event_loop.get_signal());
        let common = Rc::new(RefCell::new(common));
        let loop_handle = event_loop.handle();
        LinuxCommon::register_sources(&common, &loop_handle, main_receiver, power_receiver)
            .expect("failed to register Linux event sources");
        Self {
            common,
            event_loop: RefCell::new(Some(event_loop)),
            loop_handle,
            connection: RefCell::new(DisplayConnection::Headless(HeadlessConnection::new())),
            allowed_modes,
            initial_windowing: RefCell::new(None),
            startup_environment: RefCell::new(Some(startup)),
            pending_mode: RefCell::new(None),
            transition_waiter: RefCell::new(None),
            secret_service: Arc::new(Mutex::new(SecretServiceCache::default())),
        }
    }

    /// Connects in the initial mode, before the app finishes launching.
    ///
    /// # Panics
    ///
    /// Panics if the allowed modes don't allow the initial mode, or if its display server can't
    /// be reached, since the app can't start. A windowed initial mode whose environment names no
    /// allowed display server starts headless when that's allowed.
    fn connect_initially(&self) {
        let request = self
            .initial_windowing
            .borrow_mut()
            .take()
            .unwrap_or_else(|| WindowingRequest::Windowed(GraphicalEnvironment::detect()));
        // Dropped unused when starting headless, closing any inherited `WAYLAND_SOCKET`.
        let startup = self
            .startup_environment
            .borrow_mut()
            .take()
            .unwrap_or_default();
        let backend = match &request {
            WindowingRequest::Headless => None,
            WindowingRequest::Windowed(environment) => {
                select_backend(self.allowed_modes, environment)
                    .map(|backend| (backend, environment))
            }
        };
        match backend {
            Some((backend, environment)) => {
                let connection = self
                    .connect(backend, environment, startup)
                    .unwrap_or_else(|error| panic!("{error:#}"));
                self.replace_connection(connection);
            }
            None => assert!(
                self.allowed_modes.contains(WindowingModes::HEADLESS),
                "{:?} does not allow starting headless, and no allowed display server was given",
                self.allowed_modes
            ),
        }
    }

    fn with_common<R>(&self, function: impl FnOnce(&mut LinuxCommon) -> R) -> R {
        function(&mut self.common.borrow_mut())
    }

    fn with_keyring<R, F>(
        &self,
        read_only: bool,
        operation: impl FnOnce(Arc<oo7::Keyring>) -> F + Clone + Send + 'static,
    ) -> Task<Result<R>>
    where
        R: Send + 'static,
        F: Future<Output = Result<R>> + Send,
    {
        let secret_service = self.secret_service.clone();
        self.background_executor().spawn(async move {
            if oo7::ashpd::is_sandboxed() {
                let keyring = oo7::Keyring::new().await?;
                keyring.unlock().await?;
                return operation(Arc::new(keyring)).await;
            }

            let mut cache = secret_service.lock().await;
            let mut retry_available = true;
            loop {
                let mut connection = None;
                let mut retry_safe = true;
                let operation = operation.clone();
                let result = async {
                    let connection = connection.insert(match cache.connected.take() {
                        Some(connection) => connection,
                        None => {
                            let connection = cache
                                .connecting
                                .get_or_insert_with(|| SecretServiceKeyring::connect().boxed())
                                .await;
                            cache.connecting = None;
                            connection?
                        }
                    });
                    let SecretServiceKeyring {
                        keyring,
                        collection_path,
                        bus,
                        owner,
                        disconnected,
                    } = connection;
                    await_secret_service(disconnected, async {
                        SecretServiceKeyring::prepare(keyring, collection_path, bus, owner).await?;
                        retry_safe = read_only;
                        operation(keyring.clone()).await
                    })
                    .await
                }
                .await;
                if let Err(error) = &result
                    && classify_keyring_error(error) != KeyringErrorKind::Other
                {
                    if retry_available && retry_safe {
                        retry_available = false;
                        continue;
                    }
                    return result;
                }
                cache.connected = connection;
                return result;
            }
        })
    }

    /// Connects to the display server `backend` in `environment`.
    ///
    /// `startup` is only given for the connection made when the platform is created.
    #[cfg_attr(
        not(feature = "wayland"),
        allow(
            unused_variables,
            reason = "only Wayland uses the startup environment, and without a display backend \
                      `Backend` has no variants"
        )
    )]
    fn connect(
        &self,
        backend: Backend,
        environment: &GraphicalEnvironment,
        startup: StartupEnvironment,
    ) -> anyhow::Result<DisplayConnection> {
        let loop_handle = self.loop_handle.clone();
        let common = self.common.clone();
        match backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland => {
                let activation_token = environment
                    .activation_token
                    .clone()
                    .or(startup.activation_token);
                Ok(DisplayConnection::Wayland(
                    crate::linux::WaylandConnection::attach(
                        loop_handle,
                        common,
                        environment,
                        startup.wayland_socket,
                        activation_token,
                    )?,
                ))
            }
            #[cfg(feature = "x11")]
            Backend::X11 => Ok(DisplayConnection::X11(crate::linux::X11Connection::attach(
                loop_handle,
                common,
                environment,
            )?)),
        }
    }

    /// Installs `connection`, then drops the previous one outside the borrow, since dropping a
    /// connection can call back into the platform.
    fn replace_connection(&self, connection: DisplayConnection) {
        let previous = std::mem::replace(&mut *self.connection.borrow_mut(), connection);
        drop(previous);
        // The app caches the keyboard layout and the displays, which belong to
        // the connection.
        let callback = self
            .common
            .borrow_mut()
            .callbacks
            .keyboard_layout_change
            .take();
        if let Some(mut callback) = callback {
            callback();
            let mut common = self.common.borrow_mut();
            // The callback may have replaced itself.
            if common.callbacks.keyboard_layout_change.is_none() {
                common.callbacks.keyboard_layout_change = Some(callback);
            }
        }
        let callback = self.common.borrow_mut().callbacks.displays_changed.take();
        if let Some(mut callback) = callback {
            callback();
            self.common
                .borrow_mut()
                .callbacks
                .displays_changed
                .get_or_insert(callback);
        }
    }

    /// Applies a pending switch.
    ///
    /// Runs between event-loop iterations, so no event source is mid-dispatch when the
    /// connection's sources are added or removed.
    fn apply_pending_mode(&self) {
        let Some(mode) = self.pending_mode.borrow().clone() else {
            return;
        };
        // `request_mode` rejects open windows, but one can be opened before this runs.
        let result = if self.connection.borrow().has_windows() {
            Err(anyhow!("a window was opened while switching display modes"))
        } else {
            match mode {
                WindowingRequest::Headless => {
                    self.replace_connection(DisplayConnection::Headless(HeadlessConnection::new()));
                    Ok(())
                }
                WindowingRequest::Windowed(environment) => {
                    select_backend(self.allowed_modes, &environment)
                        .context("the environment names no allowed Wayland or X11 display server")
                        .and_then(|backend| {
                            self.connect(backend, &environment, StartupEnvironment::default())
                        })
                        .map(|connection| self.replace_connection(connection))
                }
            }
        };
        self.pending_mode.borrow_mut().take();
        if let Some(waiter) = self.transition_waiter.borrow_mut().take() {
            waiter.send(result).ok();
        }
    }

    /// Validates a requested switch, then leaves it for the event loop to apply.
    fn request_mode(&self, mode: WindowingRequest) -> Task<anyhow::Result<()>> {
        if self.pending_mode.borrow().is_some() {
            return Task::ready(Err(anyhow!(
                "a display mode transition is already in progress"
            )));
        }
        let is_headless = self.connection.borrow().is_headless();
        match &mode {
            WindowingRequest::Headless
                if !self.allowed_modes.contains(WindowingModes::HEADLESS) =>
            {
                return Task::ready(Err(anyhow!(
                    "{:?} does not allow headless mode",
                    self.allowed_modes
                )));
            }
            WindowingRequest::Headless if is_headless => {
                return Task::ready(Err(anyhow!("already headless")));
            }
            WindowingRequest::Headless => {}
            WindowingRequest::Windowed(_) if !is_headless => {
                return Task::ready(Err(anyhow!(
                    "already windowed ({}); switch to headless mode first",
                    self.connection.borrow().compositor_name()
                )));
            }
            WindowingRequest::Windowed(environment) => {
                if select_backend(self.allowed_modes, environment).is_none() {
                    return Task::ready(Err(anyhow!(
                        "the environment names no display server that {:?} allows",
                        self.allowed_modes
                    )));
                }
            }
        }
        let (sender, receiver) = oneshot::channel();
        self.transition_waiter.borrow_mut().replace(sender);
        *self.pending_mode.borrow_mut() = Some(mode);
        // The wakeup persists until the loop next polls, so a request made before `run` starts is
        // applied after its first iteration.
        self.common.borrow().signal.wakeup();
        self.common.borrow().foreground_executor.spawn(async move {
            receiver
                .await
                .map_err(|_| anyhow!("display mode transition was canceled"))
                .and_then(|result| result)
        })
    }
}

impl Drop for LinuxPlatform {
    fn drop(&mut self) {
        self.replace_connection(DisplayConnection::Headless(HeadlessConnection::new()));
        #[cfg(feature = "x11")]
        crate::linux::wait_for_clipboard_handovers();
    }
}

impl Platform for LinuxPlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        self.with_common(|common| common.background_executor.clone())
    }

    fn foreground_executor(&self) -> ForegroundExecutor {
        self.with_common(|common| common.foreground_executor.clone())
    }

    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        self.with_common(|common| common.text_system.clone())
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        self.connection.borrow().keyboard_layout()
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(gpui::DummyKeyboardMapper)
    }

    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {
        self.with_common(|common| common.callbacks.keyboard_layout_change = Some(callback));
    }

    fn on_thermal_state_change(&self, _callback: Box<dyn FnMut()>) {}

    fn thermal_state(&self) -> ThermalState {
        ThermalState::Nominal
    }

    #[cfg(not(any(feature = "wayland", feature = "x11")))]
    fn prevent_idle_sleep(&self, reason: &str) -> Task<Result<ActivityGuard>> {
        Task::ready(Err(anyhow!(
            "Idle sleep prevention for {reason:?} requires a Linux windowing backend"
        )))
    }

    #[cfg(any(feature = "wayland", feature = "x11"))]
    fn prevent_idle_sleep(&self, reason: &str) -> Task<Result<ActivityGuard>> {
        let executor = self.background_executor();
        let (guard_tx, guard_rx) = oneshot::channel();
        executor
            .spawn({
                let executor = executor.clone();
                let reason = reason.to_owned();
                async move {
                    guard_tx
                        .send(inhibit_idle_sleep(reason, executor).await)
                        .ok();
                }
            })
            .detach();
        executor
            .clone()
            .spawn(async move { await_idle_sleep_prevention(guard_rx, &executor).await })
    }

    fn run(&self, on_finish_launching: Box<dyn FnOnce()>) {
        self.connect_initially();
        on_finish_launching();

        let mut event_loop = self
            .event_loop
            .borrow_mut()
            .take()
            .expect("App is already running");
        event_loop
            .run(None, &mut (), |_| self.apply_pending_mode())
            .log_err();

        let quit = self.with_common(|common| common.callbacks.quit.take());
        if let Some(mut fun) = quit {
            fun();
        }
        #[cfg(feature = "x11")]
        crate::linux::wait_for_clipboard_handovers();
    }

    fn quit(&self) {
        self.with_common(|common| common.signal.stop());
    }

    fn set_initial_windowing(&self, request: WindowingRequest) {
        *self.initial_windowing.borrow_mut() = Some(request);
    }

    fn request_windowing(&self, request: WindowingRequest) -> Task<anyhow::Result<()>> {
        self.request_mode(request)
    }

    fn compositor_name(&self) -> &'static str {
        self.connection.borrow().compositor_name()
    }

    fn graphical_environment(&self) -> Option<GraphicalEnvironment> {
        self.connection.borrow().graphical_environment()
    }

    fn restart(&self, binary_path: Option<PathBuf>, arguments: Vec<std::ffi::OsString>) {
        use std::os::unix::process::CommandExt as _;

        // get the process id of the current process
        let app_pid = std::process::id().to_string();
        // get the path to the executable
        let app_path = if let Some(path) = binary_path {
            path
        } else {
            match self.app_path() {
                Ok(path) => path,
                Err(err) => {
                    log::error!("Failed to get app path: {:?}", err);
                    return;
                }
            }
        };

        log::info!("Restarting process, using app path: {:?}", app_path);

        // Script to wait for the current process to exit and then restart the app.
        // Pass dynamic values as positional parameters to avoid shell interpolation issues.
        let script = r#"
            while kill -0 "$0" 2>/dev/null; do
                sleep 0.1
            done

            app_path="$1"
            shift
            "$app_path" "$@"
            "#;

        #[allow(
            clippy::disallowed_methods,
            reason = "We are restarting ourselves, using std command thus is fine"
        )]
        let restart_process = new_std_command("/usr/bin/env")
            .arg("bash")
            .arg("-c")
            .arg(script)
            .arg(&app_pid)
            .arg(&app_path)
            .args(arguments)
            .process_group(0)
            .spawn();

        match restart_process {
            Ok(_) => self.quit(),
            Err(e) => log::error!("failed to spawn restart script: {:?}", e),
        }
    }

    fn activate(&self, _ignoring_other_apps: bool) {
        log::info!("activate is not implemented on Linux, ignoring the call")
    }

    fn hide(&self) {
        log::info!("hide is not implemented on Linux, ignoring the call")
    }

    fn hide_other_apps(&self) {
        log::info!("hide_other_apps is not implemented on Linux, ignoring the call")
    }

    fn unhide_other_apps(&self) {
        log::info!("unhide_other_apps is not implemented on Linux, ignoring the call")
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        self.connection.borrow().primary_display()
    }

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        self.connection.borrow().displays()
    }

    fn on_displays_changed(&self, callback: Box<dyn FnMut()>) {
        self.with_common(|common| common.callbacks.displays_changed = Some(callback));
    }

    #[cfg(feature = "screen-capture")]
    fn is_screen_capture_supported(&self) -> bool {
        self.connection.borrow().is_screen_capture_supported()
    }

    #[cfg(feature = "screen-capture")]
    fn screen_capture_sources(
        &self,
    ) -> oneshot::Receiver<Result<Vec<Rc<dyn gpui::ScreenCaptureSource>>>> {
        self.connection.borrow().screen_capture_sources()
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        self.connection.borrow().active_window()
    }

    fn window_stack(&self) -> Option<Vec<AnyWindowHandle>> {
        self.connection.borrow().window_stack()
    }

    fn open_window(
        &self,
        handle: AnyWindowHandle,
        options: WindowParams,
    ) -> anyhow::Result<Box<dyn PlatformWindow>> {
        self.connection.borrow().open_window(handle, options)
    }

    fn open_url(&self, url: &str) {
        self.connection.borrow().open_uri(url);
    }

    fn on_open_urls(&self, callback: Box<dyn FnMut(Vec<String>)>) {
        self.with_common(|common| common.callbacks.open_urls = Some(callback));
    }

    fn prompt_for_paths(
        &self,
        options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        let (done_tx, done_rx) = oneshot::channel();

        #[cfg(not(any(feature = "wayland", feature = "x11")))]
        let _ = (done_tx.send(Ok(None)), options);

        #[cfg(any(feature = "wayland", feature = "x11"))]
        let identifier = self.connection.borrow().window_identifier();

        #[cfg(any(feature = "wayland", feature = "x11"))]
        self.foreground_executor()
            .spawn(async move {
                let title = if options.directories {
                    "Open Folder"
                } else {
                    "Open File"
                };

                let request = match ashpd::desktop::file_chooser::OpenFileRequest::default()
                    .identifier(identifier.await)
                    .modal(true)
                    .title(title)
                    .accept_label(options.prompt.as_ref().map(gpui::SharedString::as_str))
                    .multiple(options.multiple)
                    .directory(options.directories)
                    .send()
                    .await
                {
                    Ok(request) => request,
                    Err(err) => {
                        let result = match err {
                            ashpd::Error::PortalNotFound(_) => anyhow!(FILE_PICKER_PORTAL_MISSING),
                            err => err.into(),
                        };
                        let _ = done_tx.send(Err(result));
                        return;
                    }
                };

                let result = match request.response() {
                    Ok(response) => Ok(Some(
                        response
                            .uris()
                            .iter()
                            .filter_map(|uri: &ashpd::Uri| url::Url::parse(uri.as_str()).ok())
                            .filter_map(|uri: url::Url| uri.to_file_path().ok())
                            .collect::<Vec<_>>(),
                    )),
                    Err(ashpd::Error::Response(_)) => Ok(None),
                    Err(e) => Err(e.into()),
                };
                let _ = done_tx.send(result);
            })
            .detach();
        done_rx
    }

    fn prompt_for_new_path(
        &self,
        directory: &Path,
        suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        let (done_tx, done_rx) = oneshot::channel();

        #[cfg(not(any(feature = "wayland", feature = "x11")))]
        let _ = (done_tx.send(Ok(None)), directory, suggested_name);

        #[cfg(any(feature = "wayland", feature = "x11"))]
        let identifier = self.connection.borrow().window_identifier();

        #[cfg(any(feature = "wayland", feature = "x11"))]
        self.foreground_executor()
            .spawn({
                let directory = directory.to_owned();
                let suggested_name = suggested_name.map(|s| s.to_owned());

                async move {
                    let mut request_builder =
                        ashpd::desktop::file_chooser::SaveFileRequest::default()
                            .identifier(identifier.await)
                            .modal(true)
                            .title("Save File")
                            .current_folder(directory)
                            .expect("pathbuf should not be nul terminated");

                    if let Some(suggested_name) = suggested_name {
                        request_builder = request_builder.current_name(suggested_name.as_str());
                    }

                    let request = match request_builder.send().await {
                        Ok(request) => request,
                        Err(err) => {
                            let result = match err {
                                ashpd::Error::PortalNotFound(_) => {
                                    anyhow!(FILE_PICKER_PORTAL_MISSING)
                                }
                                err => err.into(),
                            };
                            let _ = done_tx.send(Err(result));
                            return;
                        }
                    };

                    let result = match request.response() {
                        Ok(response) => Ok(response
                            .uris()
                            .first()
                            .and_then(|uri: &ashpd::Uri| url::Url::parse(uri.as_str()).ok())
                            .and_then(|uri: url::Url| uri.to_file_path().ok())),
                        Err(ashpd::Error::Response(_)) => Ok(None),
                        Err(e) => Err(e.into()),
                    };
                    let _ = done_tx.send(result);
                }
            })
            .detach();

        done_rx
    }

    fn can_select_mixed_files_and_dirs(&self) -> bool {
        // org.freedesktop.portal.FileChooser only supports "pick files" and "pick directories".
        false
    }

    fn reveal_path(&self, path: &Path) {
        self.connection.borrow().reveal_path(path.to_owned());
    }

    fn open_with_system(&self, path: &Path) {
        let path = path.to_owned();
        let environment = self.connection.borrow().graphical_environment();
        self.background_executor()
            .spawn(async move {
                let mut command = new_std_command("xdg-open");
                command.arg(path);
                if let Some(environment) = environment {
                    environment.apply_to(&mut command);
                }
                #[allow(
                    clippy::disallowed_methods,
                    reason = "running on a background thread, so blocking is fine"
                )]
                command.status().context("invoking xdg-open").log_err();
            })
            .detach();
    }

    fn on_quit(&self, callback: Box<dyn FnMut() -> bool>) {
        self.with_common(|common| {
            common.callbacks.quit = Some(callback);
        });
    }

    fn on_reopen(&self, callback: Box<dyn FnMut()>) {
        self.with_common(|common| {
            common.callbacks.reopen = Some(callback);
        });
    }

    fn on_system_sleep(&self, callback: Box<dyn FnMut()>) {
        self.with_common(|common| {
            common.callbacks.system_sleep = Some(callback);
            common.start_power_listener();
        });
    }

    fn on_system_wake(&self, callback: Box<dyn FnMut()>) {
        self.with_common(|common| {
            common.callbacks.system_wake = Some(callback);
            common.start_power_listener();
        });
    }

    fn set_app_identity(&self, _identifier: &str, name: &str) {
        self.with_common(|common| common.app_name = Some(name.to_string()));
    }

    fn show_system_notification(&self, notification: gpui::SystemNotification) {
        self.with_common(|common| {
            common
                .system_notifications
                .show(common.app_name.as_deref(), notification)
        });
    }

    fn dismiss_system_notification(&self, tag: &str) {
        self.with_common(|common| common.system_notifications.dismiss(tag));
    }

    fn on_system_notification_response(
        &self,
        callback: Box<dyn FnMut(gpui::SystemNotificationResponse)>,
    ) {
        self.with_common(|common| {
            let executor = common.foreground_executor.clone();
            common.system_notifications.on_response(&executor, callback)
        });
    }

    fn on_app_menu_action(&self, callback: Box<dyn FnMut(&dyn Action)>) {
        self.with_common(|common| {
            common.callbacks.app_menu_action = Some(callback);
        });
    }

    fn on_will_open_app_menu(&self, callback: Box<dyn FnMut()>) {
        self.with_common(|common| {
            common.callbacks.will_open_app_menu = Some(callback);
        });
    }

    fn on_validate_app_menu_command(&self, callback: Box<dyn FnMut(&dyn Action) -> bool>) {
        self.with_common(|common| {
            common.callbacks.validate_app_menu_command = Some(callback);
        });
    }

    fn app_path(&self) -> Result<PathBuf> {
        // get the path of the executable of the current process
        let app_path = env::current_exe()?;
        Ok(app_path)
    }

    fn set_menus(&self, menus: Vec<Menu>, _keymap: &Keymap) {
        self.with_common(|common| {
            common.menus = menus.into_iter().map(|menu| menu.owned()).collect();
        })
    }

    fn get_menus(&self) -> Option<Vec<OwnedMenu>> {
        self.with_common(|common| Some(common.menus.clone()))
    }

    fn set_dock_menu(&self, _menu: Vec<MenuItem>, _keymap: &Keymap) {
        // todo(linux)
    }

    fn path_for_auxiliary_executable(&self, _name: &str) -> Result<PathBuf> {
        Err(anyhow::Error::msg(
            "Platform<LinuxPlatform>::path_for_auxiliary_executable is not implemented yet",
        ))
    }

    fn set_cursor_style(&self, style: CursorStyle) {
        self.connection.borrow().set_cursor_style(style)
    }

    fn hide_cursor_until_mouse_moves(&self) {
        self.connection.borrow().hide_cursor_until_mouse_moves()
    }

    fn is_cursor_visible(&self) -> bool {
        self.connection.borrow().is_cursor_visible()
    }

    fn should_auto_hide_scrollbars(&self) -> bool {
        self.with_common(|common| common.auto_hide_scrollbars)
    }

    fn write_credentials(&self, url: &str, username: &str, password: &[u8]) -> Task<Result<()>> {
        let url = url.to_string();
        let username = username.to_string();
        let password = password.to_vec();
        self.with_keyring(false, |keyring| async move {
            keyring
                .create_item(
                    KEYRING_LABEL,
                    &vec![("url", &url), ("username", &username)],
                    password,
                    true,
                )
                .await?;
            Ok(())
        })
    }

    fn read_credentials(&self, url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        let url = url.to_string();
        self.with_keyring(true, |keyring| async move {
            let items = keyring.search_items(&vec![("url", &url)]).await?;
            let mut label_error = None;

            for item in items.into_iter() {
                let label = match item.label().await {
                    Ok(label) => label,
                    Err(error) => {
                        let error = anyhow!(error);
                        match classify_keyring_error(&error) {
                            KeyringErrorKind::Connection => return Err(error),
                            KeyringErrorKind::MissingObject => {}
                            KeyringErrorKind::Other => {
                                label_error.get_or_insert(error);
                            }
                        }
                        continue;
                    }
                };
                if label == KEYRING_LABEL {
                    let attributes = item.attributes().await?;
                    let username = attributes
                        .get("username")
                        .context("Cannot find username in stored credentials")?;
                    item.unlock().await?;
                    let secret = item.secret().await?;

                    // we lose the zeroizing capabilities at this boundary,
                    // a current limitation GPUI's credentials api
                    return Ok(Some((username.to_string(), secret.to_vec())));
                } else {
                    continue;
                }
            }
            label_error.map_or(Ok(None), Err)
        })
    }

    fn delete_credentials(&self, url: &str) -> Task<Result<()>> {
        let url = url.to_string();
        self.with_keyring(false, |keyring| async move {
            let items = keyring.search_items(&vec![("url", &url)]).await?;
            let mut label_error = None;

            for item in items.into_iter() {
                let label = match item.label().await {
                    Ok(label) => label,
                    Err(error) => {
                        let error = anyhow!(error);
                        match classify_keyring_error(&error) {
                            KeyringErrorKind::Connection => return Err(error),
                            KeyringErrorKind::MissingObject => {}
                            KeyringErrorKind::Other => {
                                label_error.get_or_insert(error);
                            }
                        }
                        continue;
                    }
                };
                if label == KEYRING_LABEL {
                    item.delete().await?;
                    return Ok(());
                }
            }

            label_error.map_or(Ok(()), Err)
        })
    }

    fn window_appearance(&self) -> WindowAppearance {
        self.with_common(|common| common.appearance)
    }

    fn button_layout(&self) -> Option<WindowButtonLayout> {
        Some(self.with_common(|common| common.button_layout))
    }

    fn register_url_scheme(&self, _: &str) -> Task<anyhow::Result<()>> {
        Task::ready(Err(anyhow!("register_url_scheme unimplemented")))
    }

    fn write_to_primary(&self, item: ClipboardItem) {
        self.connection.borrow().write_to_primary(item)
    }

    fn write_to_clipboard(&self, item: ClipboardItem) {
        self.connection.borrow().write_to_clipboard(item)
    }

    fn read_from_primary(&self) -> Option<ClipboardItem> {
        self.connection.borrow().read_from_primary()
    }

    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        self.connection.borrow().read_from_clipboard()
    }

    fn add_recent_document(&self, _path: &Path) {}
}

/// Opens `target` with the first of the desktop's launchers that succeeds. Returns whether one
/// did.
#[cfg(any(feature = "wayland", feature = "x11"))]
async fn run_open_commands(
    target: &std::ffi::OsStr,
    activation_token: Option<&str>,
    environment: &GraphicalEnvironment,
) -> bool {
    for mut command in open::commands(target) {
        environment.apply_to(&mut command);
        if let Some(token) = activation_token {
            command.env("XDG_ACTIVATION_TOKEN", token);
        }
        let program = format!("{:?}", command.get_program());
        match smol::process::Command::from(command).spawn() {
            Ok(mut child) => match child.status().await {
                Ok(status) if status.success() => return true,
                Ok(status) => log::error!("Command {} exited with status: {}", program, status),
                Err(error) => log::error!("Failed to get status from {}: {}", program, error),
            },
            Err(error) => log::error!("Failed to open with {}: {}", program, error),
        }
    }
    false
}

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn open_uri_internal(
    executor: BackgroundExecutor,
    uri: &str,
    activation_token: Option<String>,
    environment: GraphicalEnvironment,
) {
    if let Some(uri) = ashpd::Uri::parse(uri).log_err() {
        executor
            .spawn(async move {
                let opened = run_open_commands(
                    uri.as_str().as_ref(),
                    activation_token.as_deref(),
                    &environment,
                )
                .await;
                if !opened {
                    match ashpd::desktop::open_uri::OpenFileRequest::default()
                        .activation_token(activation_token.map(ashpd::ActivationToken::from))
                        .send_uri(&uri)
                        .await
                        .and_then(|e| e.response())
                    {
                        Ok(()) => {}
                        Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) => {}
                        Err(e) => {
                            log::error!("Failed to open with dbus: {}", e);
                        }
                    }
                }
            })
            .detach();
    }
}

#[cfg(any(feature = "x11", feature = "wayland"))]
pub(super) fn reveal_path_internal(
    executor: BackgroundExecutor,
    path: PathBuf,
    activation_token: Option<String>,
    environment: GraphicalEnvironment,
) {
    executor
        .spawn(async move {
            if let Some(dir) = File::open(path.clone()).log_err() {
                match ashpd::desktop::open_uri::OpenDirectoryRequest::default()
                    .activation_token(activation_token.clone().map(ashpd::ActivationToken::from))
                    .send(&dir.as_fd())
                    .await
                {
                    Ok(_) => return,
                    Err(e) => log::error!("Failed to open with dbus: {}", e),
                }
                let directory = if path.is_dir() {
                    path.as_path()
                } else {
                    path.parent().unwrap_or(Path::new(""))
                };
                run_open_commands(
                    directory.as_os_str(),
                    activation_token.as_deref(),
                    &environment,
                )
                .await;
            }
        })
        .detach();
}

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn is_within_click_distance(a: Point<Pixels>, b: Point<Pixels>) -> bool {
    let diff = a - b;
    diff.x.abs() <= DOUBLE_CLICK_DISTANCE && diff.y.abs() <= DOUBLE_CLICK_DISTANCE
}

/// Creates an XKB context for keymaps supplied by Wayland or X11.
///
/// Server keymaps are already resolved and need no local keyboard definitions.
/// Loading default include paths can fail on systems without those files.
#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn new_xkb_context() -> anyhow::Result<xkb::Context> {
    validate_xkb_context(xkb::Context::new(xkb::CONTEXT_NO_DEFAULT_INCLUDES))
}

#[cfg(any(feature = "wayland", feature = "x11"))]
fn validate_xkb_context(context: xkb::Context) -> anyhow::Result<xkb::Context> {
    ensure!(
        !context.get_raw_ptr().is_null(),
        "libxkbcommon failed to create an XKB context"
    );
    Ok(context)
}

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn get_xkb_compose_state(cx: &xkb::Context) -> Option<xkb::compose::State> {
    let mut locales = Vec::default();
    if let Some(locale) = env::var_os("LC_CTYPE") {
        locales.push(locale);
    }
    locales.push(OsString::from("C"));
    let mut state: Option<xkb::compose::State> = None;
    for locale in locales {
        if let Ok(table) =
            xkb::compose::Table::new_from_locale(cx, &locale, xkb::compose::COMPILE_NO_FLAGS)
        {
            state = Some(xkb::compose::State::new(
                &table,
                xkb::compose::STATE_NO_FLAGS,
            ));
            break;
        }
    }
    state
}

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) const PIPE_READ_TIMEOUT: Duration = Duration::from_secs(4);

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn read_fd_with_timeout(
    mut fd: filedescriptor::FileDescriptor,
    timeout: Duration,
) -> Result<Vec<u8>> {
    fd.set_non_blocking(true)?;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let mut poll_fds = [filedescriptor::pollfd {
            fd: fd.as_raw_fd(),
            events: filedescriptor::POLLIN,
            revents: 0,
        }];
        let ready = match filedescriptor::poll(&mut poll_fds, Some(timeout)) {
            Ok(ready) => ready,
            Err(filedescriptor::Error::Poll(err))
                if err.kind() == std::io::ErrorKind::Interrupted =>
            {
                continue;
            }
            Err(err) => return Err(err.into()),
        };
        if ready == 0 {
            anyhow::bail!("timed out waiting for data on pipe after {timeout:?}");
        }
        match fd.read(&mut chunk) {
            Ok(0) => return Ok(buffer),
            Ok(len) => buffer.extend_from_slice(&chunk[..len]),
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err.into()),
        }
    }
}

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) const DEFAULT_CURSOR_ICON_NAME: &str = "left_ptr";

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn cursor_style_to_icon_names(style: CursorStyle) -> &'static [&'static str] {
    // Based on cursor names from chromium:
    // https://github.com/chromium/chromium/blob/d3069cf9c973dc3627fa75f64085c6a86c8f41bf/ui/base/cursor/cursor_factory.cc#L113
    match style {
        CursorStyle::Arrow => &[DEFAULT_CURSOR_ICON_NAME],
        CursorStyle::IBeam => &["text", "xterm"],
        CursorStyle::Crosshair => &["crosshair", "cross"],
        CursorStyle::ClosedHand => &["closedhand", "grabbing", "hand2"],
        CursorStyle::OpenHand => &["openhand", "grab", "hand1"],
        CursorStyle::PointingHand => &["pointer", "hand", "hand2"],
        CursorStyle::ResizeLeft => &["w-resize", "left_side"],
        CursorStyle::ResizeRight => &["e-resize", "right_side"],
        CursorStyle::ResizeLeftRight => &["ew-resize", "sb_h_double_arrow"],
        CursorStyle::ResizeUp => &["n-resize", "top_side"],
        CursorStyle::ResizeDown => &["s-resize", "bottom_side"],
        CursorStyle::ResizeUpDown => &["sb_v_double_arrow", "ns-resize"],
        CursorStyle::ResizeUpLeftDownRight => &["size_fdiag", "bd_double_arrow", "nwse-resize"],
        CursorStyle::ResizeUpRightDownLeft => &["size_bdiag", "nesw-resize", "fd_double_arrow"],
        CursorStyle::ResizeColumn => &["col-resize", "sb_h_double_arrow"],
        CursorStyle::ResizeRow => &["row-resize", "sb_v_double_arrow"],
        CursorStyle::IBeamCursorForVerticalLayout => &["vertical-text"],
        CursorStyle::OperationNotAllowed => &["not-allowed", "crossed_circle"],
        CursorStyle::DragLink => &["alias"],
        CursorStyle::DragCopy => &["copy"],
        CursorStyle::ContextualMenu => &["context-menu"],
    }
}

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn log_cursor_icon_warning(message: impl std::fmt::Display) {
    if let Ok(xcursor_path) = env::var("XCURSOR_PATH") {
        log::warn!(
            "{:#}\ncursor icon loading may be failing if XCURSOR_PATH environment variable is invalid. \
                    XCURSOR_PATH overrides the default icon search. Its current value is '{}'",
            message,
            xcursor_path
        );
    } else {
        log::warn!("{:#}", message);
    }
}

#[cfg(any(feature = "wayland", feature = "x11"))]
fn guess_ascii(keycode: Keycode, shift: bool) -> Option<char> {
    let c = match (keycode.raw(), shift) {
        (24, _) => 'q',
        (25, _) => 'w',
        (26, _) => 'e',
        (27, _) => 'r',
        (28, _) => 't',
        (29, _) => 'y',
        (30, _) => 'u',
        (31, _) => 'i',
        (32, _) => 'o',
        (33, _) => 'p',
        (34, false) => '[',
        (34, true) => '{',
        (35, false) => ']',
        (35, true) => '}',
        (38, _) => 'a',
        (39, _) => 's',
        (40, _) => 'd',
        (41, _) => 'f',
        (42, _) => 'g',
        (43, _) => 'h',
        (44, _) => 'j',
        (45, _) => 'k',
        (46, _) => 'l',
        (47, false) => ';',
        (47, true) => ':',
        (48, false) => '\'',
        (48, true) => '"',
        (49, false) => '`',
        (49, true) => '~',
        (51, false) => '\\',
        (51, true) => '|',
        (52, _) => 'z',
        (53, _) => 'x',
        (54, _) => 'c',
        (55, _) => 'v',
        (56, _) => 'b',
        (57, _) => 'n',
        (58, _) => 'm',
        (59, false) => ',',
        (59, true) => '>',
        (60, false) => '.',
        (60, true) => '<',
        (61, false) => '/',
        (61, true) => '?',

        _ => return None,
    };

    Some(c)
}

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn keystroke_from_xkb(
    state: &State,
    mut modifiers: gpui::Modifiers,
    keycode: Keycode,
) -> gpui::Keystroke {
    let key_utf32 = state.key_get_utf32(keycode);
    let key_utf8 = state.key_get_utf8(keycode);
    let key_sym = state.key_get_one_sym(keycode);

    let key = match key_sym {
        Keysym::Return => "enter".to_owned(),
        Keysym::Prior => "pageup".to_owned(),
        Keysym::Next => "pagedown".to_owned(),
        Keysym::ISO_Left_Tab => "tab".to_owned(),
        Keysym::KP_Prior => "pageup".to_owned(),
        Keysym::KP_Next => "pagedown".to_owned(),
        Keysym::XF86_Back => "back".to_owned(),
        Keysym::XF86_Forward => "forward".to_owned(),
        Keysym::XF86_Cut => "cut".to_owned(),
        Keysym::XF86_Copy => "copy".to_owned(),
        Keysym::XF86_Paste => "paste".to_owned(),
        Keysym::XF86_New => "new".to_owned(),
        Keysym::XF86_Open => "open".to_owned(),
        Keysym::XF86_Save => "save".to_owned(),

        Keysym::comma => ",".to_owned(),
        Keysym::period => ".".to_owned(),
        Keysym::less => "<".to_owned(),
        Keysym::greater => ">".to_owned(),
        Keysym::slash => "/".to_owned(),
        Keysym::question => "?".to_owned(),

        Keysym::semicolon => ";".to_owned(),
        Keysym::colon => ":".to_owned(),
        Keysym::apostrophe => "'".to_owned(),
        Keysym::quotedbl => "\"".to_owned(),

        Keysym::bracketleft => "[".to_owned(),
        Keysym::braceleft => "{".to_owned(),
        Keysym::bracketright => "]".to_owned(),
        Keysym::braceright => "}".to_owned(),
        Keysym::backslash => "\\".to_owned(),
        Keysym::bar => "|".to_owned(),

        Keysym::grave => "`".to_owned(),
        Keysym::asciitilde => "~".to_owned(),
        Keysym::exclam => "!".to_owned(),
        Keysym::at => "@".to_owned(),
        Keysym::numbersign => "#".to_owned(),
        Keysym::dollar => "$".to_owned(),
        Keysym::percent => "%".to_owned(),
        Keysym::asciicircum => "^".to_owned(),
        Keysym::ampersand => "&".to_owned(),
        Keysym::asterisk => "*".to_owned(),
        Keysym::parenleft => "(".to_owned(),
        Keysym::parenright => ")".to_owned(),
        Keysym::minus => "-".to_owned(),
        Keysym::underscore => "_".to_owned(),
        Keysym::equal => "=".to_owned(),
        Keysym::plus => "+".to_owned(),
        Keysym::space => "space".to_owned(),
        Keysym::BackSpace => "backspace".to_owned(),
        Keysym::Tab => "tab".to_owned(),
        Keysym::Delete => "delete".to_owned(),
        Keysym::Escape => "escape".to_owned(),

        Keysym::Left => "left".to_owned(),
        Keysym::Right => "right".to_owned(),
        Keysym::Up => "up".to_owned(),
        Keysym::Down => "down".to_owned(),
        Keysym::Home => "home".to_owned(),
        Keysym::End => "end".to_owned(),
        Keysym::Insert => "insert".to_owned(),

        _ => {
            let name = xkb::keysym_get_name(key_sym).to_lowercase();
            if key_sym.is_keypad_key() {
                name.replace("kp_", "")
            } else if let Some(key) = key_utf8.chars().next()
                && key_utf8.len() == 1
                && key.is_ascii()
            {
                if key.is_ascii_graphic() {
                    key_utf8.to_lowercase()
                // map ctrl-a to `a`
                // ctrl-0..9 may emit control codes like ctrl-[, but
                // we don't want to map them to `[`
                } else if key_utf32 <= 0x1f
                    && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
                {
                    ((key_utf32 as u8 + 0x40) as char)
                        .to_ascii_lowercase()
                        .to_string()
                } else {
                    name
                }
            } else if let Some(key_en) = guess_ascii(keycode, modifiers.shift) {
                String::from(key_en)
            } else {
                name
            }
        }
    };

    if modifiers.shift {
        // we only include the shift for upper-case letters by convention,
        // so don't include for numbers and symbols, but do include for
        // tab/enter, etc.
        if key.chars().count() == 1 && key.to_lowercase() == key.to_uppercase() {
            modifiers.shift = false;
        }
    }

    // Ignore control characters (and DEL) for the purposes of key_char
    let key_char =
        (key_utf32 >= 32 && key_utf32 != 127 && !key_utf8.is_empty()).then_some(key_utf8);

    gpui::Keystroke {
        modifiers,
        key,
        key_char,
    }
}

/**
 * Returns which symbol the dead key represents
 * <https://developer.mozilla.org/en-US/docs/Web/API/UI_Events/Keyboard_event_key_values#dead_keycodes_for_linux>
 */
#[cfg(any(feature = "wayland", feature = "x11"))]
pub fn keystroke_underlying_dead_key(keysym: Keysym) -> Option<String> {
    match keysym {
        Keysym::dead_grave => Some("`".to_owned()),
        Keysym::dead_acute => Some("´".to_owned()),
        Keysym::dead_circumflex => Some("^".to_owned()),
        Keysym::dead_tilde => Some("~".to_owned()),
        Keysym::dead_macron => Some("¯".to_owned()),
        Keysym::dead_breve => Some("˘".to_owned()),
        Keysym::dead_abovedot => Some("˙".to_owned()),
        Keysym::dead_diaeresis => Some("¨".to_owned()),
        Keysym::dead_abovering => Some("˚".to_owned()),
        Keysym::dead_doubleacute => Some("˝".to_owned()),
        Keysym::dead_caron => Some("ˇ".to_owned()),
        Keysym::dead_cedilla => Some("¸".to_owned()),
        Keysym::dead_ogonek => Some("˛".to_owned()),
        Keysym::dead_iota => Some("ͅ".to_owned()),
        Keysym::dead_voiced_sound => Some("゙".to_owned()),
        Keysym::dead_semivoiced_sound => Some("゚".to_owned()),
        Keysym::dead_belowdot => Some("̣̣".to_owned()),
        Keysym::dead_hook => Some("̡".to_owned()),
        Keysym::dead_horn => Some("̛".to_owned()),
        Keysym::dead_stroke => Some("̶̶".to_owned()),
        Keysym::dead_abovecomma => Some("̓̓".to_owned()),
        Keysym::dead_abovereversedcomma => Some("ʽ".to_owned()),
        Keysym::dead_doublegrave => Some("̏".to_owned()),
        Keysym::dead_belowring => Some("˳".to_owned()),
        Keysym::dead_belowmacron => Some("̱".to_owned()),
        Keysym::dead_belowcircumflex => Some("ꞈ".to_owned()),
        Keysym::dead_belowtilde => Some("̰".to_owned()),
        Keysym::dead_belowbreve => Some("̮".to_owned()),
        Keysym::dead_belowdiaeresis => Some("̤".to_owned()),
        Keysym::dead_invertedbreve => Some("̯".to_owned()),
        Keysym::dead_belowcomma => Some("̦".to_owned()),
        Keysym::dead_currency => None,
        Keysym::dead_lowline => None,
        Keysym::dead_aboveverticalline => None,
        Keysym::dead_belowverticalline => None,
        Keysym::dead_longsolidusoverlay => None,
        Keysym::dead_a => None,
        Keysym::dead_A => None,
        Keysym::dead_e => None,
        Keysym::dead_E => None,
        Keysym::dead_i => None,
        Keysym::dead_I => None,
        Keysym::dead_o => None,
        Keysym::dead_O => None,
        Keysym::dead_u => None,
        Keysym::dead_U => None,
        Keysym::dead_small_schwa => Some("ə".to_owned()),
        Keysym::dead_capital_schwa => Some("Ə".to_owned()),
        Keysym::dead_greek => None,
        _ => None,
    }
}
#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn modifiers_from_xkb(keymap_state: &State) -> gpui::Modifiers {
    let shift = keymap_state.mod_name_is_active(xkb::MOD_NAME_SHIFT, xkb::STATE_MODS_EFFECTIVE);
    let alt = keymap_state.mod_name_is_active(xkb::MOD_NAME_ALT, xkb::STATE_MODS_EFFECTIVE);
    let control = keymap_state.mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_EFFECTIVE);
    let platform = keymap_state.mod_name_is_active(xkb::MOD_NAME_LOGO, xkb::STATE_MODS_EFFECTIVE);
    gpui::Modifiers {
        shift,
        alt,
        control,
        platform,
        function: false,
    }
}

#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn capslock_from_xkb(keymap_state: &State) -> gpui::Capslock {
    let on = keymap_state.mod_name_is_active(xkb::MOD_NAME_CAPS, xkb::STATE_MODS_EFFECTIVE);
    gpui::Capslock { on }
}

/// Resolve a Linux `dev_t` to PCI vendor/device IDs via sysfs, returning a
/// [`CompositorGpuHint`] that the GPU adapter selection code can use to
/// prioritize the compositor's rendering device.
#[cfg(any(feature = "wayland", feature = "x11"))]
pub(super) fn compositor_gpu_hint_from_dev_t(dev: u64) -> Option<gpui_wgpu::CompositorGpuHint> {
    fn dev_major(dev: u64) -> u32 {
        ((dev >> 8) & 0xfff) as u32 | (((dev >> 32) & !0xfff) as u32)
    }

    fn dev_minor(dev: u64) -> u32 {
        (dev & 0xff) as u32 | (((dev >> 12) & !0xff) as u32)
    }

    fn read_sysfs_hex_id(path: &str) -> Option<u32> {
        let content = std::fs::read_to_string(path).ok()?;
        let trimmed = content.trim().strip_prefix("0x").unwrap_or(content.trim());
        u32::from_str_radix(trimmed, 16).ok()
    }

    let major = dev_major(dev);
    let minor = dev_minor(dev);

    let vendor_path = format!("/sys/dev/char/{major}:{minor}/device/vendor");
    let device_path = format!("/sys/dev/char/{major}:{minor}/device/device");

    let vendor_id = read_sysfs_hex_id(&vendor_path)?;
    let device_id = read_sysfs_hex_id(&device_path)?;

    log::info!(
        "Compositor GPU hint: vendor={:#06x}, device={:#06x} (from dev {major}:{minor})",
        vendor_id,
        device_id,
    );

    Some(gpui_wgpu::CompositorGpuHint {
        vendor_id,
        device_id,
    })
}

#[cfg(any(feature = "wayland", feature = "x11"))]
async fn inhibit_idle_sleep(reason: String, executor: BackgroundExecutor) -> Result<ActivityGuard> {
    let proxy = InhibitProxy::new()
        .await
        .context("Idle sleep prevention portal is unavailable")?;
    let request = proxy
        .inhibit(
            None,
            BitFlags::from(InhibitFlags::Suspend),
            InhibitOptions::default().set_reason(reason.as_str()),
        )
        .await
        .context("Failed to request idle sleep prevention")?;
    request
        .response()
        .context("Idle sleep prevention request was rejected")?;
    Ok(release_on_drop(request, executor))
}

#[cfg(any(feature = "wayland", feature = "x11"))]
fn release_on_drop(request: Request<()>, executor: BackgroundExecutor) -> ActivityGuard {
    ActivityGuard::new(move || {
        executor
            .spawn(async move {
                request
                    .close()
                    .await
                    .context("Failed to release idle sleep prevention")
                    .log_err();
            })
            .detach();
    })
}

#[cfg(any(feature = "wayland", feature = "x11"))]
async fn await_idle_sleep_prevention(
    guard_rx: oneshot::Receiver<Result<ActivityGuard>>,
    executor: &BackgroundExecutor,
) -> Result<ActivityGuard> {
    match futures::future::select(guard_rx, executor.timer(Duration::from_secs(10))).await {
        futures::future::Either::Left((Ok(result), _)) => result,
        futures::future::Either::Left((Err(_), _)) => {
            Err(anyhow!("Idle sleep prevention request was abandoned"))
        }
        futures::future::Either::Right(_) => Err(anyhow!(
            "Idle sleep prevention acquisition timed out after 10 seconds"
        )),
    }
}

const SECRET_SERVICE_NAME: oo7::zbus::names::WellKnownName<'static> =
    oo7::zbus::names::WellKnownName::from_static_str_unchecked("org.freedesktop.secrets");

#[derive(Default)]
struct SecretServiceCache {
    connecting: Option<BoxFuture<'static, Result<SecretServiceKeyring, oo7::dbus::Error>>>,
    connected: Option<SecretServiceKeyring>,
}

struct SecretServiceKeyring {
    keyring: Arc<oo7::Keyring>,
    collection_path: oo7::zbus::zvariant::OwnedObjectPath,
    bus: oo7::zbus::fdo::DBusProxy<'static>,
    owner: oo7::zbus::names::OwnedUniqueName,
    disconnected: BoxFuture<'static, oo7::dbus::Error>,
}

impl SecretServiceKeyring {
    async fn prepare(
        keyring: &oo7::Keyring,
        expected_collection: &oo7::zbus::zvariant::OwnedObjectPath,
        bus: &oo7::zbus::fdo::DBusProxy<'_>,
        owner: &oo7::zbus::names::OwnedUniqueName,
    ) -> oo7::Result<()> {
        check_secret_service_owner(bus, owner).await?;
        let collection_path = bus
            .inner()
            .connection()
            .call_method(
                Some(owner.as_str()),
                "/org/freedesktop/secrets",
                Some("org.freedesktop.Secret.Service"),
                "ReadAlias",
                &"default",
            )
            .await
            .map_err(oo7::dbus::ServiceError::from)
            .map_err(oo7::dbus::Error::from)?
            .body()
            .deserialize::<oo7::zbus::zvariant::OwnedObjectPath>()
            .map_err(oo7::dbus::Error::from)?;
        if collection_path != *expected_collection {
            return Err(oo7::dbus::Error::from(oo7::dbus::ServiceError::NoSession(
                "Secret Service default collection changed".to_owned(),
            ))
            .into());
        }
        keyring.unlock().await?;
        check_secret_service_owner(bus, owner).await?;
        Ok(())
    }

    async fn connect() -> Result<Self, oo7::dbus::Error> {
        let connection = oo7::zbus::connection::Builder::session()?
            .method_timeout(Duration::from_secs(30))
            .build()
            .await?;
        let bus = oo7::zbus::fdo::DBusProxy::new(&connection).await?;
        let owner = match bus
            .get_name_owner(oo7::zbus::names::BusName::WellKnown(SECRET_SERVICE_NAME))
            .await
        {
            Err(oo7::zbus::fdo::Error::NameHasNoOwner(_)) => {
                bus.start_service_by_name(SECRET_SERVICE_NAME, 0).await?;
                bus.get_name_owner(oo7::zbus::names::BusName::WellKnown(SECRET_SERVICE_NAME))
                    .await?
            }
            result => result?,
        };
        let service_name = SECRET_SERVICE_NAME;
        let rule = oo7::zbus::MatchRule::builder()
            .msg_type(oo7::zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")?
            .path("/org/freedesktop/DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .add_arg(service_name.as_str())?
            .add_arg(owner.as_str())?
            .build();
        let mut changes =
            oo7::zbus::MessageStream::for_match_rule(rule, &connection, Some(1)).await?;
        let mut disconnected = async move {
            match changes.next().await {
                Some(Ok(_)) => {
                    oo7::dbus::ServiceError::NoSession("Secret Service owner changed".to_owned())
                        .into()
                }
                Some(Err(error)) => oo7::dbus::ServiceError::NoSession(format!(
                    "Secret Service owner monitor failed: {error}"
                ))
                .into(),
                None => oo7::zbus::Error::from(std::io::Error::new(
                    ErrorKind::NotConnected,
                    "Secret Service owner monitor disconnected",
                ))
                .into(),
            }
        }
        .boxed();
        let collection = await_secret_service(&mut disconnected, async {
            check_secret_service_owner(&bus, &owner).await?;
            let service = oo7::dbus::Service::new().await?;
            check_secret_service_owner(&bus, &owner).await?;
            service.default_collection().await
        })
        .await?;
        let collection_path = oo7::zbus::zvariant::OwnedObjectPath::from(collection.path().clone());
        Ok(Self {
            keyring: Arc::new(oo7::Keyring::DBus(collection)),
            collection_path,
            bus,
            owner,
            disconnected,
        })
    }
}

async fn await_secret_service<R, E>(
    disconnected: &mut BoxFuture<'static, oo7::dbus::Error>,
    operation: impl Future<Output = Result<R, E>>,
) -> Result<R, E>
where
    E: From<oo7::dbus::Error>,
{
    futures::pin_mut!(operation);
    match futures::future::select(disconnected, operation).await {
        futures::future::Either::Left((error, _)) => Err(E::from(error)),
        futures::future::Either::Right((result, _)) => result,
    }
}

async fn check_secret_service_owner(
    bus: &oo7::zbus::fdo::DBusProxy<'_>,
    expected_owner: &oo7::zbus::names::OwnedUniqueName,
) -> Result<(), oo7::dbus::Error> {
    let owner = bus
        .get_name_owner(oo7::zbus::names::BusName::WellKnown(SECRET_SERVICE_NAME))
        .await?;
    if owner != *expected_owner {
        return Err(
            oo7::dbus::ServiceError::NoSession("Secret Service owner changed".to_owned()).into(),
        );
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum KeyringErrorKind {
    Connection,
    MissingObject,
    Other,
}

fn classify_keyring_error(error: &anyhow::Error) -> KeyringErrorKind {
    let error = match error.downcast_ref::<oo7::Error>() {
        Some(oo7::Error::DBus(error)) => Some(error),
        _ => error.downcast_ref::<oo7::dbus::Error>(),
    };
    let mut error = match error {
        Some(oo7::dbus::Error::Service(oo7::dbus::ServiceError::NoSession(_))) => {
            return KeyringErrorKind::Connection;
        }
        Some(oo7::dbus::Error::ZBus(error))
        | Some(oo7::dbus::Error::Service(oo7::dbus::ServiceError::ZBus(error))) => error,
        _ => return KeyringErrorKind::Other,
    };
    let name = loop {
        match error {
            oo7::zbus::Error::InputOutput(error) => {
                return match error.kind() {
                    ErrorKind::BrokenPipe
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::ConnectionReset
                    | ErrorKind::NotConnected
                    | ErrorKind::UnexpectedEof
                    | ErrorKind::TimedOut => KeyringErrorKind::Connection,
                    _ => KeyringErrorKind::Other,
                };
            }
            oo7::zbus::Error::MethodError(name, ..) => break name.as_str(),
            oo7::zbus::Error::FDO(inner) => match inner.as_ref() {
                oo7::zbus::fdo::Error::ZBus(inner) => error = inner,
                inner => {
                    return match inner {
                        oo7::zbus::fdo::Error::UnknownObject(_) => KeyringErrorKind::MissingObject,
                        oo7::zbus::fdo::Error::NameHasNoOwner(_)
                        | oo7::zbus::fdo::Error::ServiceUnknown(_)
                        | oo7::zbus::fdo::Error::Disconnected(_)
                        | oo7::zbus::fdo::Error::NoReply(_) => KeyringErrorKind::Connection,
                        _ => KeyringErrorKind::Other,
                    };
                }
            },
            _ => return KeyringErrorKind::Other,
        }
    };
    match name {
        "org.freedesktop.DBus.Error.UnknownObject"
        | "org.qtproject.QtDBus.Error.InvalidObjectPath" => KeyringErrorKind::MissingObject,
        "org.freedesktop.DBus.Error.NameHasNoOwner"
        | "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.Disconnected"
        | "org.freedesktop.DBus.Error.NoReply" => KeyringErrorKind::Connection,
        _ => KeyringErrorKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Point, px};
    use parking_lot::Mutex;

    #[test]
    fn test_keyring_connection_errors() {
        for wrapped in [false, true] {
            for (error, expected) in [
                (
                    oo7::dbus::Error::Service(oo7::dbus::ServiceError::NoSession(String::new())),
                    KeyringErrorKind::Connection,
                ),
                (
                    oo7::dbus::Error::ZBus(oo7::zbus::Error::InputOutput(Arc::new(
                        std::io::Error::from(ErrorKind::BrokenPipe),
                    ))),
                    KeyringErrorKind::Connection,
                ),
                (
                    oo7::dbus::Error::Service(oo7::dbus::ServiceError::ZBus(
                        oo7::zbus::Error::FDO(Box::new(oo7::zbus::fdo::Error::UnknownObject(
                            String::new(),
                        ))),
                    )),
                    KeyringErrorKind::MissingObject,
                ),
                (
                    oo7::dbus::Error::ZBus(oo7::zbus::Error::FDO(Box::new(
                        oo7::zbus::fdo::Error::ZBus(oo7::zbus::Error::InputOutput(Arc::new(
                            std::io::Error::from(ErrorKind::BrokenPipe),
                        ))),
                    ))),
                    KeyringErrorKind::Connection,
                ),
                (
                    oo7::dbus::Error::Service(oo7::dbus::ServiceError::IsLocked(String::new())),
                    KeyringErrorKind::Other,
                ),
                (oo7::dbus::Error::Dismissed, KeyringErrorKind::Other),
            ] {
                let error = if wrapped {
                    anyhow!(oo7::Error::DBus(error))
                } else {
                    anyhow!(error)
                }
                .context("keyring operation failed");
                assert_eq!(classify_keyring_error(&error), expected, "{error:#}");
            }
        }
    }

    mod keyring {
        use super::*;
        use oo7::zbus::{
            self, Connection, fdo,
            message::Header,
            names::BusName,
            object_server::{ResponseDispatchNotifier, SignalEmitter},
            zvariant::{OwnedObjectPath, OwnedValue, Str, Value},
        };
        use std::{collections::HashMap, process::Command};

        #[test]
        fn reuses_connection_after_concurrent_calls_and_ordinary_errors() {
            run_on_private_bus(
                "reuses_connection_after_concurrent_calls_and_ordinary_errors",
                async {
                    let state = Arc::new(Mutex::new(ServiceState::default()));
                    let daemon = start_service(state.clone()).await;
                    let platform = LinuxPlatform::new(WindowingModes::HEADLESS);
                    let (opened, opening) = oneshot::channel();
                    let (release, released) = oneshot::channel();
                    state.lock().open_gate = Some((opened, released));
                    let reads = (0..3)
                        .map(|_| platform.read_credentials("keyring-test"))
                        .collect::<Vec<_>>();
                    opening.await.expect("session opening started");
                    assert_eq!(state.lock().opened, 1);
                    drop(reads);
                    let reads = (0..3)
                        .map(|_| platform.read_credentials("keyring-test"))
                        .collect::<Vec<_>>();
                    release.send(()).expect("release session opening");
                    for result in futures::future::join_all(reads).await {
                        assert_eq!(result.expect("initial read"), None);
                    }
                    assert_eq!(state.lock().opened, 1);
                    platform
                        .write_credentials("keyring-test", "user", b"secret")
                        .await
                        .expect("write credential");
                    state.lock().fail_label = true;
                    platform
                        .read_credentials("keyring-test")
                        .await
                        .expect_err("label access denied");
                    state.lock().fail_label = false;
                    state.lock().missing_username = true;
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect_err("malformed credential")
                            .to_string(),
                        "Cannot find username in stored credentials"
                    );
                    state.lock().missing_username = false;
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("read"),
                        Some(("user".to_owned(), b"secret".to_vec()))
                    );
                    assert_eq!(state.lock().opened, 1);
                    platform
                        .with_keyring(true, |_| async {
                            Err::<(), _>(anyhow!(oo7::dbus::Error::Dismissed))
                        })
                        .await
                        .expect_err("dismissed operation");
                    platform
                        .read_credentials("keyring-test")
                        .await
                        .expect("read");
                    assert_eq!(state.lock().opened, 1);
                    daemon.close().await.expect("close service");
                },
            );
        }

        #[test]
        fn skips_unreadable_candidates_when_reading_and_deleting_credentials() {
            run_on_private_bus(
                "skips_unreadable_candidates_when_reading_and_deleting_credentials",
                async {
                    let state = Arc::new(Mutex::new(ServiceState::default()));
                    let daemon = start_service(state.clone()).await;
                    let platform = LinuxPlatform::new(WindowingModes::HEADLESS);
                    for missing_item in [false, true] {
                        state.lock().unreadable_item_missing = missing_item;
                        for unreadable_first in [true, false] {
                            state.lock().unreadable_item_first = Some(unreadable_first);
                            platform
                                .write_credentials("keyring-test", "user", b"secret")
                                .await
                                .expect("write credential");
                            assert_eq!(
                                platform
                                    .read_credentials("keyring-test")
                                    .await
                                    .expect("read despite unreadable candidate"),
                                Some(("user".to_owned(), b"secret".to_vec()))
                            );
                            platform
                                .delete_credentials("keyring-test")
                                .await
                                .expect("delete despite unreadable candidate");
                            assert_eq!(state.lock().secret, None);
                            let read = platform.read_credentials("keyring-test").await;
                            let delete = platform.delete_credentials("keyring-test").await;
                            if missing_item {
                                assert_eq!(read.expect("skip missing candidate"), None);
                                delete.expect("skip missing candidate");
                            } else {
                                for error in [
                                    read.expect_err(
                                        "retain label error when no credential can be read",
                                    ),
                                    delete.expect_err(
                                        "retain label error when no credential can be deleted",
                                    ),
                                ] {
                                    let oo7::Error::DBus(oo7::dbus::Error::ZBus(zbus::Error::FDO(
                                        error,
                                    ))) = error
                                        .downcast::<oo7::Error>()
                                        .expect("keyring label error")
                                    else {
                                        panic!("expected a D-Bus property error");
                                    };
                                    let fdo::Error::AccessDenied(message) = *error else {
                                        panic!("expected a label access error");
                                    };
                                    assert_eq!(message, "foreign label access denied");
                                }
                            }
                            assert_eq!(state.lock().opened, 1);
                        }
                    }
                    daemon.close().await.expect("close service");
                },
            );
        }

        #[test]
        fn reconnects_before_reads_and_mutations_after_restart() {
            run_on_private_bus(
                "reconnects_before_reads_and_mutations_after_restart",
                async {
                    let state = Arc::new(Mutex::new(ServiceState::default()));
                    let daemon = start_service(state.clone()).await;
                    let platform = LinuxPlatform::new(WindowingModes::HEADLESS);
                    platform
                        .write_credentials("keyring-test", "user", b"first")
                        .await
                        .expect("initial write");
                    let daemon = restart_service(daemon, state.clone()).await;
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("read after restart"),
                        Some(("user".to_owned(), b"first".to_vec()))
                    );
                    assert_eq!(state.lock().opened, 2);
                    let daemon = restart_service(daemon, state.clone()).await;
                    platform
                        .write_credentials("keyring-test", "user", b"second")
                        .await
                        .expect("write after restart");
                    assert_eq!(state.lock().opened, 3);
                    assert_eq!(state.lock().rejected_sessions, 0);
                    assert_eq!(state.lock().writes, 2);
                    let daemon = restart_service(daemon, state.clone()).await;
                    platform
                        .delete_credentials("keyring-test")
                        .await
                        .expect("delete after restart");
                    assert_eq!(state.lock().opened, 4);
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("deleted"),
                        None
                    );
                    state.lock().default_collection = Some(object_path(
                        "/org/freedesktop/secrets/collection/replacement",
                    ));
                    platform
                        .write_credentials("keyring-test", "user", b"replacement")
                        .await
                        .expect("write after default collection changed");
                    assert_eq!(state.lock().opened, 5);
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("read from replacement collection"),
                        Some(("user".to_owned(), b"replacement".to_vec()))
                    );
                    assert_eq!(state.lock().opened, 5);
                    daemon.close().await.expect("close service");
                },
            );
        }

        #[test]
        fn serializes_recovery_without_replaying_mutations_or_running_canceled_calls() {
            run_on_private_bus(
                "serializes_recovery_without_replaying_mutations_or_running_canceled_calls",
                async {
                    let state = Arc::new(Mutex::new(ServiceState::default()));
                    let daemon = start_service(state.clone()).await;
                    let platform = LinuxPlatform::new(WindowingModes::HEADLESS);
                    state.lock().fail_write_reply = true;
                    platform
                        .write_credentials("keyring-test", "user", b"committed")
                        .await
                        .expect_err("lost write reply");
                    assert_eq!(state.lock().writes, 1);
                    assert_eq!(state.lock().opened, 1);
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("read committed write"),
                        Some(("user".to_owned(), b"committed".to_vec()))
                    );
                    assert_eq!(state.lock().opened, 2);
                    let (entered, entering) = smol::channel::bounded(1);
                    let (release, released) = smol::channel::bounded(1);
                    let delayed_failure = platform.with_keyring(false, move |_| {
                        let entered = entered.clone();
                        let released = released.clone();
                        async move {
                            entered.send(()).await?;
                            released.recv().await?;
                            Err::<(), _>(disconnected())
                        }
                    });
                    entering.recv().await.expect("operation started");
                    let canceled_attempts = Arc::new(Mutex::new(0));
                    let canceled = platform.with_keyring(false, {
                        let canceled_attempts = canceled_attempts.clone();
                        move |_| async move {
                            *canceled_attempts.lock() += 1;
                            Err::<(), _>(disconnected())
                        }
                    });
                    let mut recovery = platform.read_credentials("keyring-test");
                    assert!((&mut recovery).now_or_never().is_none());
                    drop(canceled);
                    assert_eq!(state.lock().opened, 2);
                    release.send(()).await.expect("release delayed failure");
                    delayed_failure.await.expect_err("delayed disconnect");
                    assert_eq!(
                        recovery.await.expect("queued read reconnects"),
                        Some(("user".to_owned(), b"committed".to_vec()))
                    );
                    assert_eq!(*canceled_attempts.lock(), 0);
                    assert_eq!(state.lock().opened, 3);
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("reuse recovered session"),
                        Some(("user".to_owned(), b"committed".to_vec()))
                    );
                    assert_eq!(state.lock().opened, 3);
                    let connection = platform
                        .secret_service
                        .lock()
                        .await
                        .connected
                        .as_ref()
                        .expect("cached connection")
                        .bus
                        .inner()
                        .connection()
                        .clone();
                    connection.close().await.expect("disconnect owner lookup");
                    platform
                        .read_credentials("keyring-test")
                        .await
                        .expect("recover owner lookup");
                    assert_eq!(state.lock().opened, 4);
                    for (read_only, expected_attempts) in [(true, 2), (false, 1)] {
                        let attempts = Arc::new(Mutex::new(0));
                        platform
                            .with_keyring(read_only, {
                                let attempts = attempts.clone();
                                move |_| {
                                    let attempts = attempts.clone();
                                    async move {
                                        *attempts.lock() += 1;
                                        Err::<(), _>(anyhow!(oo7::dbus::Error::Service(
                                            oo7::dbus::ServiceError::NoSession(
                                                "expired session".to_owned()
                                            )
                                        )))
                                    }
                                }
                            })
                            .await
                            .expect_err("expired session");
                        assert_eq!(*attempts.lock(), expected_attempts);
                    }
                    platform
                        .read_credentials("keyring-test")
                        .await
                        .expect("warm session before invalidation");
                    let (opened, writes) = {
                        let mut state = state.lock();
                        state.sessions.clear();
                        (state.opened, state.writes)
                    };
                    let error = platform
                        .write_credentials("keyring-test", "user", b"retry")
                        .await
                        .expect_err("expired Qt session");
                    assert_eq!(state.lock().opened, opened);
                    assert_eq!(state.lock().writes, writes);
                    assert_eq!(state.lock().rejected_sessions, 1);
                    platform
                        .write_credentials("keyring-test", "user", b"retry")
                        .await
                        .expect("write with a fresh session");
                    assert_eq!(state.lock().opened, opened + 1);
                    assert_eq!(state.lock().writes, writes + 1);
                    assert_eq!(state.lock().rejected_sessions, 1);
                    assert_eq!(
                        classify_keyring_error(&error),
                        KeyringErrorKind::MissingObject
                    );
                    daemon.close().await.expect("close service");
                },
            );
        }

        #[test]
        fn rechecks_owner_after_unlock_without_replaying_a_mid_write_restart() {
            run_on_private_bus(
                "rechecks_owner_after_unlock_without_replaying_a_mid_write_restart",
                async {
                    let state = Arc::new(Mutex::new(ServiceState::default()));
                    let daemon = start_service(state.clone()).await;
                    let platform = LinuxPlatform::new(WindowingModes::HEADLESS);
                    platform
                        .read_credentials("keyring-test")
                        .await
                        .expect("initial session");
                    let (entered, entering) = oneshot::channel();
                    let (release, released) = oneshot::channel();
                    state.lock().unlock_gate = Some((entered, released));
                    let write = platform.write_credentials("keyring-test", "user", b"first");
                    entering.await.expect("unlock started");
                    let replacement = replace_service_owner(&daemon, state.clone()).await;
                    release.send(()).expect("release unlock");
                    write.await.expect("reconnect before write");
                    assert_eq!(state.lock().opened, 2);
                    assert_eq!(state.lock().writes, 1);
                    assert_eq!(state.lock().rejected_sessions, 0);
                    daemon.close().await.expect("close previous service");

                    let (entered, entering) = smol::channel::bounded(1);
                    let (release, released) = smol::channel::bounded(1);
                    let write = platform.with_keyring(false, move |keyring| {
                        let entered = entered.clone();
                        let released = released.clone();
                        async move {
                            entered.send(()).await?;
                            released.recv().await?;
                            keyring
                                .create_item(
                                    KEYRING_LABEL,
                                    &[("url", "keyring-test"), ("username", "user")],
                                    b"second".to_vec(),
                                    true,
                                )
                                .await?;
                            Ok(())
                        }
                    });
                    entering.recv().await.expect("mutation started");
                    let replacement = restart_service(replacement, state.clone()).await;
                    let error = write.await.expect_err("restart during mutation");
                    assert!(release.send(()).await.is_err());
                    assert_eq!(classify_keyring_error(&error), KeyringErrorKind::Connection);
                    assert_eq!(state.lock().opened, 2);
                    assert_eq!(state.lock().writes, 1);
                    assert_eq!(state.lock().rejected_sessions, 0);
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("recover on next call"),
                        Some(("user".to_owned(), b"first".to_vec()))
                    );
                    assert_eq!(state.lock().opened, 3);
                    replacement.close().await.expect("close service");
                },
            );
        }

        #[test]
        fn disconnects_canceled_unlock_prompts_and_reuses_completed_sessions() {
            run_on_private_bus(
                "disconnects_canceled_unlock_prompts_and_reuses_completed_sessions",
                async {
                    for target in [
                        "/org/freedesktop/secrets/collection/default",
                        "/org/freedesktop/secrets/item/1",
                    ] {
                        for outcome in [
                            PromptOutcome::CancelBeforeReply,
                            PromptOutcome::CancelAfterReply,
                            PromptOutcome::Complete,
                            PromptOutcome::Dismiss,
                        ] {
                            let state = Arc::new(Mutex::new(ServiceState::default()));
                            let daemon = start_service(state.clone()).await;
                            let bus = fdo::DBusProxy::new(&daemon).await.expect("bus proxy");
                            let platform = LinuxPlatform::new(WindowingModes::HEADLESS);
                            platform
                                .write_credentials("keyring-test", "user", b"secret")
                                .await
                                .expect("seed credential");
                            let (entered, entering) = oneshot::channel();
                            let (release, released) = oneshot::channel();
                            {
                                let mut state = state.lock();
                                state.prompt_target = Some(object_path(target));
                                state.prompt_gate = Some((entered, released));
                                state.defer_session_close = true;
                            }
                            let read = platform.read_credentials("keyring-test");
                            let (caller, reply_dispatched) =
                                entering.await.expect("prompt started");
                            assert_eq!(
                                state.lock().sessions.values().cloned().collect::<Vec<_>>(),
                                vec![caller.clone()],
                                "{target}: {outcome:?}"
                            );
                            let mut disconnected = bus
                                .receive_name_owner_changed_with_args(&[
                                    (0, caller.as_str()),
                                    (2, ""),
                                ])
                                .await
                                .expect("watch prompt caller disconnect");
                            let mut queued = platform.read_credentials("keyring-test");
                            assert!((&mut queued).now_or_never().is_none());
                            assert_eq!(state.lock().opened, 1);
                            let release_reply = async move {
                                release.send(()).expect("release prompt reply");
                                reply_dispatched.await;
                            };
                            futures::pin_mut!(release_reply);
                            if outcome != PromptOutcome::CancelBeforeReply {
                                release_reply.as_mut().await;
                            }
                            let expected_sessions = match outcome {
                                PromptOutcome::CancelBeforeReply
                                | PromptOutcome::CancelAfterReply => {
                                    drop(read);
                                    let signal = disconnected
                                        .next()
                                        .await
                                        .expect("canceled prompt caller disconnected");
                                    let arguments =
                                        signal.args().expect("name owner changed arguments");
                                    assert_eq!(arguments.name().as_str(), caller);
                                    assert_eq!(
                                        arguments.old_owner().as_ref().map(|owner| owner.as_str()),
                                        Some(caller.as_str())
                                    );
                                    assert_eq!(
                                        arguments.new_owner().as_ref().map(|owner| owner.as_str()),
                                        None
                                    );
                                    if outcome == PromptOutcome::CancelBeforeReply {
                                        release_reply.await;
                                    }
                                    2
                                }
                                PromptOutcome::Complete | PromptOutcome::Dismiss => {
                                    let emitter = SignalEmitter::new(
                                        &daemon,
                                        "/org/freedesktop/secrets/prompt/1",
                                    )
                                    .expect("prompt signal emitter");
                                    TestPrompt::completed(
                                        &emitter,
                                        outcome == PromptOutcome::Dismiss,
                                        OwnedValue::try_from(Value::from(vec![object_path(
                                            target,
                                        )]))
                                        .expect("unlocked paths"),
                                    )
                                    .await
                                    .expect("complete prompt");
                                    if outcome == PromptOutcome::Dismiss {
                                        let error = read.await.expect_err("dismissed prompt");
                                        let oo7::Error::DBus(oo7::dbus::Error::Dismissed) =
                                            error.downcast::<oo7::Error>().expect("keyring error")
                                        else {
                                            panic!("expected prompt dismissal");
                                        };
                                    } else {
                                        assert_eq!(
                                            read.await.expect("completed prompt read"),
                                            Some(("user".to_owned(), b"secret".to_vec()))
                                        );
                                    }
                                    assert!(
                                        bus.name_has_owner(
                                            BusName::try_from(caller.as_str())
                                                .expect("caller name")
                                        )
                                        .await
                                        .expect("prompt caller owner")
                                    );
                                    1
                                }
                            };
                            assert_eq!(
                                queued.await.expect("queued read survives prompt outcome"),
                                Some(("user".to_owned(), b"secret".to_vec())),
                                "{target}: {outcome:?}"
                            );
                            assert_eq!(state.lock().opened, expected_sessions);
                            let session = object_path(&format!(
                                "/org/freedesktop/secrets/session/{expected_sessions}"
                            ));
                            let recovered_caller = state
                                .lock()
                                .sessions
                                .get(&session)
                                .expect("recovered session")
                                .clone();
                            if expected_sessions == 2 {
                                assert_ne!(recovered_caller, caller);
                            } else {
                                assert_eq!(recovered_caller, caller);
                            }
                            assert_eq!(
                                platform
                                    .read_credentials("keyring-test")
                                    .await
                                    .expect("reuse session after prompt outcome"),
                                Some(("user".to_owned(), b"secret".to_vec()))
                            );
                            assert_eq!(state.lock().opened, expected_sessions);
                            assert_eq!(state.lock().rejected_sessions, 0);
                            assert_eq!(state.lock().session_closes, 0);
                            daemon.close().await.expect("close service");
                        }
                    }
                },
            );
        }

        #[test]
        fn lost_completed_reads_recover_after_owner_loss() {
            run_on_private_bus("lost_completed_reads_recover_after_owner_loss", async {
                for (operation, loss) in [
                    (
                        CredentialOperation::ReadCollection,
                        ConnectionLoss::ReplaceOwner,
                    ),
                    (CredentialOperation::ReadItem, ConnectionLoss::ReleaseOwner),
                    (
                        CredentialOperation::CreateCollection,
                        ConnectionLoss::ReplaceOwner,
                    ),
                    (
                        CredentialOperation::ReadItem,
                        ConnectionLoss::MonitorDisconnected,
                    ),
                ] {
                    assert_lost_completed_recovery(operation, loss).await;
                }
            });
        }

        #[test]
        fn lost_completed_mutations_are_not_replayed_after_owner_loss() {
            run_on_private_bus(
                "lost_completed_mutations_are_not_replayed_after_owner_loss",
                async {
                    for (operation, loss) in [
                        (CredentialOperation::Write, ConnectionLoss::ReplaceOwner),
                        (CredentialOperation::Delete, ConnectionLoss::ReplaceOwner),
                        (
                            CredentialOperation::Write,
                            ConnectionLoss::MonitorDisconnected,
                        ),
                    ] {
                        assert_lost_completed_recovery(operation, loss).await;
                    }
                },
            );
        }

        #[test]
        fn owner_loss_during_session_setup_releases_the_cached_initializer() {
            run_on_private_bus(
                "owner_loss_during_session_setup_releases_the_cached_initializer",
                async {
                    let state = Arc::new(Mutex::new(ServiceState::default()));
                    let daemon = start_service(state.clone()).await;
                    let platform = LinuxPlatform::new(WindowingModes::HEADLESS);
                    let (entered, entering) = oneshot::channel();
                    let (release, released) = oneshot::channel();
                    state.lock().open_gate = Some((entered, released));
                    let read = platform.read_credentials("keyring-test");
                    entering.await.expect("session opening started");
                    let replacement = replace_service_owner(&daemon, state.clone()).await;
                    assert_eq!(read.await.expect("recover interrupted initializer"), None);
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("new initializer"),
                        None
                    );
                    assert_eq!(state.lock().opened, 2);
                    release.send(()).expect("release old session handler");
                    replacement.close().await.expect("close replacement");
                    daemon.close().await.expect("close original service");
                },
            );
        }

        #[test]
        fn canceled_session_setup_recovers_after_restart_for_reads_and_mutations() {
            run_on_private_bus(
                "canceled_session_setup_recovers_after_restart_for_reads_and_mutations",
                async {
                    for create_collection in [false, true] {
                        for restart in [false, true] {
                            for mutate in [false, true] {
                                let state = Arc::new(Mutex::new(ServiceState::default()));
                                let daemon = start_service(state.clone()).await;
                                let platform = LinuxPlatform::new(WindowingModes::HEADLESS);
                                let (entered, entering) = oneshot::channel();
                                let (prompted, prompting) = oneshot::channel();
                                let (release, released) = oneshot::channel();
                                let collection =
                                    object_path("/org/freedesktop/secrets/collection/default");
                                if create_collection {
                                    let mut state = state.lock();
                                    state.default_collection = Some(object_path("/"));
                                    state.prompt_target = Some(collection.clone());
                                    state.prompt_gate = Some((prompted, released));
                                    state.defer_session_close = true;
                                } else {
                                    state.lock().open_gate = Some((entered, released));
                                }
                                let canceled = platform.read_credentials("keyring-test");
                                let dispatched = if create_collection {
                                    Some(prompting.await.expect("collection prompt started").1)
                                } else {
                                    entering.await.expect("session opening started");
                                    None
                                };
                                drop(canceled);
                                drop(platform.secret_service.lock().await);
                                let replacement = if restart {
                                    Some(replace_service_owner(&daemon, state.clone()).await)
                                } else {
                                    None
                                };
                                release.send(()).expect("release initialization");
                                if let Some(dispatched) = dispatched {
                                    dispatched.await;
                                    if !restart {
                                        let emitter = SignalEmitter::new(
                                            &daemon,
                                            "/org/freedesktop/secrets/prompt/1",
                                        )
                                        .expect("prompt emitter");
                                        TestPrompt::completed(
                                            &emitter,
                                            false,
                                            OwnedValue::try_from(Value::from(collection))
                                                .expect("created collection path"),
                                        )
                                        .await
                                        .expect("complete collection creation");
                                    }
                                }
                                if mutate {
                                    platform
                                        .write_credentials("keyring-test", "user", b"recovered")
                                        .await
                                        .expect("write after canceled initialization");
                                } else {
                                    assert_eq!(
                                        platform
                                            .read_credentials("keyring-test")
                                            .await
                                            .expect("read after canceled initialization"),
                                        None
                                    );
                                }
                                {
                                    let state = state.lock();
                                    assert_eq!(state.opened, 1 + usize::from(restart));
                                    assert_eq!(state.writes, usize::from(mutate));
                                    assert_eq!(state.secret, mutate.then(|| b"recovered".to_vec()));
                                    assert_eq!(
                                        state.collections_created,
                                        usize::from(create_collection)
                                    );
                                    if !restart {
                                        assert_eq!(state.session_closes, 0);
                                    }
                                }
                                if let Some(replacement) = replacement {
                                    replacement.close().await.expect("close replacement");
                                }
                                daemon.close().await.expect("close original service");
                            }
                        }
                    }
                },
            );
        }

        #[test]
        fn filters_idle_monitor_traffic_and_recovers_after_owner_churn() {
            run_on_private_bus(
                "filters_idle_monitor_traffic_and_recovers_after_owner_churn",
                async {
                    let state = Arc::new(Mutex::new(ServiceState::default()));
                    let daemon = start_service(state.clone()).await;
                    let platform = LinuxPlatform::new(WindowingModes::HEADLESS);
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("warm cache"),
                        None
                    );
                    let monitor = platform
                        .secret_service
                        .lock()
                        .await
                        .connected
                        .as_ref()
                        .expect("cached connection")
                        .bus
                        .clone();
                    let destination = monitor
                        .inner()
                        .connection()
                        .unique_name()
                        .expect("monitor name");
                    let owner = daemon.unique_name().expect("service owner").clone();
                    for sequence in 0..128u32 {
                        for destination in [None, Some(destination.as_str())] {
                            daemon
                                .emit_signal(
                                    destination,
                                    "/noise",
                                    "org.zed.Noise",
                                    "Noise",
                                    &sequence,
                                )
                                .await
                                .expect("unrelated signal");
                        }
                        daemon
                            .emit_signal(
                                Some(destination.as_str()),
                                "/org/freedesktop/DBus",
                                "org.freedesktop.DBus",
                                "NameOwnerChanged",
                                &(SECRET_SERVICE_NAME.as_str(), owner.as_str(), ""),
                            )
                            .await
                            .expect("forged owner change");
                    }
                    fdo::DBusProxy::new(&daemon)
                        .await
                        .expect("service bus proxy")
                        .get_name_owner(BusName::WellKnown(SECRET_SERVICE_NAME))
                        .await
                        .expect("traffic barrier");
                    assert_eq!(
                        monitor
                            .get_name_owner(BusName::WellKnown(SECRET_SERVICE_NAME))
                            .await
                            .expect("idle monitor rejects unrelated traffic"),
                        owner
                    );
                    drop(monitor);
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("forged signals do not invalidate the session"),
                        None
                    );
                    assert_eq!(state.lock().opened, 1);
                    for _ in 0..128 {
                        assert!(
                            daemon
                                .release_name("org.freedesktop.secrets")
                                .await
                                .expect("release owner")
                        );
                        daemon
                            .request_name("org.freedesktop.secrets")
                            .await
                            .expect("reacquire owner");
                    }
                    assert_eq!(
                        platform
                            .read_credentials("keyring-test")
                            .await
                            .expect("recover from saturated owner-change queue"),
                        None
                    );
                    assert_eq!(state.lock().opened, 2);
                    daemon.close().await.expect("close service");
                },
            );
        }

        async fn assert_lost_completed_recovery(
            operation: CredentialOperation,
            loss: ConnectionLoss,
        ) {
            let state = Arc::new(Mutex::new(ServiceState {
                attributes: HashMap::from([
                    ("url".to_owned(), "keyring-test".to_owned()),
                    ("username".to_owned(), "user".to_owned()),
                ]),
                secret: Some(b"first".to_vec()),
                ..ServiceState::default()
            }));
            let daemon = start_service(state.clone()).await;
            let platform = LinuxPlatform::new(WindowingModes::HEADLESS);
            if operation == CredentialOperation::CreateCollection {
                state.lock().default_collection = Some(object_path("/"));
            } else {
                platform
                    .read_credentials("keyring-test")
                    .await
                    .expect("warm cache");
            }
            let monitor = platform
                .secret_service
                .lock()
                .await
                .connected
                .as_ref()
                .map(|connection| connection.bus.inner().connection().clone());
            let (entered, entering) = oneshot::channel();
            let (release, released) = oneshot::channel();
            {
                let mut state = state.lock();
                state.prompt_target = Some(object_path(match operation {
                    CredentialOperation::ReadCollection | CredentialOperation::CreateCollection => {
                        "/org/freedesktop/secrets/collection/default"
                    }
                    _ => "/org/freedesktop/secrets/item/1",
                }));
                state.prompt_gate = Some((entered, released));
            }
            let pending = match operation {
                CredentialOperation::Write => platform
                    .write_credentials("keyring-test", "user", b"second")
                    .boxed(),
                CredentialOperation::Delete => platform.delete_credentials("keyring-test").boxed(),
                _ => platform
                    .read_credentials("keyring-test")
                    .map(|result| {
                        result.map(|credentials| {
                            assert_eq!(credentials, Some(("user".to_owned(), b"first".to_vec())));
                        })
                    })
                    .boxed(),
            };
            let (_, dispatched) = entering.await.expect("prompt started");
            release.send(()).expect("acknowledge prompt");
            dispatched.await;
            let queued = platform.read_credentials("keyring-test");
            let replacement = match loss {
                ConnectionLoss::ReplaceOwner => {
                    Some(replace_service_owner(&daemon, state.clone()).await)
                }
                ConnectionLoss::ReleaseOwner => {
                    assert!(
                        daemon
                            .release_name("org.freedesktop.secrets")
                            .await
                            .expect("release owner")
                    );
                    daemon
                        .request_name("org.freedesktop.secrets")
                        .await
                        .expect("reacquire owner");
                    None
                }
                ConnectionLoss::MonitorDisconnected => {
                    monitor
                        .expect("cached monitor")
                        .close()
                        .await
                        .expect("disconnect monitor");
                    None
                }
            };
            let result = pending.await;
            if operation == CredentialOperation::Write || operation == CredentialOperation::Delete {
                let error = result.expect_err("do not replay an interrupted mutation");
                assert_eq!(classify_keyring_error(&error), KeyringErrorKind::Connection);
            } else if loss == ConnectionLoss::ReleaseOwner {
                if let Err(error) = result {
                    assert_eq!(classify_keyring_error(&error), KeyringErrorKind::Connection);
                }
            } else {
                result.expect("retry interrupted read");
            }
            let expected = match operation {
                CredentialOperation::Write => Some(("user".to_owned(), b"second".to_vec())),
                CredentialOperation::Delete => None,
                _ => Some(("user".to_owned(), b"first".to_vec())),
            };
            let queued_result = queued.await;
            if loss == ConnectionLoss::ReleaseOwner && queued_result.is_err() {
                assert_eq!(
                    classify_keyring_error(&queued_result.expect_err("owner gap")),
                    KeyringErrorKind::Connection
                );
            } else {
                assert_eq!(queued_result.expect("queued read is unblocked"), expected);
            }
            assert_eq!(
                platform
                    .read_credentials("keyring-test")
                    .await
                    .expect("recovered read"),
                expected
            );
            let state = state.lock();
            assert_eq!(
                state.writes,
                usize::from(operation == CredentialOperation::Write)
            );
            assert_eq!(
                state.deletes,
                usize::from(operation == CredentialOperation::Delete)
            );
            assert_eq!(
                state.collections_created,
                usize::from(operation == CredentialOperation::CreateCollection)
            );
            drop(state);
            if let Some(replacement) = replacement {
                replacement.close().await.expect("close replacement");
            }
            daemon.close().await.expect("close original service");
        }

        fn run_on_private_bus(name: &str, scenario: impl Future<Output = ()>) {
            const CHILD_ENV: &str = "ZED_KEYRING_TEST";
            if std::env::var(CHILD_ENV).as_deref() == Ok(name) {
                smol::block_on(smol::future::or(scenario, async {
                    smol::Timer::after(Duration::from_secs(5)).await;
                    panic!("keyring scenario {name} timed out");
                }));
                return;
            }
            let test = format!("linux::platform::tests::keyring::{name}");
            let output = Command::new("dbus-run-session")
                .arg("--")
                .arg(std::env::current_exe().expect("test executable"))
                .args(["--exact", &test, "--nocapture"])
                .env(CHILD_ENV, name)
                .output()
                .expect("run test on a private D-Bus session");
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }

        fn disconnected() -> anyhow::Error {
            anyhow!(oo7::dbus::Error::ZBus(oo7::zbus::Error::FDO(Box::new(
                fdo::Error::Disconnected("reply lost".to_owned())
            ))))
        }

        async fn start_service(state: Arc<Mutex<ServiceState>>) -> Connection {
            zbus::connection::Builder::session()
                .expect("connect service to bus")
                .serve_at("/org/freedesktop/secrets", TestService(state.clone()))
                .expect("serve Secret Service")
                .serve_at(
                    "/org/freedesktop/secrets/collection/default",
                    TestCollection(state.clone()),
                )
                .expect("serve collection")
                .serve_at(
                    "/org/freedesktop/secrets/collection/replacement",
                    TestCollection(state.clone()),
                )
                .expect("serve replacement collection")
                .serve_at("/org/freedesktop/secrets/item/1", TestItem(state.clone()))
                .expect("serve item")
                .serve_at(
                    "/org/freedesktop/secrets/item/unreadable",
                    UnreadableTestItem,
                )
                .expect("serve unreadable item")
                .serve_at("/org/freedesktop/secrets/prompt/1", TestPrompt(state))
                .expect("serve prompt")
                .name("org.freedesktop.secrets")
                .expect("service name")
                .build()
                .await
                .expect("start service")
        }

        async fn restart_service(
            connection: Connection,
            state: Arc<Mutex<ServiceState>>,
        ) -> Connection {
            connection.close().await.expect("stop service");
            state.lock().sessions.clear();
            start_service(state).await
        }

        async fn replace_service_owner(
            connection: &Connection,
            state: Arc<Mutex<ServiceState>>,
        ) -> Connection {
            fdo::DBusProxy::new(connection)
                .await
                .expect("bus proxy")
                .request_name(
                    SECRET_SERVICE_NAME,
                    fdo::RequestNameFlags::AllowReplacement.into(),
                )
                .await
                .expect("allow replacement");
            state.lock().sessions.clear();
            start_service(state).await
        }

        fn object_path(path: &str) -> OwnedObjectPath {
            OwnedObjectPath::try_from(path.to_owned()).expect("valid object path")
        }

        type TestSecret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);
        type PromptCall = (String, BoxFuture<'static, ()>);

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum PromptOutcome {
            CancelBeforeReply,
            CancelAfterReply,
            Complete,
            Dismiss,
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum CredentialOperation {
            ReadCollection,
            ReadItem,
            CreateCollection,
            Write,
            Delete,
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum ConnectionLoss {
            ReplaceOwner,
            ReleaseOwner,
            MonitorDisconnected,
        }

        #[derive(Default)]
        struct ServiceState {
            sessions: HashMap<OwnedObjectPath, String>,
            opened: usize,
            writes: usize,
            deletes: usize,
            collections_created: usize,
            rejected_sessions: usize,
            session_closes: usize,
            defer_session_close: bool,
            default_collection: Option<OwnedObjectPath>,
            attributes: HashMap<String, String>,
            secret: Option<Vec<u8>>,
            missing_username: bool,
            fail_label: bool,
            unreadable_item_first: Option<bool>,
            unreadable_item_missing: bool,
            fail_write_reply: bool,
            open_gate: Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>,
            unlock_gate: Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>,
            prompt_target: Option<OwnedObjectPath>,
            prompt_gate: Option<(oneshot::Sender<PromptCall>, oneshot::Receiver<()>)>,
        }

        impl ServiceState {
            fn take_prompt(&mut self, objects: &[OwnedObjectPath]) -> OwnedObjectPath {
                if self
                    .prompt_target
                    .as_ref()
                    .is_some_and(|target| objects.iter().any(|object| object == target))
                {
                    self.prompt_target = None;
                    object_path("/org/freedesktop/secrets/prompt/1")
                } else {
                    object_path("/")
                }
            }

            fn check_session(
                &mut self,
                path: &OwnedObjectPath,
                header: &Header<'_>,
            ) -> fdo::Result<()> {
                if self.sessions.get(path).map(String::as_str)
                    == header.sender().map(|sender| sender.as_str())
                {
                    Ok(())
                } else {
                    self.rejected_sessions += 1;
                    Err(fdo::Error::UnknownObject("invalid session".to_owned()))
                }
            }
        }

        struct TestService(Arc<Mutex<ServiceState>>);

        #[oo7::zbus::interface(name = "org.freedesktop.Secret.Service", crate = "oo7::zbus")]
        impl TestService {
            async fn open_session(
                &self,
                algorithm: &str,
                _input: OwnedValue,
                #[zbus(header)] header: Header<'_>,
                #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
            ) -> fdo::Result<(OwnedValue, OwnedObjectPath)> {
                if algorithm != "plain" {
                    return Err(fdo::Error::NotSupported("plain test service".to_owned()));
                }
                let (path, gate) = {
                    let mut state = self.0.lock();
                    let path = object_path(&format!(
                        "/org/freedesktop/secrets/session/{}",
                        state.sessions.len() + 1
                    ));
                    state
                        .sessions
                        .insert(path.clone(), header.sender().expect("sender").to_string());
                    state.opened += 1;
                    (path, state.open_gate.take())
                };
                emitter
                    .connection()
                    .object_server()
                    .at(path.clone(), TestSession(self.0.clone()))
                    .await
                    .expect("serve session");
                if let Some((entered, released)) = gate {
                    entered.send(()).expect("session opening observed");
                    released.await.expect("session opening released");
                }
                Ok((OwnedValue::from(Str::from("")), path))
            }

            fn read_alias(&self, _name: &str) -> OwnedObjectPath {
                self.0
                    .lock()
                    .default_collection
                    .clone()
                    .unwrap_or_else(|| object_path("/org/freedesktop/secrets/collection/default"))
            }

            fn create_collection(
                &self,
                _properties: HashMap<String, OwnedValue>,
                _alias: &str,
            ) -> (OwnedObjectPath, OwnedObjectPath) {
                let mut state = self.0.lock();
                let collection = object_path("/org/freedesktop/secrets/collection/default");
                state.default_collection = Some(collection.clone());
                state.collections_created += 1;
                let prompt = state.take_prompt(std::slice::from_ref(&collection));
                (collection, prompt)
            }

            async fn unlock(
                &self,
                objects: Vec<OwnedObjectPath>,
            ) -> (Vec<OwnedObjectPath>, OwnedObjectPath) {
                let gate = self.0.lock().unlock_gate.take();
                if let Some((entered, released)) = gate {
                    entered.send(()).expect("unlock observed");
                    released.await.expect("unlock released");
                }
                let mut state = self.0.lock();
                let prompt = state.take_prompt(&objects);
                if prompt == object_path("/") {
                    (objects, prompt)
                } else {
                    (Vec::new(), prompt)
                }
            }
        }

        struct TestSession(Arc<Mutex<ServiceState>>);

        #[oo7::zbus::interface(name = "org.freedesktop.Secret.Session", crate = "oo7::zbus")]
        impl TestSession {
            async fn close(&self) {
                let defer = {
                    let mut state = self.0.lock();
                    state.session_closes += 1;
                    state.defer_session_close
                };
                if defer {
                    futures::future::pending::<()>().await;
                }
            }
        }

        struct TestPrompt(Arc<Mutex<ServiceState>>);

        #[oo7::zbus::interface(name = "org.freedesktop.Secret.Prompt", crate = "oo7::zbus")]
        impl TestPrompt {
            async fn prompt(
                &self,
                _window_id: &str,
                #[zbus(header)] header: Header<'_>,
            ) -> ResponseDispatchNotifier<()> {
                let (entered, released) = self.0.lock().prompt_gate.take().expect("prompt gate");
                let (response, dispatched) = ResponseDispatchNotifier::new(());
                assert!(
                    entered
                        .send((
                            header.sender().expect("prompt caller").to_string(),
                            dispatched.boxed(),
                        ))
                        .is_ok()
                );
                released.await.expect("prompt reply released");
                response
            }

            #[zbus(signal)]
            async fn completed(
                emitter: &SignalEmitter<'_>,
                dismissed: bool,
                result: OwnedValue,
            ) -> zbus::Result<()>;
        }

        struct TestMethodError {
            name: &'static str,
            message: String,
        }

        impl zbus::DBusError for TestMethodError {
            fn create_reply(&self, header: &Header<'_>) -> zbus::Result<zbus::message::Message> {
                zbus::message::Message::error(header, self.name())?.build(&self.message)
            }

            fn name(&self) -> zbus::names::ErrorName<'_> {
                zbus::names::ErrorName::from_static_str_unchecked(self.name)
            }

            fn description(&self) -> Option<&str> {
                Some(&self.message)
            }
        }

        struct TestCollection(Arc<Mutex<ServiceState>>);

        #[oo7::zbus::interface(name = "org.freedesktop.Secret.Collection", crate = "oo7::zbus")]
        impl TestCollection {
            fn search_items(&self, attributes: HashMap<String, String>) -> Vec<OwnedObjectPath> {
                let state = self.0.lock();
                if !attributes
                    .iter()
                    .all(|(key, value)| state.attributes.get(key) == Some(value))
                {
                    return Vec::new();
                }
                let mut items = Vec::new();
                if state.secret.is_some() {
                    items.push(object_path("/org/freedesktop/secrets/item/1"));
                }
                if let Some(unreadable_first) = state.unreadable_item_first {
                    let unreadable = object_path(if state.unreadable_item_missing {
                        "/org/freedesktop/secrets/item/missing"
                    } else {
                        "/org/freedesktop/secrets/item/unreadable"
                    });
                    if unreadable_first {
                        items.insert(0, unreadable);
                    } else {
                        items.push(unreadable);
                    }
                }
                items
            }

            fn create_item(
                &self,
                mut properties: HashMap<String, OwnedValue>,
                secret: TestSecret,
                _replace: bool,
                #[zbus(header)] header: Header<'_>,
            ) -> Result<(OwnedObjectPath, OwnedObjectPath), TestMethodError> {
                let mut state = self.0.lock();
                state
                    .check_session(&secret.0, &header)
                    .map_err(|_| TestMethodError {
                        name: "org.qtproject.QtDBus.Error.InvalidObjectPath",
                        message: format!("Can't find session {}", secret.0),
                    })?;
                state.attributes = HashMap::<String, String>::try_from(
                    properties
                        .remove("org.freedesktop.Secret.Item.Attributes")
                        .expect("attributes"),
                )
                .expect("attribute map");
                state.secret = Some(secret.2);
                state.writes += 1;
                if std::mem::take(&mut state.fail_write_reply) {
                    return Err(TestMethodError {
                        name: "org.freedesktop.DBus.Error.Disconnected",
                        message: "reply lost after commit".to_owned(),
                    });
                }
                let item = object_path("/org/freedesktop/secrets/item/1");
                let prompt = state.take_prompt(std::slice::from_ref(&item));
                Ok((item, prompt))
            }
        }

        struct TestItem(Arc<Mutex<ServiceState>>);

        #[oo7::zbus::interface(name = "org.freedesktop.Secret.Item", crate = "oo7::zbus")]
        impl TestItem {
            #[zbus(property)]
            fn label(&self) -> fdo::Result<&str> {
                if self.0.lock().fail_label {
                    Err(fdo::Error::AccessDenied("label access denied".to_owned()))
                } else {
                    Ok(KEYRING_LABEL)
                }
            }

            #[zbus(property)]
            fn attributes(&self) -> HashMap<String, String> {
                let state = self.0.lock();
                let mut attributes = state.attributes.clone();
                if state.missing_username {
                    attributes.remove("username");
                }
                attributes
            }

            fn get_secret(
                &self,
                session: OwnedObjectPath,
                #[zbus(header)] header: Header<'_>,
            ) -> fdo::Result<(TestSecret,)> {
                let mut state = self.0.lock();
                state.check_session(&session, &header)?;
                Ok(((
                    session,
                    Vec::new(),
                    state.secret.clone().expect("stored secret"),
                    "application/octet-stream".to_owned(),
                ),))
            }

            fn delete(&self) -> OwnedObjectPath {
                let mut state = self.0.lock();
                state.secret = None;
                state.deletes += 1;
                state.take_prompt(&[object_path("/org/freedesktop/secrets/item/1")])
            }
        }

        struct UnreadableTestItem;

        #[oo7::zbus::interface(name = "org.freedesktop.Secret.Item", crate = "oo7::zbus")]
        impl UnreadableTestItem {
            #[zbus(property)]
            fn label(&self) -> fdo::Result<&str> {
                Err(fdo::Error::AccessDenied(
                    "foreign label access denied".to_owned(),
                ))
            }
        }
    }

    #[cfg(any(feature = "wayland", feature = "x11"))]
    #[test]
    fn rejects_null_xkb_context() {
        let context = unsafe {
            // libxkbcommon permits unref on null, matching the value returned by Context::new on failure.
            xkb::Context::from_raw_ptr(std::ptr::null_mut())
        };
        let error = validate_xkb_context(context)
            .err()
            .expect("null XKB context should be rejected");

        assert_eq!(
            error.to_string(),
            "libxkbcommon failed to create an XKB context"
        );
    }

    #[test]
    fn test_is_within_click_distance() {
        let zero = Point::new(px(0.0), px(0.0));
        assert!(is_within_click_distance(zero, Point::new(px(5.0), px(5.0))));
        assert!(is_within_click_distance(
            zero,
            Point::new(px(-4.9), px(5.0))
        ));
        assert!(is_within_click_distance(
            Point::new(px(3.0), px(2.0)),
            Point::new(px(-2.0), px(-2.0))
        ));
        assert!(!is_within_click_distance(
            zero,
            Point::new(px(5.0), px(5.1))
        ),);
    }

    #[cfg(any(feature = "wayland", feature = "x11"))]
    mod idle_sleep_prevention {
        use super::super::await_idle_sleep_prevention;
        use anyhow::{Result, anyhow};
        use futures::channel::oneshot;
        use gpui::{ActivityGuard, TestAppContext};
        use std::{
            sync::{
                Arc,
                atomic::{AtomicUsize, Ordering::SeqCst},
            },
            time::Duration,
        };

        #[gpui::test]
        async fn guard_passes_through_and_releases_on_drop(cx: &mut TestAppContext) {
            let released = Arc::new(AtomicUsize::new(0));
            let (guard_tx, guard_rx) = oneshot::channel();
            assert!(guard_tx.send(Ok(release_guard(released.clone()))).is_ok());
            let guard = await_idle_sleep_prevention(guard_rx, &cx.background_executor)
                .await
                .expect("acquisition should succeed");

            assert_eq!(released.load(SeqCst), 0);
            drop(guard);
            assert_eq!(released.load(SeqCst), 1);
        }

        #[gpui::test]
        async fn acquisition_error_passes_through(cx: &mut TestAppContext) {
            let (guard_tx, guard_rx) = oneshot::channel::<Result<ActivityGuard>>();
            assert!(guard_tx.send(Err(anyhow!("inhibition rejected"))).is_ok());
            assert_eq!(
                await_idle_sleep_prevention(guard_rx, &cx.background_executor)
                    .await
                    .err()
                    .expect("acquisition should fail")
                    .to_string(),
                "inhibition rejected"
            );
        }

        #[gpui::test]
        async fn abandoned_acquisition_is_an_error(cx: &mut TestAppContext) {
            let (guard_tx, guard_rx) = oneshot::channel::<Result<ActivityGuard>>();
            drop(guard_tx);
            assert_eq!(
                await_idle_sleep_prevention(guard_rx, &cx.background_executor)
                    .await
                    .err()
                    .expect("acquisition should fail")
                    .to_string(),
                "Idle sleep prevention request was abandoned"
            );
        }

        #[gpui::test]
        async fn late_guard_after_timeout_is_released(cx: &mut TestAppContext) {
            let released = Arc::new(AtomicUsize::new(0));
            let (guard_tx, guard_rx) = oneshot::channel();
            let task = cx.background_executor.spawn({
                let executor = cx.background_executor.clone();
                async move { await_idle_sleep_prevention(guard_rx, &executor).await }
            });
            cx.run_until_parked();
            cx.executor().advance_clock(Duration::from_secs(9));
            cx.run_until_parked();
            assert!(!guard_tx.is_canceled());
            cx.executor().advance_clock(Duration::from_secs(1));
            assert_eq!(
                task.await
                    .err()
                    .expect("acquisition should time out")
                    .to_string(),
                "Idle sleep prevention acquisition timed out after 10 seconds"
            );

            assert!(guard_tx.is_canceled());
            if let Err(Ok(guard)) = guard_tx.send(Ok(release_guard(released.clone()))) {
                assert_eq!(released.load(SeqCst), 0);
                drop(guard);
            }
            assert_eq!(released.load(SeqCst), 1);
        }

        fn release_guard(released: Arc<AtomicUsize>) -> ActivityGuard {
            ActivityGuard::new(move || {
                released.fetch_add(1, SeqCst);
            })
        }
    }

    #[cfg(any(feature = "wayland", feature = "x11"))]
    mod read_fd_with_timeout {
        use super::super::{PIPE_READ_TIMEOUT, read_fd_with_timeout};
        use std::io::Write as _;
        use std::time::{Duration, Instant};

        #[test]
        fn reads_data_written_before_close() {
            let mut pipe = filedescriptor::Pipe::new().unwrap();
            pipe.write.write_all(b"hello clipboard").unwrap();
            drop(pipe.write);

            let bytes = read_fd_with_timeout(pipe.read, PIPE_READ_TIMEOUT).unwrap();
            assert_eq!(bytes, b"hello clipboard");
        }

        #[test]
        fn returns_empty_when_writer_closes_without_writing() {
            let pipe = filedescriptor::Pipe::new().unwrap();
            drop(pipe.write);

            let bytes = read_fd_with_timeout(pipe.read, PIPE_READ_TIMEOUT).unwrap();
            assert!(bytes.is_empty());
        }

        #[test]
        fn times_out_when_writer_never_writes() {
            let pipe = filedescriptor::Pipe::new().unwrap();
            let _open_writer = pipe.write;

            let timeout = Duration::from_millis(50);
            let started = Instant::now();
            let result = read_fd_with_timeout(pipe.read, timeout);
            let elapsed = started.elapsed();

            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("timed out"),
                "unexpected error: {err}"
            );
            assert!(elapsed >= timeout, "returned before the timeout elapsed");
        }

        #[test]
        fn times_out_when_writer_stalls_after_partial_write() {
            let mut pipe = filedescriptor::Pipe::new().unwrap();
            pipe.write.write_all(b"partial").unwrap();
            let _open_writer = pipe.write;

            let err = read_fd_with_timeout(pipe.read, Duration::from_millis(50)).unwrap_err();
            assert!(
                err.to_string().contains("timed out"),
                "unexpected error: {err}"
            );
        }

        #[test]
        fn slow_writer_resets_deadline_between_chunks() {
            let pipe = filedescriptor::Pipe::new().unwrap();
            let chunks = 12;
            let gap = Duration::from_millis(40);
            let timeout = Duration::from_millis(400);

            let writer = std::thread::spawn({
                let mut write = pipe.write;
                move || {
                    for _ in 0..chunks {
                        std::thread::sleep(gap);
                        write.write_all(&[b'x'; 1000]).unwrap();
                    }
                }
            });
            // The total transfer (~480ms) exceeds the timeout; this only
            // passes because the timeout is re-armed per chunk.
            let bytes = read_fd_with_timeout(pipe.read, timeout).unwrap();
            writer.join().unwrap();
            assert_eq!(bytes, vec![b'x'; 1000 * chunks]);
        }

        #[test]
        fn reads_payload_larger_than_pipe_capacity() {
            let pipe = filedescriptor::Pipe::new().unwrap();
            // Exceeds the 64 KiB pipe capacity, forcing the writer to block.
            let payload = vec![b'z'; 1024 * 1024];

            let writer = std::thread::spawn({
                let mut write = pipe.write;
                let payload = payload.clone();
                move || write.write_all(&payload).unwrap()
            });
            let bytes = read_fd_with_timeout(pipe.read, PIPE_READ_TIMEOUT).unwrap();
            writer.join().unwrap();
            assert_eq!(bytes, payload);
        }
    }
}

#[cfg(test)]
mod display_mode_tests {
    use std::{cell::Cell, process::Command};

    #[cfg(feature = "wayland")]
    use gpui::{AppContext as _, Context, IntoElement, Render, Window, WindowOptions};
    use gpui::{Application, AsyncApp, QuitMode};

    use super::*;

    /// Set in the child process that runs a scenario, naming the scenario to run.
    const SCENARIO_ENV: &str = "GPUI_LINUX_DISPLAY_MODE_SCENARIO";
    const MISSING_WAYLAND_DISPLAY: &str = "/gpui-test/wayland-display-does-not-exist";

    #[cfg(feature = "wayland")]
    #[test]
    fn failed_wayland_attach_preserves_headless_app_and_allows_retry() {
        run_scenario(
            "failed_wayland_attach_preserves_headless_app_and_allows_retry",
            WindowingModes::all(),
            |cx| {
                Box::pin(async move {
                    let entity = cx.new(|_| 1usize);
                    let (release_task, wait_for_release) = oneshot::channel();
                    let queued_task_ran = Rc::new(Cell::new(false));
                    let queued_task = cx.spawn({
                        let queued_task_ran = queued_task_ran.clone();
                        async move |_| {
                            wait_for_release
                                .await
                                .expect("queued task release sender remains alive");
                            queued_task_ran.set(true);
                        }
                    });

                    let first_attach = cx.update(|cx| {
                        cx.request_windowing(WindowingRequest::Windowed(
                            GraphicalEnvironment::detect(),
                        ))
                    });
                    assert_error_contains(
                        first_attach.await,
                        "failed to connect to Wayland compositor",
                    );
                    entity.update(cx, |value, _| *value += 1);
                    release_task
                        .send(())
                        .expect("queued task remains attached to the executor");
                    queued_task.await;
                    assert!(queued_task_ran.get());

                    let second_attach = cx.update(|cx| {
                        cx.request_windowing(WindowingRequest::Windowed(
                            GraphicalEnvironment::detect(),
                        ))
                    });
                    assert_error_contains(
                        second_attach.await,
                        "failed to connect to Wayland compositor",
                    );
                    entity.update(cx, |value, _| *value += 1);
                    assert_eq!(entity.read_with(cx, |value, _| *value), 3);
                })
            },
        );
    }

    #[cfg(feature = "wayland")]
    #[test]
    fn headless_windows_block_switching_to_a_display_server() {
        run_scenario(
            "headless_windows_block_switching_to_a_display_server",
            WindowingModes::all(),
            |cx| {
                Box::pin(async move {
                    let window = cx
                        .update(|cx| {
                            cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| Blank))
                        })
                        .expect("headless mode opens headless windows");
                    let attach = cx.update(|cx| {
                        cx.request_windowing(WindowingRequest::Windowed(
                            GraphicalEnvironment::detect(),
                        ))
                    });
                    assert_error_contains(attach.await, "while windows are open");

                    cx.update(|cx| window.update(cx, |_, window, _| window.remove_window()))
                        .expect("close the headless window");
                    let attach = cx.update(|cx| {
                        cx.request_windowing(WindowingRequest::Windowed(
                            GraphicalEnvironment::detect(),
                        ))
                    });
                    assert_error_contains(attach.await, "failed to connect to Wayland compositor");
                })
            },
        );
    }

    #[test]
    fn display_modes_limit_switching() {
        run_scenario(
            "display_modes_limit_switching",
            WindowingModes::HEADLESS,
            |cx| {
                Box::pin(async move {
                    let attach = cx.update(|cx| {
                        cx.request_windowing(WindowingRequest::Windowed(
                            GraphicalEnvironment::detect(),
                        ))
                    });
                    assert_error_contains(attach.await, "names no display server that");
                    assert_eq!(cx.update(|cx| cx.compositor_name()), "headless");
                    assert!(cx.update(|cx| cx.graphical_environment()).is_none());

                    let detach = cx.update(|cx| cx.request_windowing(WindowingRequest::Headless));
                    assert_error_contains(detach.await, "already headless");
                })
            },
        );
    }

    #[cfg(feature = "wayland")]
    struct Blank;

    #[cfg(feature = "wayland")]
    impl Render for Blank {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            gpui::Empty
        }
    }

    type Scenario = fn(&mut AsyncApp) -> futures::future::LocalBoxFuture<'_, ()>;

    /// Runs `scenario` on a headless platform in a child process, where `WAYLAND_DISPLAY` names a
    /// socket that doesn't exist.
    fn run_scenario(name: &str, modes: WindowingModes, scenario: Scenario) {
        if std::env::var(SCENARIO_ENV).as_deref() == Ok(name) {
            let platform = Rc::new(LinuxPlatform::new(modes));
            platform.set_initial_windowing(WindowingRequest::Headless);
            let completed = Rc::new(Cell::new(false));
            Application::with_platform(platform)
                .with_quit_mode(QuitMode::Explicit)
                .run({
                    let completed = completed.clone();
                    move |cx| {
                        cx.spawn(async move |cx| {
                            scenario(cx).await;
                            completed.set(true);
                            cx.update(|cx| cx.quit());
                        })
                        .detach();
                    }
                });
            assert!(completed.get(), "scenario did not complete");
            return;
        }

        // Test names don't include the crate's name.
        let module = module_path!()
            .split_once("::")
            .map_or(module_path!(), |(_, module)| module);
        let test_name = format!("{module}::{name}");
        #[allow(
            clippy::disallowed_methods,
            reason = "the test thread has nothing else to do while the child runs"
        )]
        let output = Command::new(std::env::current_exe().expect("current test executable"))
            .args(["--exact", &test_name, "--nocapture"])
            .env(SCENARIO_ENV, name)
            .env("WAYLAND_DISPLAY", MISSING_WAYLAND_DISPLAY)
            .env_remove("DISPLAY")
            .output()
            .expect("run the scenario in a child process");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "scenario failed\nstdout:\n{stdout}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr),
        );
    }

    fn assert_error_contains(result: anyhow::Result<()>, expected: &str) {
        let error = result.expect_err("the transition fails");
        assert!(
            format!("{error:#}").contains(expected),
            "expected an error containing {expected:?}, got: {error:#}"
        );
    }
}
