use std::{collections::VecDeque, fmt, mem, ops::Range, sync::Arc};

use collections::{HashMap, HashSet};
use language::{Bias, Point, Selection, SelectionGoal};
use multi_buffer::{ToOffset as _, ToPoint as _};
use parking_lot::Mutex;
use smallvec::{SmallVec, smallvec};
use util::RangeExt as _;

use crate::{
    Anchor, DisplayPoint, DisplayRow, SelectionExt as _,
    display_map::{DisplaySnapshot, ToDisplayPoint as _},
    movement::{
        self, HorizontalMotion, HorizontalMotionSelection, HorizontalSelectionEnd,
        TextLayoutDetails,
    },
    scroll::ScrollPixelOffset,
};

#[derive(Clone, Default)]
pub(crate) struct HorizontalOrigins(Arc<Mutex<RecordedOrigins>>, bool);

#[derive(Clone, Default)]
pub(super) struct HorizontalGoals {
    origins: HorizontalOrigins,
    details: Option<Arc<TextLayoutDetails>>,
    epoch: usize,
    edit_count: usize,
    provisional: Arc<HashMap<usize, Provisional>>,
    ancestors: Arc<Vec<HorizontalIntent>>,
}

impl HorizontalOrigins {
    pub(crate) fn record(&self, x: ScrollPixelOffset, origin: Anchor, details: &TextLayoutDetails) {
        self.record_origin(
            x,
            GoalOrigin {
                anchor: Some(origin),
                x,
                projections: None,
            },
            details,
        );
    }

    pub(crate) fn record_measurement(
        &self,
        snapshot: &DisplaySnapshot,
        point: DisplayPoint,
        x: ScrollPixelOffset,
        unsettled: bool,
        details: &TextLayoutDetails,
    ) {
        let origin = {
            let recorded = self.0.lock();
            recorded
                .selection
                .and_then(|id| recorded.measurement_origins.get(&id))
                .and_then(|origins| origins.iter().find(|(endpoint, _)| *endpoint == point))
                .map(|(_, origin)| origin.clone())
        };
        if let Some(origin) = origin {
            self.record_origin(x, origin, details);
        } else if unsettled {
            self.record(
                x,
                snapshot.display_point_to_anchor(point, Bias::Left),
                details,
            );
        }
    }

    pub(crate) fn without_motion_recording(&self) -> Self {
        Self(self.0.clone(), true)
    }

    pub(crate) fn record_motion(&self, motion: HorizontalMotion, details: &TextLayoutDetails) {
        if self.1 {
            return;
        }
        let mut recorded = self.0.lock();
        if let Some(id) = recorded.selection
            && recorded.motion_sources.contains_key(&id)
        {
            recorded.motions.entry(id).or_default().steps.push(motion);
            if recorded.details.is_none() {
                recorded.details = Some(details.detached());
            }
        }
    }

    pub(crate) fn record_motion_selection(&self, selection: HorizontalMotionSelection) {
        let mut recorded = self.0.lock();
        if let Some(id) = recorded.selection
            && recorded.motion_sources.contains_key(&id)
        {
            recorded.motions.entry(id).or_default().selection = selection;
        }
    }

    pub(crate) fn record_bias(&self, bias: Bias) {
        let mut recorded = self.0.lock();
        let selection = recorded.selection;
        recorded.biases.insert(selection, bias);
    }

    pub(crate) fn record_selection(
        &self,
        snapshot: &DisplaySnapshot,
        selection: &Selection<DisplayPoint>,
        end: HorizontalSelectionEnd,
        details: &TextLayoutDetails,
    ) {
        let mut recorded = self.0.lock();
        if let Some(bias) = recorded.biases.get(&recorded.selection).copied() {
            recorded.biases.insert(Some(selection.id), bias);
        }
        if recorded.motion_sources.contains_key(&selection.id) {
            recorded.motions.entry(selection.id).or_default().selection =
                HorizontalMotionSelection::Extend(end);
        }
        recorded.selection_heads.insert(
            selection.id,
            (
                selection.map(|point| snapshot.display_point_to_anchor(point, Bias::Left)),
                end,
            ),
        );
        if !recorded.selection_origins.contains_key(&selection.id) {
            let origins = recorded
                .selection
                .and_then(|id| recorded.selection_origins.get(&id))
                .unwrap_or(&recorded.origins);
            let origins = goal_positions(selection.goal)
                .into_iter()
                .filter_map(|x| {
                    origins
                        .iter()
                        .rev()
                        .find(|(bits, _)| *bits == x.to_bits())
                        .cloned()
                })
                .collect();
            recorded.selection_origins.insert(selection.id, origins);
        }
        if recorded.details.is_none() {
            recorded.details = Some(details.detached());
        }
    }

    pub(crate) fn set_selection(&self, selection: Option<usize>) {
        let mut recorded = self.0.lock();
        recorded.selection = selection;
        if let Some(selection) = selection {
            recorded.selection_origins.entry(selection).or_default();
        }
    }

    fn record_origin(&self, x: ScrollPixelOffset, origin: GoalOrigin, details: &TextLayoutDetails) {
        let mut recorded = self.0.lock();
        let origins = if let Some(selection) = recorded.selection {
            recorded.selection_origins.entry(selection).or_default()
        } else {
            if recorded.origins.len() == MAX_RECORDED_ORIGINS {
                recorded.origins.pop_front();
            }
            &mut recorded.origins
        };
        if let Some(previous) = origins.back_mut().filter(|(bits, _)| *bits == x.to_bits()) {
            previous.1 = origin;
        } else {
            origins.push_back((x.to_bits(), origin));
        }
        if recorded.details.is_none() {
            recorded.details = Some(details.detached());
        }
    }

    fn take(&self) -> RecordedOrigins {
        mem::take(&mut *self.0.lock())
    }

    fn is_empty(&self) -> bool {
        self.0.lock().is_empty()
    }
}

impl fmt::Debug for HorizontalOrigins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HorizontalOrigins").finish_non_exhaustive()
    }
}

impl HorizontalGoals {
    pub(super) fn origins(&self) -> HorizontalOrigins {
        self.origins.clone()
    }

    pub(super) fn has_provisional(&self) -> bool {
        !self.provisional.is_empty()
    }

    pub(super) fn is_unsettled(&self, snapshot: &DisplaySnapshot) -> bool {
        self.has_provisional()
            && (self.epoch != snapshot.long_row_epoch()
                || self.edit_count != snapshot.buffer_snapshot().edit_count())
    }

    pub(super) fn has_recorded_origins(&self) -> bool {
        !self.origins.is_empty()
    }

    pub(super) fn clear_provisional(&mut self) {
        self.provisional = Arc::default();
        self.ancestors = Arc::default();
        *self.origins.0.lock() = RecordedOrigins::default();
    }

    pub(super) fn freeze_positions(&mut self, selections: impl IntoIterator<Item = usize>) {
        self.commit_searches();
        self.retain_visible(selections);
        for provisional in Arc::make_mut(&mut self.provisional).values_mut() {
            for intent in &mut provisional.intents {
                intent.derived.clear();
            }
        }
        *self.origins.0.lock() = RecordedOrigins::default();
    }

    pub(super) fn retain_visible(&mut self, selections: impl IntoIterator<Item = usize>) {
        let provisional = Arc::make_mut(&mut self.provisional);
        for id in selections {
            if let Some(provisional) = provisional.get_mut(&id)
                && provisional.intents.len() > 1
            {
                let positions = provisional
                    .intents
                    .iter_mut()
                    .find(|intent| intent.selection.id == id)
                    .map(|intent| mem::take(&mut intent.positions))
                    .unwrap_or_default();
                provisional.intents = smallvec![HorizontalIntent {
                    selection: provisional.selection,
                    positions,
                    derived: SmallVec::new(),
                    search: None,
                    motion: None,
                }];
            }
        }
    }

    pub(super) fn has_searches(&self) -> bool {
        self.provisional
            .values()
            .flat_map(|provisional| &provisional.intents)
            .any(|intent| {
                intent.search.is_some()
                    || intent.motion.is_some()
                    || intent.derived.iter().any(|endpoint| {
                        endpoint
                            .range_adjustment
                            .is_some_and(|adjustment| adjustment.visible.is_some())
                    })
            })
    }

    pub(super) fn commit_searches(&mut self) {
        if !self.has_searches() {
            return;
        }
        Arc::make_mut(&mut self.provisional).retain(|_, provisional| {
            provisional.intents.retain(|intent| intent.visible());
            for intent in &mut provisional.intents {
                intent.search = None;
                intent.motion = None;
                for endpoint in &mut intent.derived {
                    if let Some(adjustment) = &mut endpoint.range_adjustment {
                        adjustment.visible = None;
                    }
                }
            }
            !provisional.intents.is_empty()
        });
        self.ancestors = Arc::default();
    }

    pub(super) fn record_search(
        &self,
        source: &Selection<Point>,
        selection: &Selection<Point>,
        above: bool,
        visible: bool,
    ) {
        self.origins.0.lock().searches.insert(
            selection.id,
            ColumnarSearch {
                source: IntentSource::Selection(source.id),
                above,
                visible,
            },
        );
    }

    pub(super) fn has_pending_positions(&self, id: usize) -> bool {
        let recorded = self.origins.0.lock();
        recorded.pending_positions.contains(&id)
            || recorded
                .selection_positions
                .get(&id)
                .is_some_and(|positions| positions.iter().any(|position| position.origin.is_some()))
    }

    pub(super) fn find_columnar_selection(
        snapshot: &DisplaySnapshot,
        mut row: DisplayRow,
        above: bool,
        positions: &Range<ScrollPixelOffset>,
        details: &TextLayoutDetails,
    ) -> (Option<Range<Point>>, bool) {
        let boundary = if above {
            DisplayRow(0)
        } else {
            snapshot.max_point().row()
        };
        let mut unsettled = false;
        while row != boundary {
            row = DisplayRow(if above { row.0 - 1 } else { row.0 + 1 });
            if snapshot.is_block_line(row) {
                continue;
            }
            let layout = snapshot.layout_row(row, details);
            let start = layout.closest_index_for_x(positions.start) as u32;
            let end = layout.closest_index_for_x(positions.end) as u32;
            unsettled |= snapshot
                .long_row(row)
                .is_some_and(|long_row| !long_row.is_settled_through(start.max(end)));
            if positions.start == positions.end || start < snapshot.line_len(row) {
                return (
                    Some(
                        DisplayPoint::new(row, start).to_point(snapshot)
                            ..DisplayPoint::new(row, end).to_point(snapshot),
                    ),
                    unsettled,
                );
            }
        }
        (None, unsettled)
    }

    pub(super) fn discard_columnar_positions(&mut self, id: usize) {
        let mut recorded = self.origins.0.lock();
        recorded.selection_positions.remove(&id);
        recorded.motions.remove(&id);
        recorded.selection_origins.entry(id).or_default().clear();
    }

    pub(super) fn merged_selections(
        &self,
        selections: &[Selection<Anchor>],
        snapshot: &DisplaySnapshot,
        include_pending_searches: bool,
    ) -> Option<Vec<Selection<Anchor>>> {
        if self.edit_count != snapshot.buffer_snapshot().edit_count()
            || !self
                .provisional
                .values()
                .any(|provisional| provisional.intents.len() > 1)
        {
            return None;
        }
        let mut intents = Vec::new();
        for selection in selections {
            if let Some(provisional) = self
                .provisional
                .get(&selection.id)
                .filter(|provisional| provisional.selection == *selection)
            {
                intents.extend(
                    provisional
                        .intents
                        .iter()
                        .filter(|intent| include_pending_searches || intent.visible())
                        .map(|intent| intent.selection),
                );
            } else {
                intents.push(*selection);
            }
        }
        intents.sort_unstable_by(|first, second| {
            first
                .start
                .cmp(&second.start, snapshot.buffer_snapshot())
                .then_with(|| first.id.cmp(&second.id))
        });
        Some(intents)
    }

    pub(super) fn is_visible(&self, id: usize) -> bool {
        self.provisional
            .get(&id)
            .is_none_or(|provisional| provisional.intents.iter().any(HorizontalIntent::visible))
    }

    pub(super) fn attach_hidden(&mut self, selections: &[Selection<Anchor>]) {
        let Some(selection) = selections.first() else {
            return;
        };
        let hidden = self
            .provisional
            .iter()
            .filter(|(_, provisional)| provisional.intents.iter().all(|intent| !intent.visible()))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        if hidden.is_empty() {
            return;
        }
        let provisional = Arc::make_mut(&mut self.provisional);
        let mut intents = SmallVec::<[HorizontalIntent; 1]>::new();
        for id in hidden {
            if let Some(hidden) = provisional.remove(&id) {
                intents.extend(hidden.intents);
            }
        }
        provisional
            .entry(selection.id)
            .or_insert_with(|| Provisional {
                selection: *selection,
                intents: smallvec![HorizontalIntent {
                    selection: *selection,
                    positions: goal_positions(selection.goal)
                        .into_iter()
                        .map(|x| GoalPosition { x, origin: None })
                        .collect(),
                    derived: SmallVec::new(),
                    search: None,
                    motion: None,
                }],
            })
            .intents
            .extend(intents);
    }

    pub(super) fn merge(
        &mut self,
        selections: &[Selection<Anchor>],
        selection: Selection<Anchor>,
        snapshot: &DisplaySnapshot,
    ) {
        if !selections
            .iter()
            .any(|selection| self.provisional.contains_key(&selection.id))
        {
            return;
        }
        let provisional = Arc::make_mut(&mut self.provisional);
        let mut intents = SmallVec::new();
        let mut exact = Vec::<Selection<Anchor>>::new();
        let buffer = snapshot.buffer_snapshot();
        for selection in selections {
            if let Some(mut provisional) = provisional.remove(&selection.id) {
                if provisional.intents.len() == 1
                    && let Some(intent) = provisional.intents.first_mut()
                {
                    intent.selection = *selection;
                }
                intents.extend(provisional.intents);
            } else if let Some(previous) = exact.last_mut()
                && super::should_merge(
                    previous.start.to_offset(buffer),
                    previous.end.to_offset(buffer),
                    selection.start.to_offset(buffer),
                    selection.end.to_offset(buffer),
                    true,
                )
            {
                if previous.end.cmp(&selection.end, buffer).is_lt() {
                    previous.end = selection.end;
                }
            } else {
                exact.push(*selection);
            }
        }
        intents.extend(exact.into_iter().map(|selection| {
            HorizontalIntent {
                selection,
                positions: goal_positions(selection.goal)
                    .into_iter()
                    .map(|x| GoalPosition { x, origin: None })
                    .collect(),
                derived: SmallVec::new(),
                search: None,
                motion: None,
            }
        }));
        provisional.insert(selection.id, Provisional { selection, intents });
    }

    pub(super) fn copy_goals(&mut self, copies: &[(usize, usize)]) {
        if copies.is_empty() {
            return;
        }
        let intents = self
            .provisional
            .values()
            .flat_map(|provisional| &provisional.intents)
            .map(|intent| (intent.selection.id, intent))
            .collect::<HashMap<_, _>>();
        let mut copied = HashMap::<usize, Provisional>::default();
        for (from, to) in copies {
            if let Some(mut intent) = copied
                .get(from)
                .and_then(|provisional| provisional.intents.first())
                .or_else(|| intents.get(from).copied())
                .cloned()
            {
                intent.selection.id = *to;
                intent.search = None;
                copied.insert(
                    *to,
                    Provisional {
                        selection: intent.selection,
                        intents: smallvec![intent],
                    },
                );
            }
        }
        Arc::make_mut(&mut self.provisional).extend(copied);
        let mut recorded = self.origins.0.lock();
        for (from, to) in copies {
            let origins = recorded
                .selection_origins
                .get(from)
                .cloned()
                .unwrap_or_default();
            recorded.selection_origins.insert(*to, origins);
            if recorded.pending_positions.contains(from) {
                recorded.pending_positions.insert(*to);
            }
            if let Some(positions) = recorded.selection_positions.get(from).cloned() {
                recorded.selection_positions.insert(*to, positions);
            }
            if let Some(head) = recorded.selection_heads.get(from).copied() {
                recorded.selection_heads.insert(*to, head);
            }
        }
    }

    pub(super) fn prepare_motion(
        &mut self,
        selections: &[Selection<DisplayPoint>],
        snapshot: &DisplaySnapshot,
        coalesce: bool,
    ) {
        if selections.is_empty()
            || !self.has_provisional()
            || self.edit_count != snapshot.buffer_snapshot().edit_count()
        {
            return;
        }
        let intents = self
            .provisional
            .values()
            .flat_map(|provisional| &provisional.intents)
            .map(|intent| (intent.selection.id, intent))
            .collect::<HashMap<_, _>>();
        let mut frozen = intents
            .into_iter()
            .map(|(id, intent)| (id, intent.clone()))
            .collect::<HashMap<_, _>>();
        for selection in selections {
            frozen
                .entry(selection.id)
                .or_insert_with(|| HorizontalIntent {
                    selection: super::selection_to_anchor_selection(
                        selection.map(|point| {
                            point
                                .to_point(snapshot)
                                .to_offset(snapshot.buffer_snapshot())
                        }),
                        snapshot.buffer_snapshot(),
                    ),
                    positions: goal_positions(selection.goal)
                        .into_iter()
                        .map(|x| GoalPosition { x, origin: None })
                        .collect(),
                    derived: SmallVec::new(),
                    search: None,
                    motion: None,
                });
        }
        let mut frozen = frozen.into_values().collect::<Vec<_>>();
        frozen.sort_unstable_by_key(|intent| intent.selection.id);
        let ancestors = Arc::make_mut(&mut self.ancestors);
        let sources = frozen
            .iter()
            .enumerate()
            .map(|(index, intent)| (intent.selection.id, ancestors.len() + index))
            .collect::<HashMap<_, _>>();
        for intent in &mut frozen {
            if let Some(search) = &mut intent.search
                && let IntentSource::Selection(id) = search.source
                && let Some(index) = sources.get(&id)
            {
                search.source = IntentSource::Ancestor(*index);
            }
        }
        ancestors.extend(frozen);
        let mut recorded = self.origins.0.lock();
        recorded.motion_batch = coalesce.then(|| {
            Arc::new(MotionBatch {
                sources: selections
                    .iter()
                    .filter_map(|selection| sources.get(&selection.id).copied())
                    .collect(),
            })
        });
        recorded.motion_sources = sources;
    }

    pub(super) fn prepare_measurements(
        &self,
        selections: &[Selection<DisplayPoint>],
        snapshot: &DisplaySnapshot,
    ) {
        if self.edit_count != snapshot.buffer_snapshot().edit_count() {
            return;
        }
        let intents = self
            .provisional
            .values()
            .flat_map(|provisional| &provisional.intents)
            .map(|intent| (intent.selection.id, intent))
            .collect::<HashMap<_, _>>();
        let mut recorded = self.origins.0.lock();
        recorded.measurement_origins.clear();
        for selection in selections {
            let Some(intent) = intents.get(&selection.id) else {
                continue;
            };
            for derived in &intent.derived {
                let Some(position) = intent.positions.get(derived.position) else {
                    continue;
                };
                for endpoint in [Endpoint::Start, Endpoint::End] {
                    if derived.inclusive_head.is_none() && derived.endpoint != endpoint {
                        continue;
                    }
                    let point = match endpoint {
                        Endpoint::Start => selection.start,
                        Endpoint::End => selection.end,
                    };
                    let anchor = derived
                        .inclusive_head
                        .as_ref()
                        .map(|head| head.head)
                        .or(derived.logical)
                        .unwrap_or_else(|| snapshot.display_point_to_anchor(point, Bias::Left));
                    let mut origin = position.origin.clone().unwrap_or(GoalOrigin {
                        anchor: None,
                        x: position.x,
                        projections: None,
                    });
                    origin.project(
                        anchor,
                        derived.bias,
                        derived
                            .inclusive_head
                            .as_ref()
                            .map(|head| (head.clone(), endpoint)),
                    );
                    recorded
                        .measurement_origins
                        .entry(selection.id)
                        .or_default()
                        .push((point, origin));
                }
            }
        }
    }

    pub(super) fn columnar_positions(
        &mut self,
        selections: &mut [Selection<Point>],
        snapshot: &DisplaySnapshot,
        details: &TextLayoutDetails,
    ) -> HashMap<usize, Range<ScrollPixelOffset>> {
        self.set_details(details);
        self.prepare_motion(
            &selections
                .iter()
                .map(|selection| selection.map(|point| point.to_display_point(snapshot)))
                .collect::<Vec<_>>(),
            snapshot,
            false,
        );
        let buffer = snapshot.buffer_snapshot();
        let intents = self
            .provisional
            .values()
            .flat_map(|provisional| &provisional.intents)
            .map(|intent| (intent.selection.id, intent))
            .collect::<HashMap<_, _>>();
        self.origins.0.lock().pending_positions = intents
            .values()
            .filter(|intent| {
                intent.search.is_some()
                    || intent.motion.is_some()
                    || intent
                        .positions
                        .iter()
                        .any(|position| position.origin.is_some())
            })
            .map(|intent| intent.selection.id)
            .collect();
        let mut ranges = HashMap::default();
        for selection in selections {
            let intent = intents.get(&selection.id).filter(|intent| {
                self.edit_count == buffer.edit_count()
                    && intent.selection.map(|anchor| anchor.to_point(buffer)) == *selection
            });
            let range = selection.display_range(snapshot).sorted();
            let mut positions = SmallVec::<[GoalPosition; 2]>::new();
            self.origins.set_selection(Some(selection.id));
            self.origins
                .record_motion_selection(HorizontalMotionSelection::Columnar);
            for (endpoint, mut point) in
                [(Endpoint::Start, range.start), (Endpoint::End, range.end)]
            {
                let source = intent
                    .filter(|intent| intent.motion.is_none())
                    .and_then(|intent| {
                        let derived = intent
                            .derived
                            .iter()
                            .find(|derived| derived.inclusive_head.is_some())
                            .or_else(|| {
                                intent
                                    .derived
                                    .iter()
                                    .find(|derived| derived.endpoint == endpoint)
                            })?;
                        Some((intent.positions.get(derived.position)?, derived))
                    });
                let mut origin = None;
                let mut unsettled_row = false;
                if let Some((source, derived)) = source {
                    let anchor = derived.inclusive_head.as_ref().map_or_else(
                        || {
                            derived.logical.unwrap_or_else(|| {
                                snapshot.display_point_to_anchor(point, Bias::Left)
                            })
                        },
                        |head| head.head,
                    );
                    if let Some(projected) = self.resolve(
                        snapshot,
                        anchor.to_display_point(snapshot).row(),
                        source.x,
                        derived.bias,
                    ) {
                        unsettled_row = snapshot
                            .long_row(projected.row())
                            .is_some_and(|row| !row.is_settled_through(projected.column()));
                        let inclusive = derived
                            .inclusive_head
                            .as_ref()
                            .map(|head| (head.clone(), endpoint));
                        point = if let Some((head, endpoint)) = &inclusive {
                            let projected = head.apply(projected, snapshot);
                            selection.reversed = projected.reversed;
                            match endpoint {
                                Endpoint::Start => projected.start,
                                Endpoint::End => projected.end,
                            }
                        } else {
                            projected
                        };
                        let mut projected_origin = source.origin.clone().unwrap_or(GoalOrigin {
                            anchor: None,
                            x: source.x,
                            projections: None,
                        });
                        projected_origin.project(anchor, derived.bias, inclusive);
                        origin = Some(projected_origin);
                        match endpoint {
                            Endpoint::Start => selection.start = point.to_point(snapshot),
                            Endpoint::End => selection.end = point.to_point(snapshot),
                        }
                    }
                }
                let x = snapshot.x_for_display_point(point, details);
                unsettled_row |= snapshot
                    .long_row(point.row())
                    .is_some_and(|row| !row.is_settled_through(point.column()));
                if !unsettled_row && source.is_none_or(|(source, _)| source.origin.is_none()) {
                    origin = None;
                }
                if origin.is_none() && unsettled_row {
                    origin = Some(GoalOrigin {
                        anchor: Some(snapshot.display_point_to_anchor(point, Bias::Left)),
                        x,
                        projections: None,
                    });
                }
                positions.push(GoalPosition { x, origin });
            }
            if selection.start > selection.end {
                mem::swap(&mut selection.start, &mut selection.end);
                selection.reversed = !selection.reversed;
            }
            if positions[0].x > positions[1].x {
                positions.swap(0, 1);
            }
            ranges.insert(selection.id, positions[0].x..positions[1].x);
            self.origins
                .0
                .lock()
                .selection_positions
                .insert(selection.id, positions);
        }
        self.origins.set_selection(None);
        ranges
    }

    pub(super) fn set_details(&mut self, details: &TextLayoutDetails) {
        self.details = Some(Arc::new(details.detached()));
    }

    pub(super) fn capture<'a>(
        &mut self,
        selections: impl IntoIterator<Item = &'a Selection<Anchor>>,
        snapshot: &DisplaySnapshot,
    ) {
        let recorded = self.origins.take();
        self.capture_with(recorded, selections, snapshot, true);
    }

    pub(super) fn capture_unmerged<'a>(
        &mut self,
        selections: impl IntoIterator<Item = &'a Selection<Anchor>>,
        snapshot: &DisplaySnapshot,
    ) {
        let recorded = self.origins.take();
        self.capture_with(recorded, selections, snapshot, false);
    }

    pub(super) fn settle(
        &mut self,
        selections: &[Selection<Anchor>],
        snapshot: &DisplaySnapshot,
    ) -> Option<Vec<Selection<Point>>> {
        if !self.is_unsettled(snapshot) {
            return None;
        }
        self.epoch = snapshot.long_row_epoch();
        let buffer = snapshot.buffer_snapshot();
        let edited = buffer.edit_count() != self.edit_count;
        if edited {
            self.commit_searches();
            self.retain_visible(selections.iter().map(|selection| selection.id));
        }
        let mut remaining = HashMap::default();
        let mut changed = false;
        let mut settled = Vec::new();
        let mut resolved_sources =
            HashMap::<IntentSource, Option<(Selection<Point>, bool, bool)>>::default();
        let mut resolved_batches = HashMap::<*const MotionBatch, ResolvedMotionBatch>::default();
        let mut intents = Vec::new();
        for selection in selections {
            if let Some(provisional) = self
                .provisional
                .get(&selection.id)
                .filter(|provisional| provisional.selection == *selection)
            {
                changed |= provisional.intents.len() > 1;
                intents.extend(provisional.intents.iter().cloned());
            } else {
                intents.push(HorizontalIntent {
                    selection: *selection,
                    positions: SmallVec::new(),
                    derived: SmallVec::new(),
                    search: None,
                    motion: None,
                });
            }
        }
        let search_sources = intents
            .iter()
            .filter_map(|intent| intent.search.as_ref())
            .map(|search| search.source)
            .collect::<HashSet<_>>();
        if !search_sources.is_empty() {
            intents.sort_unstable_by_key(|intent| intent.selection.id);
        }
        let mut ancestors = self.ancestors.as_ref().clone();
        let work = ancestors
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, intent)| (IntentSource::Ancestor(index), intent))
            .chain(
                intents
                    .into_iter()
                    .map(|intent| (IntentSource::Selection(intent.selection.id), intent)),
            )
            .collect::<Vec<_>>();
        for (key, intent) in &work {
            let selection = intent.selection;
            let resolved = selection.map(|anchor| anchor.to_point(buffer));
            let (mut positions, projected): (
                SmallVec<[GoalPosition; 2]>,
                SmallVec<[Option<ProjectedEndpoint>; 2]>,
            ) = intent
                .positions
                .iter()
                .map(|position| self.settled_position(position, snapshot))
                .unzip();
            let mut moved = resolved;
            moved.goal = map_goal(selection.goal, &intent.positions, &positions);
            let mut unsettled = positions.iter().any(|position| position.origin.is_some());
            let mut derived = if edited {
                SmallVec::new()
            } else {
                intent.derived.clone()
            };
            derived.retain(|endpoint| {
                endpoint
                    .logical
                    .is_none_or(|anchor| buffer.can_resolve(&anchor))
                    && endpoint
                        .inclusive_head
                        .as_ref()
                        .is_none_or(|head| head.can_resolve(snapshot))
            });
            let mut motion = intent.motion.clone();
            if let Some(motion) = &mut motion {
                let Some(Some((source, pending, visible))) =
                    resolved_sources.get(&IntentSource::Ancestor(motion.source))
                else {
                    changed |= matches!(key, IntentSource::Selection(_));
                    resolved_sources.insert(*key, None);
                    continue;
                };
                let Some(details) = self.details.as_deref() else {
                    continue;
                };
                let (selection, pending_motion, pending, visible) =
                    if let Some(batch) = &motion.batch {
                        let batch = resolved_batches
                            .entry(Arc::as_ptr(batch))
                            .or_insert_with(|| batch.resolve(&resolved_sources, snapshot));
                        if let Some(selection) = batch.selections.get(&source.id) {
                            let (selection, pending_motion) = motion
                                .operation
                                .apply_display(*selection, snapshot, details);
                            (selection, pending_motion, batch.unsettled, true)
                        } else if batch.unsettled {
                            let (selection, pending_motion) =
                                motion.operation.apply(*source, snapshot, details);
                            (selection, pending_motion, true, false)
                        } else {
                            changed |= matches!(key, IntentSource::Selection(_));
                            resolved_sources.insert(*key, None);
                            continue;
                        }
                    } else {
                        let (selection, pending_motion) =
                            motion.operation.apply(*source, snapshot, details);
                        (selection, pending_motion, *pending, *visible)
                    };
                moved = Selection {
                    id: moved.id,
                    ..selection
                };
                unsettled = pending || pending_motion;
                motion.visible = visible;
                positions = goal_positions(moved.goal)
                    .into_iter()
                    .map(|x| GoalPosition { x, origin: None })
                    .collect();
                derived.clear();
            }
            let mut search = intent.search.clone();
            if let Some(pending) = &search
                && !resolved_sources.contains_key(&pending.source)
            {
                if !pending.visible {
                    changed |= matches!(key, IntentSource::Selection(_));
                    resolved_sources.insert(*key, None);
                    continue;
                }
                search = None;
            }
            if let Some(search) = &mut search {
                let Some(Some((source, pending, source_visible))) =
                    resolved_sources.get(&search.source).copied()
                else {
                    changed |= matches!(key, IntentSource::Selection(_));
                    resolved_sources.insert(*key, None);
                    continue;
                };
                unsettled = pending;
                let row = if search.above {
                    source.start
                } else {
                    source.end
                }
                .to_display_point(snapshot)
                .row();
                let Some(details) = self.details.as_deref() else {
                    continue;
                };
                let (start, end) = match source.goal {
                    SelectionGoal::HorizontalRange { start, end } => (start, end),
                    _ => (
                        snapshot
                            .x_for_display_point(source.start.to_display_point(snapshot), details),
                        snapshot
                            .x_for_display_point(source.end.to_display_point(snapshot), details),
                    ),
                };
                let range = start.min(end)..start.max(end);
                positions = smallvec![
                    GoalPosition {
                        x: start,
                        origin: None
                    },
                    GoalPosition {
                        x: end,
                        origin: None
                    }
                ];
                moved.goal = SelectionGoal::HorizontalRange { start, end };
                moved.reversed = source.reversed;
                let (found, pending) = if source_visible {
                    Self::find_columnar_selection(snapshot, row, search.above, &range, details)
                } else {
                    (None, pending)
                };
                unsettled |= pending;
                search.visible = found.is_some();
                if let Some(range) = found {
                    moved.start = range.start;
                    moved.end = range.end;
                } else if unsettled {
                    moved.start = source.start;
                    moved.end = source.end;
                } else {
                    changed |= matches!(key, IntentSource::Selection(_));
                    resolved_sources.insert(*key, None);
                    continue;
                }
                if positions
                    .first()
                    .zip(positions.get(1))
                    .is_some_and(|(start, end)| start.x > end.x)
                {
                    mem::swap(&mut moved.start, &mut moved.end);
                }
                derived = if search.visible {
                    [
                        (Endpoint::Start, moved.start, 0),
                        (Endpoint::End, moved.end, 1),
                    ]
                    .into_iter()
                    .map(|(endpoint, point, position)| Derived {
                        endpoint,
                        position,
                        bias: Bias::Left,
                        logical: Some(buffer.anchor_before(point)),
                        inclusive_head: None,
                        projected: ProjectionBinding::None,
                        range_adjustment: None,
                    })
                    .collect()
                } else {
                    SmallVec::new()
                };
            }
            let mut projected_direction = None;
            for endpoint in derived.iter_mut().filter(|_| search.is_none()) {
                if endpoint.projected != ProjectionBinding::None
                    && let Some(projected) = projected.get(endpoint.position).copied().flatten()
                {
                    projected_direction = Some(projected.reversed);
                    if endpoint.projected == ProjectionBinding::Endpoint
                        && projected.id == selection.id
                        && projected.endpoint == endpoint.endpoint
                    {
                        match endpoint.endpoint {
                            Endpoint::Start => moved.start = projected.point.to_point(snapshot),
                            Endpoint::End => moved.end = projected.point.to_point(snapshot),
                        }
                        continue;
                    }
                }
                let anchor = endpoint
                    .inclusive_head
                    .as_ref()
                    .map(|head| head.head)
                    .or(endpoint.logical)
                    .unwrap_or(match endpoint.endpoint {
                        Endpoint::Start => selection.start,
                        Endpoint::End => selection.end,
                    });
                let row = anchor.to_display_point(snapshot).row();
                let Some(point) = positions
                    .get(endpoint.position)
                    .and_then(|position| self.resolve(snapshot, row, position.x, endpoint.bias))
                else {
                    continue;
                };
                unsettled |= snapshot
                    .long_row(point.row())
                    .is_some_and(|long_row| !long_row.is_settled_through(point.column()));
                if let Some(head) = &endpoint.inclusive_head {
                    let inclusive = head.apply(point, snapshot);
                    endpoint.endpoint = if inclusive.reversed {
                        Endpoint::Start
                    } else {
                        Endpoint::End
                    };
                    moved = Selection {
                        goal: moved.goal,
                        ..inclusive.map(|point| point.to_point(snapshot))
                    };
                } else {
                    let point = point.to_point(snapshot);
                    match endpoint.endpoint {
                        Endpoint::Start => moved.start = point,
                        Endpoint::End => moved.end = point,
                    }
                }
            }
            let range_crossed =
                if let SelectionGoal::HorizontalRange { start, end } = &mut moved.goal {
                    if *start > *end && positions.len() == 2 {
                        mem::swap(start, end);
                        positions.swap(0, 1);
                        for endpoint in &mut derived {
                            endpoint.position = 1 - endpoint.position;
                        }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
            let endpoints_crossed = moved.start > moved.end;
            if endpoints_crossed {
                mem::swap(&mut moved.start, &mut moved.end);
            }
            if endpoints_crossed || (range_crossed && moved.start == moved.end) {
                moved.reversed = !moved.reversed;
                for endpoint in &mut derived {
                    endpoint.endpoint = match endpoint.endpoint {
                        Endpoint::Start => Endpoint::End,
                        Endpoint::End => Endpoint::Start,
                    };
                    if let Some(adjustment) = &mut endpoint.range_adjustment {
                        adjustment.reversed = !adjustment.reversed;
                    }
                }
            }
            if let Some(reversed) = projected_direction {
                moved.reversed = reversed;
            }
            let mut range_visible = true;
            if let Some(adjustment) = derived
                .iter_mut()
                .find_map(|endpoint| endpoint.range_adjustment.as_mut())
            {
                if let Some(visible) = &mut adjustment.visible
                    && let Some(position) = positions.first()
                    && let Some(details) = self.details.as_deref()
                {
                    let row = moved.start.to_display_point(snapshot).row();
                    *visible = position.x <= snapshot.layout_row(row, details).width();
                    range_visible = *visible;
                }
                if moved.start == moved.end && adjustment.end != HorizontalSelectionEnd::None {
                    moved.end = adjustment
                        .end
                        .apply(moved.end.to_display_point(snapshot), snapshot)
                        .to_point(snapshot);
                }
                if adjustment.end == HorizontalSelectionEnd::SaturatingRight {
                    let range = moved.display_range(snapshot).sorted();
                    moved.reversed = adjustment.reversed
                        && range.end.column().saturating_sub(range.start.column()) > 1;
                }
            }
            changed |= matches!(key, IntentSource::Selection(_)) && moved != resolved;
            let visible = search.as_ref().is_none_or(|search| search.visible)
                && motion.as_ref().is_none_or(|motion| motion.visible)
                && range_visible;
            changed |= matches!(key, IntentSource::Selection(_)) && visible != intent.visible();
            resolved_sources.insert(*key, Some((moved, unsettled, visible)));
            if !unsettled {
                motion = None;
            }
            if let IntentSource::Ancestor(index) = *key {
                if !unsettled {
                    derived.clear();
                    search = None;
                }
                ancestors[index] = HorizontalIntent {
                    selection: super::selection_to_anchor_selection(
                        moved.map(|point| point.to_offset(buffer)),
                        buffer,
                    ),
                    positions,
                    derived,
                    search,
                    motion,
                };
                continue;
            }
            if !visible && !unsettled {
                changed = true;
                continue;
            }
            if unsettled || search_sources.contains(key) {
                if !unsettled {
                    derived.clear();
                    search = None;
                }
                let selection = if moved == resolved {
                    selection
                } else {
                    super::selection_to_anchor_selection(
                        moved.map(|point| point.to_offset(buffer)),
                        buffer,
                    )
                };
                remaining.insert(
                    selection.id,
                    Provisional {
                        selection,
                        intents: smallvec![HorizontalIntent {
                            selection,
                            positions,
                            derived,
                            search,
                            motion,
                        }],
                    },
                );
            }
            settled.push(moved);
        }
        self.edit_count = buffer.edit_count();
        Self::retain_search_sources(&mut remaining);
        self.provisional = Arc::new(remaining);
        self.ancestors = Arc::new(ancestors);
        self.retain_ancestors();
        changed.then_some(settled)
    }

    fn capture_with<'a>(
        &mut self,
        recorded: RecordedOrigins,
        selections: impl IntoIterator<Item = &'a Selection<Anchor>>,
        snapshot: &DisplaySnapshot,
        preserve_merged: bool,
    ) {
        let recorded_is_empty = recorded.is_empty();
        if let Some(details) = recorded.details {
            self.details = Some(Arc::new(details));
        }
        self.epoch = snapshot.long_row_epoch();
        let edited = self.edit_count != snapshot.buffer_snapshot().edit_count();
        self.edit_count = snapshot.buffer_snapshot().edit_count();
        if recorded_is_empty && self.provisional.is_empty() && !snapshot.may_have_long_rows() {
            return;
        }
        let previous = mem::take(&mut self.provisional);
        let previous_intents = previous
            .values()
            .flat_map(|provisional| &provisional.intents)
            .map(|intent| (intent.selection.id, intent))
            .collect::<HashMap<_, _>>();

        let search_sources = recorded
            .searches
            .values()
            .chain(
                previous_intents
                    .values()
                    .filter_map(|intent| intent.search.as_ref()),
            )
            .map(|search| search.source)
            .collect::<HashSet<_>>();
        let buffer = snapshot.buffer_snapshot();
        let mut provisional = HashMap::default();
        for selection in selections {
            if preserve_merged
                && !edited
                && let Some(previous) = previous
                    .get(&selection.id)
                    .filter(|previous| previous.selection == *selection)
                && (previous.intents.len() > 1 || recorded_is_empty)
            {
                provisional.insert(selection.id, previous.clone());
                continue;
            }
            let own = previous_intents.get(&selection.id).copied();
            let motion = if edited || recorded.searches.contains_key(&selection.id) {
                None
            } else {
                recorded
                    .motions
                    .get(&selection.id)
                    .filter(|operation| {
                        !operation.steps.is_empty()
                            || matches!(operation.selection, HorizontalMotionSelection::Columnar)
                    })
                    .zip(recorded.motion_sources.get(&selection.id))
                    .map(|(operation, source)| DeferredMotion {
                        source: *source,
                        batch: recorded.motion_batch.clone(),
                        operation: operation.clone(),
                        visible: self
                            .ancestors
                            .get(*source)
                            .is_none_or(HorizontalIntent::visible),
                    })
                    .or_else(|| {
                        own.filter(|own| own.selection == *selection)
                            .and_then(|own| own.motion.clone())
                    })
            };
            let xs = goal_positions(selection.goal);
            if let Some(motion) = motion {
                provisional.insert(
                    selection.id,
                    Provisional {
                        selection: *selection,
                        intents: smallvec![HorizontalIntent {
                            selection: *selection,
                            positions: xs
                                .into_iter()
                                .map(|x| GoalPosition { x, origin: None })
                                .collect(),
                            derived: SmallVec::new(),
                            search: None,
                            motion: Some(motion),
                        }],
                    },
                );
                continue;
            }
            if xs.is_empty() {
                continue;
            }
            let recorded_origins = recorded
                .selection_origins
                .get(&selection.id)
                .unwrap_or(&recorded.origins);
            let positions = xs
                .into_iter()
                .enumerate()
                .map(|(index, x)| {
                    let bits = x.to_bits();
                    if let Some(position) = recorded
                        .selection_positions
                        .get(&selection.id)
                        .and_then(|positions| positions.get(index))
                        .filter(|position| position.x.to_bits() == bits)
                    {
                        return position.clone();
                    }
                    let origin = recorded_origins
                        .iter()
                        .rev()
                        .find(|(x, _)| *x == bits)
                        .map(|(_, origin)| origin.clone())
                        .or_else(|| {
                            own.and_then(|own| {
                                let position = if matches!(
                                    selection.goal,
                                    SelectionGoal::HorizontalRange { .. }
                                ) && matches!(
                                    own.selection.goal,
                                    SelectionGoal::HorizontalRange { .. }
                                ) {
                                    own.positions
                                        .get(index)
                                        .filter(|position| position.x.to_bits() == bits)
                                } else {
                                    own.positions
                                        .iter()
                                        .rev()
                                        .find(|position| position.x.to_bits() == bits)
                                };
                                position.and_then(|position| position.origin.clone())
                            })
                        })
                        .filter(|origin| {
                            origin
                                .anchor
                                .is_none_or(|anchor| buffer.can_resolve(&anchor))
                                && origin
                                    .projections()
                                    .all(|projection| projection.can_resolve(snapshot))
                        });
                    GoalPosition { x, origin }
                })
                .collect::<SmallVec<[GoalPosition; 2]>>();
            let derived = if !edited || previous.is_empty() || !preserve_merged {
                if !edited
                    && !recorded.selection_heads.contains_key(&selection.id)
                    && let Some(own) = own.filter(|own| own.selection == *selection)
                    && own.derived.iter().any(|derived| {
                        derived.inclusive_head.is_some()
                            || derived.projected != ProjectionBinding::None
                            || derived.logical.is_some()
                    })
                {
                    own.derived.clone()
                } else {
                    let recorded_selection = recorded
                        .selection_heads
                        .get(&selection.id)
                        .filter(|(logical, _)| logical.goal == selection.goal);
                    self.derived_endpoints(
                        selection,
                        &positions,
                        recorded_selection,
                        recorded.selection_positions.contains_key(&selection.id),
                        recorded
                            .biases
                            .get(&Some(selection.id))
                            .or_else(|| recorded.biases.get(&None))
                            .copied(),
                        snapshot,
                    )
                }
            } else {
                SmallVec::new()
            };
            let search = recorded.searches.get(&selection.id).cloned().or_else(|| {
                own.filter(|own| !edited && own.selection == *selection)
                    .and_then(|own| own.search.clone())
            });
            if positions.iter().any(|position| position.origin.is_some())
                || !derived.is_empty()
                || search.is_some()
                || search_sources.contains(&IntentSource::Selection(selection.id))
            {
                provisional.insert(
                    selection.id,
                    Provisional {
                        selection: *selection,
                        intents: smallvec![HorizontalIntent {
                            selection: *selection,
                            positions,
                            derived,
                            search,
                            motion: None,
                        }],
                    },
                );
            }
        }
        Self::retain_search_sources(&mut provisional);
        self.provisional = Arc::new(provisional);
        self.retain_ancestors();
    }

    fn retain_ancestors(&mut self) {
        if self.ancestors.is_empty() {
            return;
        }
        let mut indices = Vec::new();
        let mut batches = HashSet::default();
        for intent in self
            .provisional
            .values()
            .flat_map(|provisional| &provisional.intents)
        {
            intent.push_ancestors(&mut batches, &mut indices);
        }
        let mut retained = vec![false; self.ancestors.len()];
        while let Some(index) = indices.pop() {
            if !mem::replace(&mut retained[index], true) {
                self.ancestors[index].push_ancestors(&mut batches, &mut indices);
            }
        }
        if retained.iter().all(|retain| *retain) {
            return;
        }
        let mut next_index = 0;
        let indices = retained
            .iter()
            .map(|retain| {
                let index = next_index;
                next_index += usize::from(*retain);
                index
            })
            .collect::<Vec<_>>();
        let ancestors = Arc::make_mut(&mut self.ancestors);
        let mut retained = retained.into_iter();
        ancestors.retain(|_| retained.next().unwrap_or(false));
        let mut batches = HashMap::<*const MotionBatch, Arc<MotionBatch>>::default();
        for intent in ancestors.iter_mut().chain(
            Arc::make_mut(&mut self.provisional)
                .values_mut()
                .flat_map(|provisional| &mut provisional.intents),
        ) {
            if let Some(motion) = &mut intent.motion {
                motion.source = indices[motion.source];
                if let Some(batch) = &mut motion.batch {
                    *batch = batches
                        .entry(Arc::as_ptr(batch))
                        .or_insert_with(|| {
                            Arc::new(MotionBatch {
                                sources: batch
                                    .sources
                                    .iter()
                                    .map(|index| indices[*index])
                                    .collect(),
                            })
                        })
                        .clone();
                }
            }
            if let Some(search) = &mut intent.search
                && let IntentSource::Ancestor(index) = &mut search.source
            {
                *index = indices[*index];
            }
        }
    }

    fn retain_search_sources(provisional: &mut HashMap<usize, Provisional>) {
        let sources = provisional
            .values()
            .flat_map(|provisional| &provisional.intents)
            .filter_map(|intent| intent.search.as_ref())
            .map(|search| search.source)
            .collect::<HashSet<_>>();
        provisional.retain(|_, provisional| {
            provisional.intents.len() > 1
                || provisional.intents.iter().any(|intent| {
                    sources.contains(&IntentSource::Selection(intent.selection.id))
                        || intent.search.is_some()
                        || intent.motion.is_some()
                        || !intent.derived.is_empty()
                        || intent
                            .positions
                            .iter()
                            .any(|position| position.origin.is_some())
                })
        });
    }

    fn derived_endpoints(
        &self,
        selection: &Selection<Anchor>,
        positions: &[GoalPosition],
        recorded: Option<&(Selection<Anchor>, HorizontalSelectionEnd)>,
        projected: bool,
        bias: Option<Bias>,
        snapshot: &DisplaySnapshot,
    ) -> SmallVec<[Derived; 2]> {
        let mut derived = SmallVec::new();
        let is_range = matches!(selection.goal, SelectionGoal::HorizontalRange { .. });
        let bias = if is_range { Some(Bias::Left) } else { bias };
        let resolved = selection.map(|anchor| anchor.to_display_point(snapshot));
        let inclusive_head = recorded.filter(|_| !is_range).and_then(|(logical, end)| {
            let head = InclusiveHead {
                id: selection.id,
                head: logical.head(),
                tail: selection.tail(),
                reversed: selection.reversed,
                end: *end,
            };
            let projected = head.apply(head.head.to_display_point(snapshot), snapshot);
            (projected.range() == resolved.range() && projected.reversed == resolved.reversed)
                .then(|| Arc::new(head))
        });
        for (endpoint, anchor) in [
            (Endpoint::Start, selection.start),
            (Endpoint::End, selection.end),
        ] {
            if !resolved.is_empty() && !is_range && anchor == selection.tail() {
                continue;
            }
            if projected
                && let Some(index) = positions.iter().position(|position| {
                    position
                        .origin
                        .as_ref()
                        .and_then(|origin| origin.projections.as_ref())
                        .and_then(|projection| projection.inclusive.as_ref())
                        .is_some_and(|(head, projected)| {
                            head.id == selection.id && *projected == endpoint
                        })
                })
            {
                derived.push(Derived {
                    endpoint,
                    position: index,
                    bias: Bias::Right,
                    logical: None,
                    inclusive_head: None,
                    projected: ProjectionBinding::Endpoint,
                    range_adjustment: None,
                });
                continue;
            }
            let point = anchor.to_display_point(snapshot);
            let mut logical = None;
            let mut range_adjustment = None;
            if is_range && let Some((recorded, adjustment)) = recorded {
                let start = recorded.start.to_display_point(snapshot);
                let raw_end = recorded.end.to_display_point(snapshot);
                let adjusted = if raw_end <= start {
                    adjustment.apply(start, snapshot)
                } else {
                    raw_end
                };
                if (endpoint == Endpoint::Start && point == start)
                    || (endpoint == Endpoint::End && point == adjusted)
                {
                    logical = Some(match endpoint {
                        Endpoint::Start => recorded.start,
                        Endpoint::End => recorded.end,
                    });
                    if endpoint == Endpoint::End {
                        range_adjustment = Some(RangeAdjustment {
                            end: *adjustment,
                            reversed: recorded.reversed,
                            visible: (*adjustment == HorizontalSelectionEnd::SaturatingRight
                                && (positions.iter().any(|position| position.origin.is_some())
                                    || snapshot.long_row(point.row()).is_some_and(|row| {
                                        !row.is_settled_through(point.column())
                                    })))
                            .then(|| {
                                positions.first().is_none_or(|position| {
                                    self.details.as_deref().is_none_or(|details| {
                                        position.x
                                            <= snapshot.layout_row(point.row(), details).width()
                                    })
                                })
                            }),
                        });
                    }
                }
            }
            let inclusive_head = inclusive_head
                .as_ref()
                .filter(|_| anchor == selection.head())
                .cloned();
            let logical = inclusive_head.as_ref().map(|head| head.head).or(logical);
            let point = logical.map_or(point, |anchor| anchor.to_display_point(snapshot));
            let unsettled_row = snapshot
                .long_row(point.row())
                .is_some_and(|row| !row.is_settled_through(point.column()));
            let found = positions.iter().enumerate().find_map(|(index, position)| {
                if position.origin.is_none()
                    && !unsettled_row
                    && !(range_adjustment
                        .is_some_and(|adjustment| adjustment.end != HorizontalSelectionEnd::None)
                        && positions.iter().any(|position| position.origin.is_some()))
                {
                    return None;
                }
                if is_range && index != if endpoint == Endpoint::Start { 0 } else { 1 } {
                    return None;
                }
                [
                    Some(bias.unwrap_or(Bias::Right)),
                    bias.is_none().then_some(Bias::Left),
                ]
                .into_iter()
                .flatten()
                .find_map(|bias| {
                    (self.resolve(snapshot, point.row(), position.x, bias)? == point).then(|| {
                        Derived {
                            endpoint,
                            position: index,
                            bias,
                            logical,
                            inclusive_head: inclusive_head.clone(),
                            projected: if projected {
                                ProjectionBinding::Direction
                            } else {
                                ProjectionBinding::None
                            },
                            range_adjustment,
                        }
                    })
                })
            });
            derived.extend(found);
        }
        derived
    }

    fn settled_position(
        &self,
        position: &GoalPosition,
        snapshot: &DisplaySnapshot,
    ) -> (GoalPosition, Option<ProjectedEndpoint>) {
        let Some(origin) = &position.origin else {
            return (position.clone(), None);
        };
        let buffer = snapshot.buffer_snapshot();
        if origin
            .anchor
            .is_some_and(|anchor| !buffer.can_resolve(&anchor))
            || origin
                .projections()
                .any(|projection| !projection.can_resolve(snapshot))
        {
            return (
                GoalPosition {
                    x: position.x,
                    origin: None,
                },
                None,
            );
        }
        let measure = |point: DisplayPoint| match snapshot.long_row(point.row()) {
            Some(row) => (
                row.measured_x_for_column(point.column()),
                row.is_settled_through(point.column()),
            ),
            None => (
                self.details.as_deref().map_or(position.x, |details| {
                    snapshot
                        .layout_row(point.row(), details)
                        .x_for_index(point.column() as usize)
                }),
                true,
            ),
        };
        let (mut x, mut settled) = origin.anchor.map_or((origin.x, true), |anchor| {
            measure(anchor.to_display_point(snapshot))
        });
        let projections = origin.projections().collect::<Vec<_>>();
        let mut resolved_prefix = 0;
        let mut resolved_anchor = None;
        let mut projected_endpoint = None;
        for (index, projection) in projections.iter().rev().enumerate() {
            let row = projection.anchor.to_display_point(snapshot).row();
            let Some(point) = self.resolve(snapshot, row, x, projection.bias) else {
                return (position.clone(), None);
            };
            settled &= snapshot
                .long_row(point.row())
                .is_none_or(|row| row.is_settled_through(point.column()));
            projected_endpoint = projection.inclusive.as_ref().map(|(head, endpoint)| {
                let selection = head.apply(point, snapshot);
                ProjectedEndpoint {
                    id: head.id,
                    endpoint: *endpoint,
                    point: match endpoint {
                        Endpoint::Start => selection.start,
                        Endpoint::End => selection.end,
                    },
                    reversed: selection.reversed,
                }
            });
            let point = projected_endpoint.map_or(point, |projected| projected.point);
            let (projected_x, projected_settled) = measure(point);
            x = projected_x;
            settled &= projected_settled;
            if settled {
                resolved_prefix = index + 1;
                resolved_anchor = Some(snapshot.display_point_to_anchor(point, Bias::Left));
            }
        }
        let origin = if settled {
            None
        } else if resolved_prefix > 0 {
            let mut origin = GoalOrigin {
                anchor: resolved_anchor,
                x,
                projections: None,
            };
            for projection in projections.iter().rev().skip(resolved_prefix) {
                origin.project(
                    projection.anchor,
                    projection.bias,
                    projection.inclusive.clone(),
                );
            }
            Some(origin)
        } else {
            Some(origin.clone())
        };
        (GoalPosition { x, origin }, projected_endpoint)
    }

    fn resolve(
        &self,
        snapshot: &DisplaySnapshot,
        row: DisplayRow,
        x: ScrollPixelOffset,
        bias: Bias,
    ) -> Option<DisplayPoint> {
        let column = match snapshot.long_row(row) {
            Some(long_row) => long_row.measured_closest_column_for_x(x),
            None => snapshot
                .layout_row(row, self.details.as_deref()?)
                .closest_index_for_x(x) as u32,
        };
        let point = DisplayPoint::new(row, column);
        let clipped = snapshot.clip_point(point, bias);
        Some(if clipped.row() != row {
            snapshot.clip_point(
                point,
                if bias == Bias::Left {
                    Bias::Right
                } else {
                    Bias::Left
                },
            )
        } else {
            clipped
        })
    }
}

impl fmt::Debug for HorizontalGoals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HorizontalGoals")
            .field("epoch", &self.epoch)
            .field("provisional", &self.provisional)
            .finish_non_exhaustive()
    }
}

const MAX_RECORDED_ORIGINS: usize = 256;

#[derive(Clone, Default)]
struct RecordedOrigins {
    origins: VecDeque<(u64, GoalOrigin)>,
    selection_origins: HashMap<usize, VecDeque<(u64, GoalOrigin)>>,
    measurement_origins: HashMap<usize, SmallVec<[(DisplayPoint, GoalOrigin); 2]>>,
    biases: HashMap<Option<usize>, Bias>,
    motion_sources: HashMap<usize, usize>,
    motion_batch: Option<Arc<MotionBatch>>,
    motions: HashMap<usize, RecordedMotion>,
    searches: HashMap<usize, ColumnarSearch>,
    pending_positions: HashSet<usize>,
    selection_positions: HashMap<usize, SmallVec<[GoalPosition; 2]>>,
    selection_heads: HashMap<usize, (Selection<Anchor>, HorizontalSelectionEnd)>,
    selection: Option<usize>,
    details: Option<TextLayoutDetails>,
}

impl RecordedOrigins {
    fn is_empty(&self) -> bool {
        self.origins.is_empty()
            && self.selection_origins.is_empty()
            && self.selection_positions.is_empty()
            && self.selection_heads.is_empty()
            && self.searches.is_empty()
            && self.motions.is_empty()
    }
}

#[derive(Clone, Debug)]
struct Provisional {
    selection: Selection<Anchor>,
    intents: SmallVec<[HorizontalIntent; 1]>,
}

#[derive(Clone, Debug)]
struct HorizontalIntent {
    selection: Selection<Anchor>,
    positions: SmallVec<[GoalPosition; 2]>,
    derived: SmallVec<[Derived; 2]>,
    search: Option<ColumnarSearch>,
    motion: Option<DeferredMotion>,
}

impl HorizontalIntent {
    fn push_ancestors(&self, batches: &mut HashSet<*const MotionBatch>, indices: &mut Vec<usize>) {
        if let Some(motion) = &self.motion {
            if let Some(batch) = &motion.batch {
                if batches.insert(Arc::as_ptr(batch)) {
                    indices.extend(&batch.sources);
                }
            } else {
                indices.push(motion.source);
            }
        }
        if let Some(search) = &self.search
            && let IntentSource::Ancestor(index) = search.source
        {
            indices.push(index);
        }
    }

    fn visible(&self) -> bool {
        self.search.as_ref().is_none_or(|search| search.visible)
            && self.motion.as_ref().is_none_or(|motion| motion.visible)
            && self.derived.iter().all(|endpoint| {
                endpoint
                    .range_adjustment
                    .is_none_or(|adjustment| adjustment.visible != Some(false))
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum IntentSource {
    Selection(usize),
    Ancestor(usize),
}

#[derive(Clone, Debug)]
struct MotionBatch {
    sources: Vec<usize>,
}

impl MotionBatch {
    fn resolve(
        &self,
        resolved: &HashMap<IntentSource, Option<(Selection<Point>, bool, bool)>>,
        snapshot: &DisplaySnapshot,
    ) -> ResolvedMotionBatch {
        let mut unsettled = false;
        let mut selections = Vec::new();
        for source in &self.sources {
            if let Some(Some((selection, pending, visible))) =
                resolved.get(&IntentSource::Ancestor(*source))
            {
                unsettled |= *pending;
                if *visible {
                    selections.push(*selection);
                }
            }
        }
        super::sort_selections(&mut selections);
        let selections = super::coalesced_selection_groups(&selections)
            .map(|(_, selection)| super::selection_to_display_selection(selection, snapshot));
        ResolvedMotionBatch {
            selections: super::coalesce_selections(selections)
                .map(|selection| (selection.id, selection))
                .collect(),
            unsettled,
        }
    }
}

struct ResolvedMotionBatch {
    selections: HashMap<usize, Selection<DisplayPoint>>,
    unsettled: bool,
}

#[derive(Clone, Debug, Default)]
struct RecordedMotion {
    steps: SmallVec<[HorizontalMotion; 1]>,
    selection: HorizontalMotionSelection,
}

impl RecordedMotion {
    fn apply(
        &self,
        selection: Selection<Point>,
        snapshot: &DisplaySnapshot,
        details: &TextLayoutDetails,
    ) -> (Selection<Point>, bool) {
        let selection = if matches!(self.selection, HorizontalMotionSelection::Columnar) {
            selection.map(|point| point.to_display_point(snapshot))
        } else {
            super::selection_to_display_selection(selection, snapshot)
        };
        self.apply_display(selection, snapshot, details)
    }

    fn apply_display(
        &self,
        mut selection: Selection<DisplayPoint>,
        snapshot: &DisplaySnapshot,
        details: &TextLayoutDetails,
    ) -> (Selection<Point>, bool) {
        let details = details.detached();
        let pending = |point: DisplayPoint| {
            snapshot
                .long_row(point.row())
                .is_some_and(|row| !row.is_settled_through(point.column()))
        };
        let mut unsettled = pending(selection.start) || pending(selection.end);
        if matches!(self.selection, HorizontalMotionSelection::Columnar) {
            let start = snapshot.x_for_display_point(selection.start, &details);
            let end = snapshot.x_for_display_point(selection.end, &details);
            selection.goal = SelectionGoal::HorizontalRange {
                start: start.min(end),
                end: start.max(end),
            };
            return (selection.map(|point| point.to_point(snapshot)), unsettled);
        }
        let mut head = match self.selection {
            HorizontalMotionSelection::Start => selection.start,
            HorizontalMotionSelection::End => selection.end,
            _ => selection.head(),
        };
        if !selection.is_empty() && !selection.reversed {
            head = match self.selection {
                HorizontalMotionSelection::InclusiveHead
                | HorizontalMotionSelection::Extend(HorizontalSelectionEnd::Right) => {
                    movement::left(snapshot, head)
                }
                HorizontalMotionSelection::Extend(HorizontalSelectionEnd::SaturatingRight) => {
                    movement::saturating_left(snapshot, head)
                }
                _ => head,
            };
        }
        let mut goal = if !selection.is_empty()
            && matches!(
                self.selection,
                HorizontalMotionSelection::Start | HorizontalMotionSelection::End
            ) {
            SelectionGoal::None
        } else {
            selection.goal
        };
        for motion in &self.steps {
            unsettled |= pending(head);
            let (moved, pending_motion) = motion.apply_with_pending(snapshot, head, goal, &details);
            (head, goal) = moved;
            unsettled |= pending_motion || pending(head);
        }
        let extend = match self.selection {
            HorizontalMotionSelection::Extend(end) => Some(end),
            HorizontalMotionSelection::ExtendIfNonempty if !selection.is_empty() => {
                Some(HorizontalSelectionEnd::None)
            }
            _ => None,
        };
        if let Some(end) = extend {
            let inclusive = InclusiveHead {
                id: selection.id,
                head: snapshot.display_point_to_anchor(head, Bias::Left),
                tail: snapshot.display_point_to_anchor(selection.tail(), Bias::Left),
                reversed: selection.reversed,
                end,
            };
            selection = inclusive.apply(head, snapshot);
            selection.goal = goal;
        } else {
            selection.collapse_to(head, goal);
        }
        (selection.map(|point| point.to_point(snapshot)), unsettled)
    }
}

#[derive(Clone, Debug)]
struct DeferredMotion {
    source: usize,
    batch: Option<Arc<MotionBatch>>,
    operation: RecordedMotion,
    visible: bool,
}

#[derive(Clone, Debug)]
struct ColumnarSearch {
    source: IntentSource,
    above: bool,
    visible: bool,
}

#[derive(Clone, Debug)]
struct GoalPosition {
    x: ScrollPixelOffset,
    origin: Option<GoalOrigin>,
}

#[derive(Clone, Debug)]
struct GoalOrigin {
    anchor: Option<Anchor>,
    x: ScrollPixelOffset,
    projections: Option<Arc<GoalProjection>>,
}

impl GoalOrigin {
    fn projections(&self) -> impl Iterator<Item = &GoalProjection> {
        std::iter::successors(self.projections.as_deref(), |projection| {
            projection.previous.as_deref()
        })
    }

    fn project(
        &mut self,
        anchor: Anchor,
        bias: Bias,
        inclusive: Option<(Arc<InclusiveHead>, Endpoint)>,
    ) {
        if inclusive.is_none()
            && self.projections.as_ref().is_some_and(|projection| {
                projection.inclusive.is_none()
                    && projection.anchor == anchor
                    && projection.bias == bias
            })
        {
            return;
        }
        self.projections = Some(Arc::new(GoalProjection {
            anchor,
            bias,
            inclusive,
            previous: self.projections.take(),
        }));
    }
}

#[derive(Clone, Debug)]
struct GoalProjection {
    anchor: Anchor,
    bias: Bias,
    inclusive: Option<(Arc<InclusiveHead>, Endpoint)>,
    previous: Option<Arc<GoalProjection>>,
}

impl GoalProjection {
    fn can_resolve(&self, snapshot: &DisplaySnapshot) -> bool {
        let buffer = snapshot.buffer_snapshot();
        buffer.can_resolve(&self.anchor)
            && self
                .inclusive
                .as_ref()
                .is_none_or(|(head, _)| head.can_resolve(snapshot))
    }
}

impl Drop for GoalProjection {
    fn drop(&mut self) {
        let mut previous = self.previous.take();
        while let Some(projection) = previous {
            let Ok(mut projection) = Arc::try_unwrap(projection) else {
                break;
            };
            previous = projection.previous.take();
        }
    }
}

#[derive(Clone, Debug)]
struct Derived {
    endpoint: Endpoint,
    position: usize,
    bias: Bias,
    logical: Option<Anchor>,
    inclusive_head: Option<Arc<InclusiveHead>>,
    projected: ProjectionBinding,
    range_adjustment: Option<RangeAdjustment>,
}

#[derive(Clone, Copy, Debug)]
struct RangeAdjustment {
    end: HorizontalSelectionEnd,
    reversed: bool,
    visible: Option<bool>,
}

#[derive(Clone, Debug)]
struct InclusiveHead {
    id: usize,
    head: Anchor,
    tail: Anchor,
    reversed: bool,
    end: HorizontalSelectionEnd,
}

impl InclusiveHead {
    fn can_resolve(&self, snapshot: &DisplaySnapshot) -> bool {
        let buffer = snapshot.buffer_snapshot();
        buffer.can_resolve(&self.head) && buffer.can_resolve(&self.tail)
    }

    fn apply(&self, head: DisplayPoint, snapshot: &DisplaySnapshot) -> Selection<DisplayPoint> {
        let tail = self.tail.to_display_point(snapshot);
        let mut selection = Selection {
            id: self.id,
            start: tail,
            end: tail,
            reversed: self.reversed,
            goal: SelectionGoal::None,
        };
        selection.set_head(head, SelectionGoal::None);
        if !selection.reversed {
            selection.end = self.end.apply(selection.end, snapshot);
        }
        if self.reversed && !selection.reversed {
            selection.start = match self.end {
                HorizontalSelectionEnd::None => selection.start,
                HorizontalSelectionEnd::Right => movement::left(snapshot, selection.start),
                HorizontalSelectionEnd::SaturatingRight => {
                    movement::saturating_left(snapshot, selection.start)
                }
            };
        } else if !self.reversed && selection.reversed {
            selection.end = self.end.apply(selection.end, snapshot);
        }
        selection
    }
}

impl HorizontalSelectionEnd {
    fn apply(self, point: DisplayPoint, snapshot: &DisplaySnapshot) -> DisplayPoint {
        match self {
            Self::None => point,
            Self::Right => movement::right(snapshot, point),
            Self::SaturatingRight => movement::saturating_right(snapshot, point),
        }
    }
}

#[derive(Clone, Copy)]
struct ProjectedEndpoint {
    id: usize,
    endpoint: Endpoint,
    point: DisplayPoint,
    reversed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectionBinding {
    None,
    Direction,
    Endpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Endpoint {
    Start,
    End,
}

fn goal_positions(goal: SelectionGoal) -> SmallVec<[ScrollPixelOffset; 2]> {
    let mut positions = match goal {
        SelectionGoal::None => SmallVec::<[ScrollPixelOffset; 2]>::new(),
        SelectionGoal::HorizontalPosition(x) | SelectionGoal::WrappedHorizontalPosition((_, x)) => {
            smallvec![x]
        }
        SelectionGoal::HorizontalRange { start, end } => smallvec![start, end],
    };
    positions.retain(|x| x.is_finite());
    positions
}

fn map_goal(goal: SelectionGoal, from: &[GoalPosition], to: &[GoalPosition]) -> SelectionGoal {
    let map = |x: ScrollPixelOffset| {
        from.iter()
            .zip(to)
            .find(|(from, _)| from.x.to_bits() == x.to_bits())
            .map_or(x, |(_, to)| to.x)
    };
    match goal {
        SelectionGoal::None => SelectionGoal::None,
        SelectionGoal::HorizontalPosition(x) => SelectionGoal::HorizontalPosition(map(x)),
        SelectionGoal::WrappedHorizontalPosition((row, x)) => {
            SelectionGoal::WrappedHorizontalPosition((row, map(x)))
        }
        SelectionGoal::HorizontalRange { start, end } => match to {
            [start, end] => SelectionGoal::HorizontalRange {
                start: start.x,
                end: end.x,
            },
            _ => SelectionGoal::HorizontalRange {
                start: map(start),
                end: map(end),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AddSelectionAbove, AddSelectionBelow, Editor, EditorMode, Inlay, MultiBuffer,
        SelectionEffects, actions::SwapSelectionEnds, editor_tests::init_test,
        test::ShapingTextSystem,
    };
    use gpui::{
        AbsoluteLength, Context, Entity, TestAppContext, TextStyleRefinement, VisualTestContext,
        Window, px,
    };
    use language::language_settings::SoftWrap;
    use zed_actions::editor::MoveDown;

    #[gpui::test]
    fn test_selection_scoped_horizontal_goals(cx: &mut TestAppContext) {
        init_test(cx, |_| {});
        let editor = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple("abcd\nabcd\nabcd\nabcd", cx);
            let mut editor = Editor::new(EditorMode::full(), buffer, None, window, cx);
            editor.set_soft_wrap_mode(SoftWrap::None, cx);
            editor
        });
        editor
            .update(cx, |editor, window, cx| {
                let snapshot = editor.display_snapshot(cx);
                let details = editor.text_layout_details(window, cx);
                let buffer = snapshot.buffer_snapshot();
                let x = snapshot.x_for_display_point(DisplayPoint::new(DisplayRow(0), 4), &details);
                let mut selections = [
                    (0, Point::new(1, 4)..Point::new(1, 4)),
                    (1, Point::new(2, 4)..Point::new(3, 4)),
                ]
                .map(|(id, range)| Selection {
                    id,
                    start: buffer.anchor_before(range.start),
                    end: buffer.anchor_before(range.end),
                    reversed: false,
                    goal: SelectionGoal::HorizontalPosition(x),
                });
                let mut goals = HorizontalGoals::default();
                for (selection, source) in
                    selections.iter().zip([Point::new(0, 2), Point::new(2, 3)])
                {
                    goals.origins.set_selection(Some(selection.id));
                    goals
                        .origins
                        .record(x, buffer.anchor_before(source), &details);
                }
                goals.origins.set_selection(None);
                goals.copy_goals(&[(0, 2)]);
                selections[0].id = 2;
                goals.capture(&selections, &snapshot);
                goals.copy_goals(&[(2, 3)]);
                selections[0].id = 3;
                goals.capture(&selections, &snapshot);
                goals.epoch = snapshot.long_row_epoch().wrapping_sub(1);
                let settled = goals
                    .settle(&selections, &snapshot)
                    .expect("settled selections");
                assert_eq!(
                    settled
                        .iter()
                        .map(|selection| selection.start..selection.end)
                        .collect::<Vec<_>>(),
                    [
                        Point::new(1, 2)..Point::new(1, 2),
                        Point::new(2, 4)..Point::new(3, 3)
                    ]
                );

                for (
                    (end, range, ((head_column, tail_column, reversed, columnar), expected)),
                    source_columns,
                ) in [
                    ((3, 0, false, false), (0, 3, false)),
                    ((3, 4, true, false), (2, 4, true)),
                    ((4, 0, false, false), (0, 3, false)),
                    ((3, 0, false, true), (0, 3, false)),
                    ((3, 4, true, true), (2, 4, true)),
                    ((3, 3, false, false), (2, 4, true)),
                    ((1, 2, true, false), (1, 3, false)),
                    ((3, 3, false, true), (2, 4, true)),
                    ((4, 0, false, true), (0, 3, false)),
                ]
                .into_iter()
                .map(|case| (HorizontalSelectionEnd::Right, false, case))
                .chain([
                    (
                        HorizontalSelectionEnd::SaturatingRight,
                        true,
                        ((3, 3, false, false), (2, 3, false)),
                    ),
                    (
                        HorizontalSelectionEnd::SaturatingRight,
                        true,
                        ((4, 4, false, false), (2, 3, false)),
                    ),
                    (
                        HorizontalSelectionEnd::SaturatingRight,
                        true,
                        ((3, 1, false, false), (2, 3, false)),
                    ),
                    (
                        HorizontalSelectionEnd::None,
                        true,
                        ((3, 3, false, false), (2, 2, false)),
                    ),
                ])
                .map(|case| (case, [2, 2]))
                .chain([
                    (
                        (
                            HorizontalSelectionEnd::SaturatingRight,
                            true,
                            ((2, 2, true, false), (2, 4, true)),
                        ),
                        [2, 4],
                    ),
                    (
                        (
                            HorizontalSelectionEnd::SaturatingRight,
                            true,
                            ((1, 3, true, false), (2, 3, false)),
                        ),
                        [2, 2],
                    ),
                    (
                        (
                            HorizontalSelectionEnd::SaturatingRight,
                            true,
                            ((1, 4, true, false), (2, 4, true)),
                        ),
                        [2, 4],
                    ),
                    (
                        (
                            HorizontalSelectionEnd::SaturatingRight,
                            true,
                            ((1, 3, true, false), (1, 3, false)),
                        ),
                        [3, 1],
                    ),
                    (
                        (
                            HorizontalSelectionEnd::SaturatingRight,
                            true,
                            ((4, 4, true, false), (2, 4, true)),
                        ),
                        [2, 4],
                    ),
                    (
                        (
                            HorizontalSelectionEnd::None,
                            true,
                            ((1, 2, true, false), (2, 3, true)),
                        ),
                        [2, 3],
                    ),
                ]) {
                    let mut goals = HorizontalGoals::default();
                    let details = TextLayoutDetails {
                        horizontal_origins: goals.origins(),
                        ..details.detached()
                    };
                    let head = DisplayPoint::new(DisplayRow(1), head_column);
                    let tail = DisplayPoint::new(DisplayRow(1), tail_column);
                    let (start, finish) = if reversed { (head, tail) } else { (tail, head) };
                    let goal = if range {
                        let start = snapshot.x_for_display_point(start, &details);
                        let mut end = snapshot.x_for_display_point(finish, &details);
                        if reversed && head_column == tail_column {
                            end += snapshot
                                .x_for_display_point(DisplayPoint::new(DisplayRow(1), 1), &details)
                                * 0.4;
                        }
                        SelectionGoal::HorizontalRange { start, end }
                    } else {
                        SelectionGoal::HorizontalPosition(
                            snapshot.x_for_display_point(head, &details),
                        )
                    };
                    let mut selection = Selection {
                        id: 0,
                        start,
                        end: finish,
                        reversed,
                        goal,
                    };
                    if !range {
                        goals.origins.set_selection(Some(selection.id));
                    }
                    for (x, source_column) in goal_positions(goal).into_iter().zip(source_columns) {
                        goals.origins.record(
                            x,
                            buffer.anchor_before(Point::new(0, source_column)),
                            &details,
                        );
                    }
                    details.record_horizontal_selection(&snapshot, &selection, end);
                    if (range && selection.is_empty()) || (!range && !reversed) {
                        selection.end = end.apply(selection.end, &snapshot);
                    }
                    if range && end == HorizontalSelectionEnd::SaturatingRight {
                        selection.reversed &= selection.end.column() - selection.start.column() > 1;
                    }
                    if range && reversed && head_column == 2 && tail_column == 2 {
                        assert_eq!(
                            (selection.start, selection.end, selection.reversed),
                            (
                                DisplayPoint::new(DisplayRow(1), 2),
                                DisplayPoint::new(DisplayRow(1), 3),
                                false
                            )
                        );
                    }
                    goals.origins.set_selection(None);
                    let mut selection =
                        selection.map(|point| snapshot.display_point_to_anchor(point, Bias::Left));
                    goals.capture([&selection], &snapshot);
                    goals.origins.set_selection(Some(selection.id));
                    goals.origins.set_selection(None);
                    goals.capture([&selection], &snapshot);
                    if columnar {
                        let mut selections = [selection.map(|anchor| anchor.to_point(buffer))];
                        let positions =
                            goals.columnar_positions(&mut selections, &snapshot, &details);
                        let range = &positions[&selection.id];
                        selections[0].goal = SelectionGoal::HorizontalRange {
                            start: range.start,
                            end: range.end,
                        };
                        selection = selections[0].map(|point| buffer.anchor_before(point));
                        goals.capture([&selection], &snapshot);
                    }
                    goals.epoch = snapshot.long_row_epoch().wrapping_sub(1);
                    let settled = goals
                        .settle(&[selection], &snapshot)
                        .expect("inclusive head correction");
                    assert_eq!(
                        settled
                            .iter()
                            .map(|selection| (selection.range(), selection.reversed))
                            .collect::<Vec<_>>(),
                        [(
                            Point::new(1, expected.0)..Point::new(1, expected.1),
                            expected.2
                        )],
                        "{head_column}, {tail_column}, {reversed}, {columnar}, {range}, {end:?}"
                    );
                    assert!(!goals.has_provisional());
                }

                for above in [false, true] {
                    for (tail, expected_tail) in [(4, 3), (2, 1)] {
                        let row = if above { 2 } else { 0 };
                        editor.change_selections(
                            SelectionEffects::no_scroll(),
                            window,
                            cx,
                            |selections| {
                                selections
                                    .select_ranges([Point::new(row, 4)..Point::new(row, tail)]);
                            },
                        );
                        for seed_origins in [false, true] {
                            if seed_origins {
                                for (column, source_column) in [(2, 1), (4, 3)] {
                                    let x = snapshot.x_for_display_point(
                                        DisplayPoint::new(DisplayRow(0), column),
                                        &details,
                                    );
                                    editor.selections.horizontal_origins().record(
                                        x,
                                        buffer.anchor_before(Point::new(3, source_column)),
                                        &details,
                                    );
                                }
                                editor.change_selections(
                                    SelectionEffects::no_scroll(),
                                    window,
                                    cx,
                                    |_| {},
                                );
                            }
                            add_columnar_selection(editor, above, window, cx);
                        }
                        editor.selections.horizontal_goals.epoch =
                            snapshot.long_row_epoch().wrapping_sub(1);
                        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |_| {});
                        assert_eq!(
                            editor.selections.ranges::<Point>(&snapshot),
                            (0..3)
                                .map(|row| Point::new(row, 3)..Point::new(row, expected_tail))
                                .collect::<Vec<_>>()
                        );
                    }
                }
                editor.set_text_style_refinement(TextStyleRefinement {
                    font_size: Some(AbsoluteLength::Pixels(px(10.))),
                    ..TextStyleRefinement::default()
                });
                replace_row(editor, 0, 4, &"x".repeat(34_008), cx);
            })
            .expect("editor window");
        cx.run_until_parked();
        cx.background_executor.set_block_on_ticks(0..=0);
        editor
            .update(cx, |editor, window, cx| {
                let suffix = shaped_source(4);
                replace_row(editor, 3, 4, &suffix, cx);
                let snapshot = editor.display_snapshot(cx);
                let details = editor.text_layout_details(window, cx);
                let prefix = snapshot.long_row(DisplayRow(0)).expect("prefix row");
                assert!(prefix.is_settled_through(34_003));
                let x = prefix.measured_x_for_column(34_003);
                let anchor = snapshot
                    .display_point_to_anchor(DisplayPoint::new(DisplayRow(0), 34_003), Bias::Left);
                let mut origin = GoalOrigin {
                    anchor: None,
                    x,
                    projections: None,
                };
                for _ in 0..64 {
                    origin.project(anchor, Bias::Right, None);
                }
                assert_eq!(origin.projections().count(), 1);
                origin.project(
                    snapshot.display_point_to_anchor(
                        DisplayPoint::new(DisplayRow(3), 34_003),
                        Bias::Left,
                    ),
                    Bias::Right,
                    None,
                );
                let mut goals = HorizontalGoals::default();
                goals.set_details(&details);
                let (position, _) = goals.settled_position(
                    &GoalPosition {
                        x,
                        origin: Some(origin),
                    },
                    &snapshot,
                );
                let origin = position.origin.as_ref().expect("pending suffix");
                assert_eq!(origin.anchor, Some(anchor));
                assert_eq!(origin.projections().count(), 1);
                let mut style = details.editor_style.clone();
                style.text.font_size =
                    AbsoluteLength::Pixels(style.text.font_size.to_pixels(details.rem_size) * 2.);
                editor.set_style(style, window, cx);
                let snapshot = editor.display_snapshot(cx);
                goals.set_details(&editor.text_layout_details(window, cx));
                assert_eq!(goals.settled_position(&position, &snapshot).0.x, x * 2.);
            })
            .expect("editor window");
    }

    #[gpui::test]
    fn test_provisional_horizontal_collisions() {
        for (ranges, destination_len, follow_up) in [
            (false, 34_003, CollisionFollowUp::Settle),
            (true, 34_003, CollisionFollowUp::MoveAgain),
            (false, 34_003, CollisionFollowUp::MoveAgain),
            (false, 34_003, CollisionFollowUp::Columnar),
            (
                false,
                34_002,
                CollisionFollowUp::VerticalToColumnar {
                    wide_len: 16,
                    settled: false,
                },
            ),
            (
                true,
                34_002,
                CollisionFollowUp::VerticalToColumnar {
                    wide_len: 16,
                    settled: false,
                },
            ),
            (
                false,
                34_001,
                CollisionFollowUp::VerticalToColumnar {
                    wide_len: 4,
                    settled: false,
                },
            ),
            (
                false,
                34_001,
                CollisionFollowUp::VerticalToColumnar {
                    wide_len: 4,
                    settled: true,
                },
            ),
            (false, 34_002, CollisionFollowUp::Settle),
            (false, 34_003, CollisionFollowUp::Cancel),
            (false, 34_003, CollisionFollowUp::Replace),
            (false, 34_003, CollisionFollowUp::Edit),
            (false, 34_003, CollisionFollowUp::Pending),
            (false, 34_003, CollisionFollowUp::Abandon),
            (false, 34_003, CollisionFollowUp::Commit),
            (true, 34_003, CollisionFollowUp::Commit),
            (false, 34_003, CollisionFollowUp::Eof),
        ] {
            let mut cx = shaping_context();
            let destination = "x".repeat(destination_len);
            let next_destination = if let CollisionFollowUp::VerticalToColumnar { .. } = follow_up {
                "x".repeat(34_008)
            } else {
                destination.clone()
            };
            let text = if follow_up == CollisionFollowUp::Eof {
                format!("\n{destination}")
            } else {
                format!("\n{destination}\n{next_destination}\nx")
            };
            let (editor, mut cx) = settled_editor(&text, &mut cx);
            let cx = &mut cx;
            let wide_len = if let CollisionFollowUp::VerticalToColumnar { wide_len, .. } = follow_up
            {
                wide_len
            } else {
                4
            };
            let source = shaped_source(wide_len);
            let correction = wide_len as u32 / 4;
            let source_ranges = if ranges {
                [34_001..34_003, 34_003..34_004]
            } else {
                [34_003..34_003, 34_004..34_004]
            };
            let mut expected = source_ranges
                .iter()
                .enumerate()
                .map(|(index, range)| {
                    (
                        Point::new(1, (range.start - correction).min(destination_len as u32))
                            ..Point::new(1, (range.end - correction).min(destination_len as u32)),
                        ranges && index == 0,
                    )
                })
                .collect::<Vec<_>>();
            expected.dedup();
            let mut expected_goal = None;
            editor.update_in(cx, |editor, window, cx| {
                replace_row(editor, 0, 0, &source, cx);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_ranges(source_ranges.iter().enumerate().map(
                        |(index, range)| {
                            if ranges && index == 0 {
                                Point::new(0, range.end)..Point::new(0, range.start)
                            } else {
                                Point::new(0, range.start)..Point::new(0, range.end)
                            }
                        },
                    ));
                });
                let snapshot = editor.display_snapshot(cx);
                let details = editor.text_layout_details(window, cx);
                let advance =
                    snapshot.x_for_display_point(DisplayPoint::new(DisplayRow(1), 1), &details);
                assert_eq!(
                    snapshot
                        .x_for_display_point(DisplayPoint::new(DisplayRow(0), 34_004), &details),
                    advance * 34_004.
                );
                assert!(
                    !snapshot
                        .long_row(DisplayRow(0))
                        .expect("source row")
                        .is_settled_through(34_004)
                );
                assert!(
                    snapshot
                        .long_row(DisplayRow(1))
                        .expect("destination row")
                        .is_settled_through(destination_len as u32)
                );
                if follow_up == CollisionFollowUp::Columnar {
                    add_columnar_selection(editor, false, window, cx);
                } else if ranges {
                    move_collision_ranges_down(editor, window, cx);
                } else {
                    editor.move_down(&MoveDown, window, cx);
                }
                assert_eq!(
                    editor.selections.count(),
                    if follow_up == CollisionFollowUp::Columnar {
                        3
                    } else {
                        1
                    },
                    "{ranges}, {follow_up:?}"
                );
                assert_eq!(
                    editor
                        .selections
                        .horizontal_goals
                        .provisional
                        .values()
                        .map(|provisional| provisional.intents.len())
                        .sum::<usize>(),
                    if follow_up == CollisionFollowUp::Columnar {
                        4
                    } else {
                        2
                    }
                );
                if !ranges && follow_up == CollisionFollowUp::Settle {
                    editor.swap_selection_ends(&SwapSelectionEnds, window, cx);
                    editor.change_selections(
                        SelectionEffects::no_scroll(),
                        window,
                        cx,
                        |selections| {
                            selections.move_offsets_with(&mut |_, _| {});
                        },
                    );
                }
                match follow_up {
                    CollisionFollowUp::Settle => {}

                    CollisionFollowUp::Commit => {
                        editor.change_selections(
                            SelectionEffects::no_scroll(),
                            window,
                            cx,
                            |selections| {
                                selections.move_with(&mut |map, selection| {
                                    if ranges {
                                        selection.end = DisplayPoint::new(DisplayRow(2), 5);
                                        selection.reversed = false;
                                    } else {
                                        let (point, goal) = crate::movement::down(
                                            map,
                                            selection.head(),
                                            selection.goal,
                                            false,
                                            &details,
                                        );
                                        selection.collapse_to(point, goal);
                                    }
                                });
                            },
                        );
                        expected = vec![(
                            if ranges {
                                Point::new(1, 34_000)..Point::new(2, 5)
                            } else {
                                Point::new(2, 34_002)..Point::new(2, 34_002)
                            },
                            false,
                        )];
                    }
                    CollisionFollowUp::Eof => {
                        let previous = editor.selections.disjoint_anchors_arc();
                        for _ in 0..64 {
                            add_columnar_selection(editor, false, window, cx);
                        }
                        assert_eq!(editor.selections.disjoint_anchors(), previous.as_ref());
                        assert_eq!(
                            editor
                                .selections
                                .horizontal_goals
                                .provisional
                                .values()
                                .flat_map(|group| &group.intents)
                                .flat_map(|intent| &intent.positions)
                                .filter_map(|position| position.origin.as_ref())
                                .map(|origin| origin.projections().count())
                                .sum::<usize>(),
                            0
                        );
                    }
                    CollisionFollowUp::MoveAgain => {
                        if ranges {
                            move_collision_ranges_down(editor, window, cx);
                        } else {
                            editor.move_down(&MoveDown, window, cx);
                        }
                        for (range, _) in &mut expected {
                            range.start.row = 2;
                            range.end.row = 2;
                        }
                    }
                    CollisionFollowUp::Columnar => {
                        add_columnar_selection(editor, false, window, cx);
                        let destination_selections = expected.clone();
                        expected = source_ranges
                            .iter()
                            .map(|range| {
                                (Point::new(0, range.start)..Point::new(0, range.end), false)
                            })
                            .collect();
                        expected.extend(destination_selections.iter().cloned());
                        expected.extend(destination_selections.into_iter().map(
                            |(range, reversed)| {
                                (
                                    Point::new(2, range.start.column)
                                        ..Point::new(2, range.end.column),
                                    reversed,
                                )
                            },
                        ));
                    }
                    CollisionFollowUp::VerticalToColumnar { settled, .. } => {
                        if !settled {
                            add_columnar_selection(editor, false, window, cx);
                        }
                        expected.extend(expected.clone().into_iter().map(|(range, reversed)| {
                            (
                                Point::new(2, range.start.column)..Point::new(2, range.end.column),
                                reversed,
                            )
                        }));
                    }
                    CollisionFollowUp::Cancel => {
                        editor.change_selections(
                            SelectionEffects::no_scroll(),
                            window,
                            cx,
                            |selections| {
                                assert!(selections.try_cancel());
                            },
                        );
                        expected = vec![(Point::new(1, 34_003)..Point::new(1, 34_003), false)];
                    }
                    CollisionFollowUp::Replace => {
                        editor.change_selections(
                            SelectionEffects::no_scroll(),
                            window,
                            cx,
                            |selections| {
                                selections.select_ranges([Point::new(2, 2)..Point::new(2, 2)]);
                            },
                        );
                        expected = vec![(Point::new(2, 2)..Point::new(2, 2), false)];
                    }
                    CollisionFollowUp::Edit => {
                        editor.buffer.update(cx, |buffer, cx| {
                            buffer.edit([(Point::new(3, 1)..Point::new(3, 1), "y")], None, cx);
                        });
                        expected = vec![(Point::new(1, 34_003)..Point::new(1, 34_003), false)];
                        expected_goal = Some(SelectionGoal::HorizontalPosition(advance * 34_002.));
                    }
                    CollisionFollowUp::Pending => {
                        editor.change_selections(
                            SelectionEffects::no_scroll(),
                            window,
                            cx,
                            |selections| {
                                let snapshot = selections.display_snapshot();
                                let anchor =
                                    snapshot.buffer_snapshot().anchor_before(Point::new(3, 0));
                                selections.set_pending_anchor_range(
                                    anchor..anchor,
                                    crate::SelectMode::Character,
                                );
                            },
                        );
                        editor.change_selections(
                            SelectionEffects::no_scroll(),
                            window,
                            cx,
                            |selections| {
                                selections.clear_pending();
                            },
                        );
                        expected = vec![(Point::new(1, 34_003)..Point::new(1, 34_003), false)];
                    }
                    CollisionFollowUp::Abandon => {
                        editor.change_selections(
                            SelectionEffects::no_scroll(),
                            window,
                            cx,
                            |selections| {
                                selections.move_cursors_with(&mut |map, point, _| {
                                    (crate::movement::left(map, point), SelectionGoal::None)
                                });
                            },
                        );
                        expected = vec![(Point::new(1, 34_002)..Point::new(1, 34_002), false)];
                    }
                }
            });
            cx.background_executor.set_block_on_ticks(0..=50);
            cx.run_until_parked();
            if matches!(
                follow_up,
                CollisionFollowUp::VerticalToColumnar { settled: true, .. }
            ) {
                editor.update_in(cx, |editor, window, cx| {
                    let snapshot = editor.display_snapshot(cx);
                    assert!(
                        snapshot
                            .long_row(DisplayRow(0))
                            .expect("source row")
                            .is_settled_through(34_004)
                    );
                    add_columnar_selection(editor, false, window, cx);
                });
                cx.run_until_parked();
            }
            editor.update(cx, |editor, cx| {
                let snapshot = editor.display_snapshot(cx);
                assert_eq!(
                    editor
                        .selections
                        .all::<Point>(&snapshot)
                        .into_iter()
                        .map(|selection| (selection.range(), selection.reversed))
                        .collect::<Vec<_>>(),
                    expected,
                    "{ranges}, {destination_len}, {follow_up:?}"
                );
                if let Some(goal) = expected_goal {
                    assert_eq!(editor.selections.first_anchor().goal, goal);
                }
                assert!(!editor.selections.has_provisional_horizontal_goals());
            });
        }
    }

    #[gpui::test]
    fn test_horizontal_correction_collision() {
        for clamped_exact_pair in [false, true] {
            let mut cx = shaping_context();
            let destination = "x".repeat(if clamped_exact_pair { 34_003 } else { 34_008 });
            let next_destination = "x".repeat(34_008);
            let second_source = shaped_source(8);
            let (editor, mut cx) = settled_editor(
                &format!("\n{second_source}\n{destination}\n{next_destination}"),
                &mut cx,
            );
            let cx = &mut cx;
            editor.update_in(cx, |editor, window, cx| {
                let first_source = shaped_source(4);
                replace_row(editor, 0, 0, &first_source, cx);
                let details = editor.text_layout_details(window, cx);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    let mut points = vec![Point::new(0, 34_003), Point::new(1, 34_005)];
                    if clamped_exact_pair {
                        points.push(Point::new(1, 34_006));
                    }
                    selections.select_ranges(points.into_iter().map(|point| point..point));
                    selections.move_with_horizontal_goals(&mut |map, selection| {
                        let x = if selection.head().row() == DisplayRow(1) {
                            let advance = map
                                .x_for_display_point(DisplayPoint::new(DisplayRow(3), 1), &details);
                            let x = advance * f64::from(selection.head().column());
                            details.horizontal_origins.record(
                                x,
                                map.display_point_to_anchor(selection.head(), Bias::Left),
                                &details,
                            );
                            x
                        } else {
                            map.x_for_display_point(selection.head(), &details)
                        };
                        let point = DisplayPoint::new(
                            DisplayRow(2),
                            map.display_column_for_x(DisplayRow(2), x, &details),
                        );
                        selection.collapse_to(point, SelectionGoal::HorizontalPosition(x));
                    });
                });
                let snapshot = editor.display_snapshot(cx);
                let expected = if clamped_exact_pair {
                    vec![Point::new(2, 34_003)..Point::new(2, 34_003)]
                } else {
                    vec![
                        Point::new(2, 34_003)..Point::new(2, 34_003),
                        Point::new(2, 34_005)..Point::new(2, 34_005),
                    ]
                };
                assert_eq!(editor.selections.ranges::<Point>(&snapshot), expected);
                let source = snapshot.long_row(DisplayRow(1)).expect("second source");
                assert!(source.is_settled_through(34_006));
                assert!(
                    !snapshot
                        .long_row(DisplayRow(0))
                        .expect("first source")
                        .is_settled_through(34_003)
                );
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |_| {});
                let snapshot = editor.display_snapshot(cx);
                assert_eq!(
                    editor.selections.ranges::<Point>(&snapshot),
                    [Point::new(2, 34_003)..Point::new(2, 34_003),]
                );
                assert_eq!(
                    editor
                        .selections
                        .horizontal_goals
                        .provisional
                        .values()
                        .map(|provisional| provisional.intents.len())
                        .sum::<usize>(),
                    2
                );
                if clamped_exact_pair {
                    editor.move_down(&MoveDown, window, cx);
                }
            });
            cx.background_executor.set_block_on_ticks(0..=50);
            cx.run_until_parked();
            editor.update(cx, |editor, cx| {
                let snapshot = editor.display_snapshot(cx);
                let row = if clamped_exact_pair { 3 } else { 2 };
                assert_eq!(
                    editor.selections.ranges::<Point>(&snapshot),
                    [
                        Point::new(row, 34_002)..Point::new(row, 34_002),
                        Point::new(row, 34_003)..Point::new(row, 34_003),
                    ],
                    "clamped_exact_pair={clamped_exact_pair}"
                );
                assert!(!editor.selections.has_provisional_horizontal_goals());
            });
        }
    }

    #[gpui::test]
    fn test_horizontal_range_correction_direction() {
        for (destination_len, reversed, rebase, move_again) in [
            (34_008, false, false, false),
            (34_008, true, false, false),
            (33_998, false, false, false),
            (33_998, true, false, false),
            (34_001, false, true, false),
            (34_001, false, true, true),
        ] {
            let mut cx = shaping_context();
            let plain = "x".repeat(34_008);
            let destination = "x".repeat(destination_len);
            let (editor, mut cx) = settled_editor(
                &format!("{plain}\n\n{destination}\n{plain}\n{plain}"),
                &mut cx,
            );
            let cx = &mut cx;
            let advance = editor.update_in(cx, |editor, window, cx| {
                let source = shaped_source(16);
                replace_row(editor, 1, 0, &source, cx);
                let details = editor.text_layout_details(window, cx);
                let advance = editor.change_selections(
                    SelectionEffects::no_scroll(),
                    window,
                    cx,
                    |selections| {
                        let map = selections.display_snapshot();
                        let advance =
                            map.x_for_display_point(DisplayPoint::new(DisplayRow(0), 1), &details);
                        let id = selections.new_selection_id();
                        details.horizontal_origins.set_selection(Some(id));
                        let start = map.x_for_display_point(
                            DisplayPoint::new(DisplayRow(0), 34_001),
                            &details,
                        );
                        let end = map.x_for_display_point(
                            DisplayPoint::new(DisplayRow(1), 34_003),
                            &details,
                        );
                        details.horizontal_origins.set_selection(None);
                        assert_eq!((start, end), (advance * 34_001., advance * 34_003.));
                        assert!(
                            !map.long_row(DisplayRow(1))
                                .expect("source row")
                                .is_settled_through(34_003)
                        );
                        selections.select(vec![Selection {
                            id,
                            start: Point::new(2, 34_001_u32.min(destination_len as u32)),
                            end: Point::new(2, 34_003_u32.min(destination_len as u32)),
                            reversed,
                            goal: SelectionGoal::HorizontalRange { start, end },
                        }]);
                        advance
                    },
                );
                if rebase {
                    add_columnar_selection(editor, false, window, cx);
                    let snapshot = editor.display_snapshot(cx);
                    assert_eq!(
                        editor
                            .selections
                            .all::<Point>(&snapshot)
                            .into_iter()
                            .map(|selection| selection.goal)
                            .collect::<Vec<_>>(),
                        [SelectionGoal::HorizontalRange {
                            start: advance * 34_001.,
                            end: advance * 34_001.
                        }; 2]
                    );
                    if move_again {
                        editor.move_down(&MoveDown, window, cx);
                    }
                }
                advance
            });
            cx.background_executor.set_block_on_ticks(0..=50);
            cx.run_until_parked();
            editor.update_in(cx, |editor, window, cx| {
                let snapshot = editor.display_snapshot(cx);
                if move_again {
                    assert_eq!(
                        editor
                            .selections
                            .all::<Point>(&snapshot)
                            .into_iter()
                            .map(|selection| (
                                selection.range(),
                                selection.reversed,
                                selection.goal
                            ))
                            .collect::<Vec<_>>(),
                        [3, 4].map(|row| (
                            Point::new(row, 34_001)..Point::new(row, 34_001),
                            false,
                            SelectionGoal::HorizontalPosition(advance * 34_001.)
                        ))
                    );
                    return;
                }
                let start = 33_999_u32.min(destination_len as u32);
                let end = 34_001_u32.min(destination_len as u32);
                assert_eq!(
                    editor
                        .selections
                        .all::<Point>(&snapshot)
                        .into_iter()
                        .map(|selection| (selection.range(), selection.reversed, selection.goal))
                        .collect::<Vec<_>>(),
                    (2..=if rebase { 3 } else { 2 })
                        .map(|row| (
                            Point::new(row, start)..Point::new(row, end),
                            !reversed,
                            SelectionGoal::HorizontalRange {
                                start: advance * 33_999.,
                                end: advance * 34_001.
                            }
                        ))
                        .collect::<Vec<_>>()
                );
                add_columnar_selection(editor, false, window, cx);
                let snapshot = editor.display_snapshot(cx);
                assert_eq!(
                    editor
                        .selections
                        .all::<Point>(&snapshot)
                        .into_iter()
                        .map(|selection| (selection.range(), selection.reversed))
                        .collect::<Vec<_>>(),
                    (2..=if rebase { 4 } else { 3 })
                        .map(|row| (Point::new(row, start)..Point::new(row, end), !reversed))
                        .collect::<Vec<_>>(),
                    "destination_len={destination_len}, reversed={reversed}, rebase={rebase}"
                );
            });
        }
    }

    #[gpui::test]
    fn test_horizontal_range_rebased_motion_settled_first() {
        let mut cx = shaping_context();
        let plain = "x".repeat(34_008);
        let destination = "x".repeat(34_001);
        let (editor, mut cx) = settled_editor(
            &format!("{plain}\n\n{destination}\n{plain}\n{plain}"),
            &mut cx,
        );
        let advance = editor.update_in(&mut cx, |editor, window, cx| {
            replace_row(editor, 1, 0, &shaped_source(16), cx);
            let details = editor.text_layout_details(window, cx);
            let advance =
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    let snapshot = selections.display_snapshot();
                    let advance =
                        snapshot.x_for_display_point(DisplayPoint::new(DisplayRow(0), 1), &details);
                    let id = selections.new_selection_id();
                    details.horizontal_origins.set_selection(Some(id));
                    let start = snapshot
                        .x_for_display_point(DisplayPoint::new(DisplayRow(0), 34_001), &details);
                    let end = snapshot
                        .x_for_display_point(DisplayPoint::new(DisplayRow(1), 34_003), &details);
                    details.horizontal_origins.set_selection(None);
                    assert_eq!((start, end), (advance * 34_001., advance * 34_003.));
                    selections.select(vec![Selection {
                        id,
                        start: Point::new(2, 34_001),
                        end: Point::new(2, 34_001),
                        reversed: false,
                        goal: SelectionGoal::HorizontalRange { start, end },
                    }]);
                    advance
                });
            assert_deferred_row(editor, 1, cx);
            add_columnar_selection(editor, false, window, cx);
            advance
        });
        cx.background_executor.set_block_on_ticks(0..=50);
        cx.run_until_parked();
        editor.update_in(&mut cx, |editor, window, cx| {
            let snapshot = editor.display_snapshot(cx);
            let details = editor.text_layout_details(window, cx);
            assert_eq!(
                snapshot.x_for_display_point(DisplayPoint::new(DisplayRow(1), 34_003), &details),
                advance * 33_999.
            );
            assert_eq!(
                editor
                    .selections
                    .all::<Point>(&snapshot)
                    .into_iter()
                    .map(|selection| (selection.range(), selection.reversed, selection.goal))
                    .collect::<Vec<_>>(),
                [2, 3].map(|row| (
                    Point::new(row, 33_999)..Point::new(row, 34_001),
                    true,
                    SelectionGoal::HorizontalRange {
                        start: advance * 33_999.,
                        end: advance * 34_001.,
                    }
                ))
            );
            assert!(!editor.selections.has_provisional_horizontal_goals());
            editor.move_down(&MoveDown, window, cx);
            let snapshot = editor.display_snapshot(cx);
            assert_eq!(
                editor
                    .selections
                    .all::<Point>(&snapshot)
                    .into_iter()
                    .map(|selection| (selection.range(), selection.reversed, selection.goal))
                    .collect::<Vec<_>>(),
                [3, 4].map(|row| (
                    Point::new(row, 34_001)..Point::new(row, 34_001),
                    false,
                    SelectionGoal::HorizontalPosition(advance * 34_001.)
                ))
            );
        });
    }

    #[gpui::test]
    fn test_initial_columnar_groups_keep_distinct_horizontal_origins() {
        for (above, tail_offset, head_offset, repeated) in
            [(false, 0, 0, false), (true, 0, 4, true)]
        {
            let mut cx = shaping_context();
            cx.background_executor.set_block_on_ticks(0..=0);
            let source = format!(
                "{}{}{}",
                "x".repeat(33_000),
                "w".repeat(3_000),
                "x".repeat(33_000)
            );
            let plain = "x".repeat(69_000);
            let rows_per_group = if repeated { 3 } else { 2 };
            let source_row = if above { rows_per_group - 1 } else { 0 };
            let mut rows = vec![plain.as_str(); (2 * rows_per_group) as usize];
            rows[source_row as usize] = &source;
            let window = cx.add_window(|window, cx| {
                let buffer = MultiBuffer::build_simple(&rows.join("\n"), cx);
                Editor::new(EditorMode::full(), buffer, None, window, cx)
            });
            let cx = &mut VisualTestContext::from_window(*window, &mut cx);
            let editor = window.root(cx).expect("editor");
            editor.update_in(cx, |editor, window, cx| {
                let snapshot = editor.display_snapshot(cx);
                let details = editor.text_layout_details(window, cx);
                for row in 0..2 * rows_per_group {
                    snapshot.x_for_display_point(DisplayPoint::new(DisplayRow(row), 0), &details);
                }
                let [source_x, plain_x] = [source_row, source_row + rows_per_group].map(|row| {
                    snapshot
                        .x_for_display_point(DisplayPoint::new(DisplayRow(row), 69_000), &details)
                });
                assert_eq!(source_x, plain_x);
                assert!(
                    !snapshot
                        .long_row(DisplayRow(source_row))
                        .expect("source row")
                        .is_settled_through(69_000)
                );
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_ranges([source_row, source_row + rows_per_group].map(
                        |row| {
                            Point::new(row, 69_000 - tail_offset)
                                ..Point::new(row, 69_000 - head_offset)
                        },
                    ));
                });
                for _ in 1..rows_per_group {
                    add_columnar_selection(editor, above, window, cx);
                }
                assert_eq!(editor.selections.count(), (2 * rows_per_group) as usize);
            });

            cx.background_executor.set_block_on_ticks(0..=50);
            cx.run_until_parked();
            editor.update(cx, |editor, cx| {
                let snapshot = editor.display_snapshot(cx);
                assert_eq!(
                    editor.selections.ranges::<Point>(&snapshot),
                    (0..2 * rows_per_group)
                        .map(|row| {
                            let column = if row < rows_per_group && row != source_row { 68_250 } else { 69_000 };
                            Point::new(row, column - tail_offset)
                                ..Point::new(row, column - head_offset)
                        })
                        .collect::<Vec<_>>(),
                    "above={above}, tail_offset={tail_offset}, head_offset={head_offset}, repeated={repeated}"
                );
            });
        }
    }

    #[gpui::test]
    fn test_deferred_horizontal_goal_remeasurement() {
        for (above, select_first) in [(false, true), (true, true), (false, false)] {
            let mut app = shaping_context();
            let middle = format!("ww{}", "x".repeat(34_006));
            let destination = "x".repeat(34_008);
            let text = if above {
                format!("{destination}\n{middle}\n")
            } else {
                format!("\n{middle}\n{destination}")
            };
            let (editor, mut cx) = settled_editor(&text, &mut app);
            editor.update_in(&mut cx, |editor, window, cx| {
                let row = if above { 2 } else { 0 };
                replace_row(editor, row, 0, &shaped_source(16), cx);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    let point = Point::new(row, 34_003);
                    selections.select_ranges([point..point]);
                });
                assert_deferred_row(editor, row, cx);
                if above {
                    editor.select_up(&Default::default(), window, cx);
                    editor.move_up(&Default::default(), window, cx);
                } else {
                    if select_first {
                        editor.select_down(&Default::default(), window, cx);
                    } else {
                        editor.move_down(&MoveDown, window, cx);
                    }
                    editor.move_down(&MoveDown, window, cx);
                }
            });
            cx.background_executor.set_block_on_ticks(0..=50);
            cx.run_until_parked();
            editor.update(&mut cx, |editor, cx| {
                let snapshot = editor.display_snapshot(cx);
                let point = Point::new(
                    if above { 0 } else { 2 },
                    if select_first { 33_998 } else { 33_999 },
                );
                assert_eq!(
                    editor.selections.ranges::<Point>(&snapshot),
                    [point..point],
                    "above={above}, select_first={select_first}"
                );
                assert!(!editor.selections.has_provisional_horizontal_goals());
            });
        }
    }

    #[gpui::test]
    fn test_deferred_horizontal_motion_bias() {
        for (above, inlay) in [(true, true), (false, true), (true, false)] {
            let mut app = shaping_context();
            let destination = "x".repeat(34_008);
            let text = if above {
                format!("{destination}\n")
            } else {
                format!("\n{destination}")
            };
            let (editor, mut cx) = settled_editor(&text, &mut app);
            let source_row = u32::from(above);
            let destination_row = u32::from(!above);
            if inlay {
                editor.update(&mut cx, |editor, cx| {
                    let snapshot = editor.buffer.read(cx).snapshot(cx);
                    let anchor = snapshot.anchor_after(Point::new(destination_row, 33_998));
                    editor.splice_inlays(&[], vec![Inlay::edit_prediction(1, anchor, "zz")], cx);
                });
                cx.background_executor.set_block_on_ticks(0..=50);
                cx.run_until_parked();
                cx.background_executor.set_block_on_ticks(0..=0);
            }
            editor.update_in(&mut cx, |editor, window, cx| {
                replace_row(editor, source_row, 0, &shaped_source(16), cx);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    let point = Point::new(source_row, 34_003);
                    selections.select_ranges([point..point]);
                });
                assert_deferred_row(editor, source_row, cx);
                if above {
                    editor.move_up(&Default::default(), window, cx);
                } else {
                    editor.move_down(&MoveDown, window, cx);
                }
            });
            cx.background_executor.set_block_on_ticks(0..=50);
            cx.run_until_parked();
            editor.update(&mut cx, |editor, cx| {
                let snapshot = editor.display_snapshot(cx);
                let point = Point::new(
                    destination_row,
                    if above && inlay { 33_998 } else { 33_999 },
                );
                assert_eq!(
                    editor.selections.ranges::<Point>(&snapshot),
                    [point..point],
                    "above={above}, inlay={inlay}"
                );
                assert!(!editor.selections.has_provisional_horizontal_goals());
            });
        }
    }

    #[gpui::test]
    fn test_deferred_columnar_row_search() {
        for (above, later_row, follow_up) in [
            (false, false, ColumnarFollowUp::Settle),
            (true, false, ColumnarFollowUp::Settle),
            (false, true, ColumnarFollowUp::Settle),
            (true, true, ColumnarFollowUp::Settle),
            (false, true, ColumnarFollowUp::Repeat),
            (false, false, ColumnarFollowUp::Repeat),
            (false, false, ColumnarFollowUp::Reverse),
            (false, false, ColumnarFollowUp::Cancel),
            (false, false, ColumnarFollowUp::Edit),
            (false, false, ColumnarFollowUp::Move),
        ] {
            let mut app = shaping_context();
            let mut rows = vec![String::new(), "x".repeat(34_002)];
            if later_row {
                rows.push("x".repeat(34_008));
            }
            if above {
                rows.reverse();
            }
            let source_row = if above { rows.len() as u32 - 1 } else { 0 };
            let target_row = if above { source_row - 1 } else { 1 };
            let (editor, mut cx) = settled_editor(&rows.join("\n"), &mut app);
            let mut expected = vec![Point::new(source_row, 34_003)..Point::new(source_row, 34_004)];
            if follow_up == ColumnarFollowUp::Settle || follow_up == ColumnarFollowUp::Repeat {
                expected.push(Point::new(target_row, 33_999)..Point::new(target_row, 34_000));
                if follow_up == ColumnarFollowUp::Repeat && later_row {
                    expected.push(Point::new(2, 33_999)..Point::new(2, 34_000));
                }
            }
            editor.update_in(&mut cx, |editor, window, cx| {
                replace_row(editor, source_row, 0, &shaped_source(16), cx);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_ranges([
                        Point::new(source_row, 34_003)..Point::new(source_row, 34_004)
                    ]);
                });
                assert_deferred_row(editor, source_row, cx);
                add_columnar_selection(editor, above, window, cx);
                match follow_up {
                    ColumnarFollowUp::Settle => {}
                    ColumnarFollowUp::Repeat => add_columnar_selection(editor, above, window, cx),
                    ColumnarFollowUp::Reverse => add_columnar_selection(editor, !above, window, cx),
                    ColumnarFollowUp::Cancel => {
                        editor.change_selections(
                            SelectionEffects::no_scroll(),
                            window,
                            cx,
                            |selections| {
                                assert!(selections.try_cancel());
                            },
                        );
                    }
                    ColumnarFollowUp::Edit => {
                        editor.buffer.update(cx, |buffer, cx| {
                            let point = Point::new(1, 0);
                            buffer.edit([(point..point, "y")], None, cx);
                        });
                    }
                    ColumnarFollowUp::Move => {
                        editor.move_down(&MoveDown, window, cx);
                        expected = vec![
                            Point::new(1, 34_000)..Point::new(1, 34_000),
                            Point::new(1, 34_002)..Point::new(1, 34_002),
                        ];
                    }
                }
            });
            cx.background_executor.set_block_on_ticks(0..=50);
            cx.run_until_parked();
            expected.sort_by_key(|range| range.start);
            editor.update(&mut cx, |editor, cx| {
                let snapshot = editor.display_snapshot(cx);
                assert_eq!(
                    editor.selections.ranges::<Point>(&snapshot),
                    expected,
                    "above={above}, later_row={later_row}, follow_up={follow_up:?}"
                );
                assert!(!editor.selections.has_provisional_horizontal_goals());
            });
        }

        let mut app = shaping_context();
        let plain = "x".repeat(34_008);
        let (editor, mut cx) = settled_editor(&format!("{plain}\n\n{plain}"), &mut app);
        editor.update_in(&mut cx, |editor, window, cx| {
            replace_row(editor, 1, 0, &shaped_source(4), cx);
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                selections.select_ranges([Point::new(0, 34_003)..Point::new(0, 34_004)]);
            });
            assert_deferred_row(editor, 1, cx);
            add_columnar_selection(editor, false, window, cx);
        });
        cx.background_executor.set_block_on_ticks(0..=50);
        cx.run_until_parked();
        editor.update(&mut cx, |editor, cx| {
            let snapshot = editor.display_snapshot(cx);
            assert_eq!(
                editor.selections.ranges::<Point>(&snapshot),
                [0, 2].map(|row| Point::new(row, 34_003)..Point::new(row, 34_004))
            );
            assert!(!editor.selections.has_provisional_horizontal_goals());
        });
    }

    #[gpui::test]
    fn test_deferred_columnar_vertical_motion() {
        for above in [false, true] {
            for follow_up in [
                ColumnarMotionFollowUp::Once,
                ColumnarMotionFollowUp::Twice,
                ColumnarMotionFollowUp::Reverse,
                ColumnarMotionFollowUp::CountedThenReverse,
                ColumnarMotionFollowUp::AddThenMove,
                ColumnarMotionFollowUp::MoveThenAdd,
                ColumnarMotionFollowUp::ExtendThenCollapse,
                ColumnarMotionFollowUp::InlayThenReverse,
            ] {
                for settle_first in [false, true] {
                    let mut app = shaping_context();
                    let mut rows = vec![String::new(), "x".repeat(34_002)];
                    rows.extend((0..3).map(|_| "x".repeat(34_008)));
                    if above {
                        rows.reverse();
                    }
                    let source_row = if above { 4 } else { 0 };
                    let (editor, mut cx) = settled_editor(&rows.join("\n"), &mut app);
                    if follow_up == ColumnarMotionFollowUp::InlayThenReverse {
                        editor.update(&mut cx, |editor, cx| {
                            let snapshot = editor.buffer.read(cx).snapshot(cx);
                            let anchor = snapshot
                                .anchor_after(Point::new(2, if above { 33_998 } else { 33_999 }));
                            editor.splice_inlays(
                                &[],
                                vec![Inlay::edit_prediction(1, anchor, "zz")],
                                cx,
                            );
                        });
                        cx.background_executor.set_block_on_ticks(0..=50);
                        cx.run_until_parked();
                        cx.background_executor.set_block_on_ticks(0..=0);
                    }
                    editor.update_in(&mut cx, |editor, window, cx| {
                        replace_row(editor, source_row, 0, &shaped_source(16), cx);
                        editor.change_selections(
                            SelectionEffects::no_scroll(),
                            window,
                            cx,
                            |selections| {
                                selections
                                    .select_ranges([Point::new(source_row, 34_003)
                                        ..Point::new(source_row, 34_004)]);
                            },
                        );
                        assert_deferred_row(editor, source_row, cx);
                    });
                    if settle_first {
                        cx.background_executor.set_block_on_ticks(0..=50);
                        cx.run_until_parked();
                        cx.background_executor.set_block_on_ticks(0..=0);
                    }
                    editor.update_in(&mut cx, |editor, window, cx| {
                        add_columnar_selection(editor, above, window, cx);
                        match follow_up {
                            ColumnarMotionFollowUp::AddThenMove => {
                                add_columnar_selection(editor, above, window, cx)
                            }
                            ColumnarMotionFollowUp::ExtendThenCollapse => {
                                if above {
                                    editor.select_up(&Default::default(), window, cx);
                                } else {
                                    editor.select_down(&Default::default(), window, cx);
                                }
                            }
                            _ => {}
                        }
                        if follow_up == ColumnarMotionFollowUp::CountedThenReverse {
                            if above {
                                editor.move_up_by_lines(
                                    &crate::actions::MoveUpByLines { lines: 3 },
                                    window,
                                    cx,
                                );
                            } else {
                                editor.move_down_by_lines(
                                    &crate::actions::MoveDownByLines { lines: 3 },
                                    window,
                                    cx,
                                );
                            }
                        } else {
                            move_columnar_selection(editor, above, window, cx);
                        }
                        match follow_up {
                            ColumnarMotionFollowUp::Twice => {
                                move_columnar_selection(editor, above, window, cx)
                            }
                            ColumnarMotionFollowUp::Reverse
                            | ColumnarMotionFollowUp::InlayThenReverse
                            | ColumnarMotionFollowUp::CountedThenReverse => {
                                move_columnar_selection(editor, !above, window, cx)
                            }
                            ColumnarMotionFollowUp::MoveThenAdd => {
                                add_columnar_selection(editor, above, window, cx)
                            }
                            _ => {}
                        }
                    });
                    cx.background_executor.set_block_on_ticks(0..=50);
                    cx.run_until_parked();
                    let (rows, column) = match follow_up {
                        ColumnarMotionFollowUp::Once => {
                            (vec![1, 2], if above { 33_999 } else { 34_000 })
                        }
                        ColumnarMotionFollowUp::Twice => {
                            (vec![2, 3], if above { 33_999 } else { 34_000 })
                        }
                        ColumnarMotionFollowUp::Reverse
                        | ColumnarMotionFollowUp::InlayThenReverse => {
                            (vec![0, 1], if above { 33_999 } else { 34_000 })
                        }
                        ColumnarMotionFollowUp::CountedThenReverse => (vec![2, 3], 33_999),
                        ColumnarMotionFollowUp::AddThenMove
                        | ColumnarMotionFollowUp::MoveThenAdd => {
                            (vec![1, 2, 3], if above { 33_999 } else { 34_000 })
                        }
                        ColumnarMotionFollowUp::ExtendThenCollapse => {
                            (if above { vec![2, 3] } else { vec![3] }, 34_000)
                        }
                    };
                    let mut expected = rows
                        .into_iter()
                        .map(|row| {
                            let column = column + if row == 0 { 4 } else { 0 };
                            let point = Point::new(if above { 4 - row } else { row }, column);
                            point..point
                        })
                        .collect::<Vec<_>>();
                    expected.sort_by_key(|range| range.start);
                    editor.update_in(&mut cx, |editor, window, cx| {
                        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |_| {});
                        let snapshot = editor.display_snapshot(cx);
                        assert_eq!(
                            editor.selections.ranges::<Point>(&snapshot),
                            expected,
                            "above={above}, follow_up={follow_up:?}, settle_first={settle_first}"
                        );
                        assert!(!editor.selections.has_provisional_horizontal_goals());
                        assert!(editor.selections.horizontal_goals.ancestors.is_empty());
                    });
                }
            }
        }
    }

    #[gpui::test]
    fn test_deferred_columnar_motion_without_admissible_row() {
        for (above, reverse_after_add) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let mut app = shaping_context();
            let target = "x".repeat(33_998);
            let text = if above {
                format!("{target}\n")
            } else {
                format!("\n{target}")
            };
            let (editor, mut cx) = settled_editor(&text, &mut app);
            let source_row = u32::from(above);
            editor.update_in(&mut cx, |editor, window, cx| {
                replace_row(editor, source_row, 0, &shaped_source(16), cx);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_ranges([
                        Point::new(source_row, 34_003)..Point::new(source_row, 34_004)
                    ]);
                });
                assert_deferred_row(editor, source_row, cx);
                add_columnar_selection(editor, above, window, cx);
                move_columnar_selection(editor, above, window, cx);
                if reverse_after_add {
                    add_columnar_selection(editor, above, window, cx);
                    move_columnar_selection(editor, !above, window, cx);
                }
                assert_eq!(editor.selections.count(), 1);
            });
            cx.background_executor.set_block_on_ticks(0..=50);
            cx.run_until_parked();
            editor.update_in(&mut cx, |editor, window, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |_| {});
                let point = if reverse_after_add {
                    Point::new(source_row, if above { 34_003 } else { 34_004 })
                } else {
                    Point::new(u32::from(!above), 33_998)
                };
                let snapshot = editor.display_snapshot(cx);
                assert_eq!(editor.selections.ranges::<Point>(&snapshot), [point..point]);
                assert!(!editor.selections.has_provisional_horizontal_goals());
                assert!(editor.selections.horizontal_goals.ancestors.is_empty());
            });
        }
    }

    #[gpui::test]
    fn test_deferred_columnar_group_reversal() {
        for above in [false, true] {
            for settle_first in [false, true] {
                let mut app = shaping_context();
                let mut rows = vec![String::new(), "x".repeat(34_002), "x".repeat(34_008)];
                if above {
                    rows.reverse();
                }
                let source_row = if above { 2 } else { 0 };
                let (editor, mut cx) = settled_editor(&rows.join("\n"), &mut app);
                editor.update_in(&mut cx, |editor, window, cx| {
                    replace_row(editor, source_row, 0, &shaped_source(16), cx);
                    editor.change_selections(
                        SelectionEffects::no_scroll(),
                        window,
                        cx,
                        |selections| {
                            selections.select_ranges([
                                Point::new(source_row, 34_003)..Point::new(source_row, 34_004)
                            ]);
                        },
                    );
                    assert_deferred_row(editor, source_row, cx);
                });
                if settle_first {
                    cx.background_executor.set_block_on_ticks(0..=50);
                    cx.run_until_parked();
                    cx.background_executor.set_block_on_ticks(0..=0);
                }
                editor.update_in(&mut cx, |editor, window, cx| {
                    add_columnar_selection(editor, above, window, cx);
                    add_columnar_selection(editor, above, window, cx);
                });
                cx.background_executor.set_block_on_ticks(0..=50);
                cx.run_until_parked();
                editor.update_in(&mut cx, |editor, window, cx| {
                    add_columnar_selection(editor, !above, window, cx);
                    add_columnar_selection(editor, above, window, cx);
                    add_columnar_selection(editor, !above, window, cx);
                    let snapshot = editor.display_snapshot(cx);
                    let mut expected = vec![
                        Point::new(source_row, 34_003)..Point::new(source_row, 34_004),
                        Point::new(1, 33_999)..Point::new(1, 34_000),
                    ];
                    expected.sort_by_key(|range| range.start);
                    assert_eq!(
                        editor.selections.ranges::<Point>(&snapshot),
                        expected,
                        "above={above}, settle_first={settle_first}"
                    );
                    let group = &editor
                        .add_selections_state
                        .as_ref()
                        .expect("columnar group")
                        .groups[0];
                    let mut ids = editor
                        .selections
                        .disjoint_anchors()
                        .iter()
                        .map(|selection| selection.id)
                        .collect::<Vec<_>>();
                    ids.sort_unstable();
                    let mut stack = group.stack.clone();
                    stack.sort_unstable();
                    assert_eq!(stack, ids);
                    assert!(!editor.selections.has_provisional_horizontal_goals());
                });
            }
        }
    }

    #[gpui::test]
    fn test_deferred_vertical_motion_batch_merges() {
        for (above, reversed, settle_first) in [
            (false, false, false),
            (false, false, true),
            (false, true, false),
            (true, false, false),
        ] {
            let mut app = shaping_context();
            let plain = "x".repeat(34_008);
            let mut rows = vec![String::new(), plain.clone(), plain.clone(), plain.clone()];
            if above {
                rows.reverse();
            }
            let source_row = if above { 3 } else { 0 };
            let (editor, mut cx) = settled_editor(&rows.join("\n"), &mut app);
            let mut oldest_id = None;
            editor.update_in(&mut cx, |editor, window, cx| {
                replace_row(editor, source_row, 0, &plain, cx);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    let start = Point::new(source_row, 34_003);
                    let end = Point::new(source_row, 34_004);
                    selections.select_ranges([if reversed { end..start } else { start..end }]);
                    oldest_id = Some(selections.oldest_anchor().id);
                });
                assert_deferred_row(editor, source_row, cx);
            });
            if settle_first {
                cx.background_executor.set_block_on_ticks(0..=50);
                cx.run_until_parked();
                cx.background_executor.set_block_on_ticks(0..=0);
            }
            editor.update_in(&mut cx, |editor, window, cx| {
                add_columnar_selection(editor, above, window, cx);
                if above {
                    editor.select_up(&crate::actions::SelectUp, window, cx);
                } else {
                    editor.select_down(&crate::actions::SelectDown, window, cx);
                }
                if settle_first && !above && !reversed {
                    let snapshot = editor.display_snapshot(cx);
                    assert_eq!(
                        editor.selections.ranges::<Point>(&snapshot),
                        [Point::new(0, 34_003)..Point::new(2, 34_004)]
                    );
                    assert_eq!(Some(editor.selections.oldest_anchor().id), oldest_id);
                }
                move_columnar_selection(editor, above, window, cx);
            });
            cx.background_executor.set_block_on_ticks(0..=50);
            cx.run_until_parked();
            editor.update_in(&mut cx, |editor, window, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |_| {});
                let rows = if above {
                    vec![0, 1]
                } else if reversed {
                    vec![2, 3]
                } else {
                    vec![3]
                };
                let expected = rows
                    .into_iter()
                    .map(|row| {
                        let point = Point::new(row, 34_004);
                        point..point
                    })
                    .collect::<Vec<_>>();
                let snapshot = editor.display_snapshot(cx);
                assert_eq!(
                    editor.selections.ranges::<Point>(&snapshot),
                    expected,
                    "above={above}, reversed={reversed}, settle_first={settle_first}"
                );
                assert_eq!(Some(editor.selections.oldest_anchor().id), oldest_id);
                assert!(!editor.selections.has_provisional_horizontal_goals());
                assert!(editor.selections.horizontal_goals.ancestors.is_empty());
            });
        }
    }

    #[gpui::test]
    fn test_horizontal_motion_batch_coalescence() {
        let mut app = shaping_context();
        let (editor, mut cx) = settled_editor("abcdefghij", &mut app);
        editor.update_in(&mut cx, |editor, window, cx| {
            let snapshot = editor.display_snapshot(cx);
            let inputs = [
                (8, 1..3, false),
                (2, 2..5, false),
                (1, 5..7, false),
                (0, 7..7, true),
                (6, 1..2, true),
                (3, 0..8, false),
            ]
            .map(|(id, range, reversed)| Selection {
                id,
                start: Point::new(0, range.start),
                end: Point::new(0, range.end),
                reversed,
                goal: SelectionGoal::HorizontalPosition(id as f64),
            });
            let batch = MotionBatch {
                sources: (0..inputs.len()).collect(),
            };
            for reveal in [false, true] {
                let resolved = inputs
                    .iter()
                    .enumerate()
                    .map(|(index, selection)| {
                        let pending = index == 5 && !reveal;
                        (
                            IntentSource::Ancestor(index),
                            Some((*selection, pending, !pending)),
                        )
                    })
                    .collect::<HashMap<_, _>>();
                let batch = batch.resolve(&resolved, &snapshot);
                let mut actual = batch.selections.into_values().collect::<Vec<_>>();
                super::super::sort_selections(&mut actual);
                let expected = if reveal {
                    vec![Selection {
                        id: 3,
                        start: DisplayPoint::new(DisplayRow(0), 0),
                        end: DisplayPoint::new(DisplayRow(0), 8),
                        reversed: false,
                        goal: SelectionGoal::HorizontalPosition(3.),
                    }]
                } else {
                    vec![
                        Selection {
                            id: 6,
                            start: DisplayPoint::new(DisplayRow(0), 1),
                            end: DisplayPoint::new(DisplayRow(0), 5),
                            reversed: true,
                            goal: SelectionGoal::HorizontalPosition(6.),
                        },
                        Selection {
                            id: 1,
                            start: DisplayPoint::new(DisplayRow(0), 5),
                            end: DisplayPoint::new(DisplayRow(0), 7),
                            reversed: false,
                            goal: SelectionGoal::HorizontalPosition(1.),
                        },
                    ]
                };
                assert_eq!(actual, expected);
                assert_eq!(batch.unsettled, !reveal);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select(
                        inputs
                            .iter()
                            .take(if reveal { 6 } else { 5 })
                            .copied()
                            .collect(),
                    );
                });
                assert_eq!(editor.selections.all_display(&snapshot), expected);
            }
        });
    }

    #[gpui::test]
    fn test_counted_horizontal_motion_recording() {
        let mut app = shaping_context();
        let (editor, mut cx) = settled_editor("abcdef\nx\nabcdef", &mut app);
        editor.update_in(&mut cx, |editor, window, cx| {
            let snapshot = editor.display_snapshot(cx);
            let details = editor.text_layout_details(window, cx).detached();
            for (bias, count) in [
                (Bias::Left, 0),
                (Bias::Left, 3),
                (Bias::Right, 1),
                (Bias::Right, 1_000_000),
            ] {
                for id in 0..2 {
                    details
                        .horizontal_origins
                        .0
                        .lock()
                        .motion_sources
                        .insert(id, id);
                    details.horizontal_origins.set_selection(Some(id));
                    let point = DisplayPoint::new(
                        DisplayRow(if bias == Bias::Left { 0 } else { 2 }),
                        id as u32 + 1,
                    );
                    let goal = SelectionGoal::HorizontalPosition(
                        snapshot.x_for_display_point(point, &details),
                    );
                    assert_eq!(
                        HorizontalMotion::DisplaySteps { count, bias }
                            .apply(&snapshot, point, goal, &details),
                        (point, goal)
                    );
                }
                details.horizontal_origins.set_selection(None);
                let recorded = details.horizontal_origins.take();
                assert_eq!(recorded.motions.len(), 2);
                for motion in recorded.motions.values() {
                    assert_eq!(motion.steps.len(), 1);
                    assert!(!motion.steps.spilled());
                    let HorizontalMotion::DisplaySteps {
                        count: recorded_count,
                        bias: recorded_bias,
                    } = &motion.steps[0]
                    else {
                        panic!("counted display motion");
                    };
                    assert_eq!((*recorded_count, *recorded_bias), (count, bias));
                }
            }
            replace_row(editor, 2, 6, &"x".repeat(34_008), cx);
            let snapshot = editor.display_snapshot(cx);
            let details = editor.text_layout_details(window, cx).detached();
            for id in 0..2 {
                details
                    .horizontal_origins
                    .0
                    .lock()
                    .motion_sources
                    .insert(id, id);
                details.horizontal_origins.set_selection(Some(id));
                let point = DisplayPoint::new(DisplayRow(2), 34_003 + id as u32);
                assert!(
                    !snapshot
                        .long_row(point.row())
                        .expect("long row")
                        .is_settled_through(point.column())
                );
                assert_eq!(
                    HorizontalMotion::DisplaySteps {
                        count: 3,
                        bias: Bias::Right,
                    }
                    .apply(&snapshot, point, SelectionGoal::None, &details),
                    (point, SelectionGoal::None)
                );
            }
            let recorded = details.horizontal_origins.take();
            assert_eq!(recorded.motions.len(), 2);
            assert_eq!(recorded.selection_origins.len(), 2);
            for origins in recorded.selection_origins.values() {
                assert_eq!(origins.len(), 1);
            }
        });
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ColumnarMotionFollowUp {
        Once,
        Twice,
        Reverse,
        CountedThenReverse,
        AddThenMove,
        MoveThenAdd,
        ExtendThenCollapse,
        InlayThenReverse,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ColumnarFollowUp {
        Settle,
        Repeat,
        Reverse,
        Cancel,
        Edit,
        Move,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum CollisionFollowUp {
        Settle,
        MoveAgain,
        Columnar,
        VerticalToColumnar { wide_len: usize, settled: bool },
        Cancel,
        Replace,
        Edit,
        Pending,
        Abandon,
        Commit,
        Eof,
    }

    fn shaping_context() -> TestAppContext {
        let mut cx = TestAppContext::build_with_text_system(
            gpui::TestDispatcher::new(0),
            None,
            Arc::new(ShapingTextSystem(gpui::NoopTextSystem)),
        );
        init_test(&mut cx, |_| {});
        cx
    }

    fn settled_editor(text: &str, cx: &mut TestAppContext) -> (Entity<Editor>, VisualTestContext) {
        let window = cx.add_window(|window, cx| {
            let buffer = MultiBuffer::build_simple(text, cx);
            let mut editor = Editor::new(EditorMode::full(), buffer, None, window, cx);
            editor.set_soft_wrap_mode(SoftWrap::None, cx);
            editor
        });
        let mut cx = VisualTestContext::from_window(*window, cx);
        let editor = window.root(&mut cx).expect("editor");
        cx.run_until_parked();
        cx.background_executor.set_block_on_ticks(0..=0);
        (editor, cx)
    }

    fn assert_deferred_row(editor: &mut Editor, row: u32, cx: &mut Context<Editor>) {
        assert!(
            !editor
                .display_snapshot(cx)
                .long_row(DisplayRow(row))
                .expect("deferred row")
                .is_settled_through(34_003)
        );
    }

    fn shaped_source(wide_len: usize) -> String {
        format!(
            "{}{}{}",
            "x".repeat(1_000),
            "w".repeat(wide_len),
            "x".repeat(33_000)
        )
    }

    fn replace_row(
        editor: &mut Editor,
        row: u32,
        old_len: u32,
        text: &str,
        cx: &mut Context<Editor>,
    ) {
        editor.buffer.update(cx, |buffer, cx| {
            buffer.edit(
                [(Point::new(row, 0)..Point::new(row, old_len), text)],
                None,
                cx,
            );
        });
    }

    fn add_columnar_selection(
        editor: &mut Editor,
        above: bool,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if above {
            editor.add_selection_above(
                &AddSelectionAbove {
                    skip_soft_wrap: false,
                },
                window,
                cx,
            );
        } else {
            editor.add_selection_below(
                &AddSelectionBelow {
                    skip_soft_wrap: false,
                },
                window,
                cx,
            );
        }
    }

    fn move_columnar_selection(
        editor: &mut Editor,
        above: bool,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if above {
            editor.move_up(&Default::default(), window, cx);
        } else {
            editor.move_down(&MoveDown, window, cx);
        }
    }

    fn move_collision_ranges_down(
        editor: &mut Editor,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let details = editor.text_layout_details(window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.move_with_horizontal_goals(&mut |map, selection| {
                let (start, end) = match selection.goal {
                    SelectionGoal::HorizontalRange { start, end } => (start, end),
                    _ => (
                        map.x_for_display_point(selection.start, &details),
                        map.x_for_display_point(selection.end, &details),
                    ),
                };
                let row = selection.start.row() + 1;
                selection.start = map.clip_point(
                    DisplayPoint::new(row, map.display_column_for_x(row, start, &details)),
                    Bias::Left,
                );
                selection.end = map.clip_point(
                    DisplayPoint::new(row, map.display_column_for_x(row, end, &details)),
                    Bias::Right,
                );
                selection.goal = SelectionGoal::HorizontalRange { start, end };
            });
        });
    }
}
