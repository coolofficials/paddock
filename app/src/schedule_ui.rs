use super::*;
use crate::race_catalog::{Category, License, Licenses};
use std::collections::{BTreeMap, BTreeSet};

fn card_height() -> i32 { design::row_height()*2+8 }
fn card_pitch() -> i32 { card_height()+8 }

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Hash)]
pub(super) enum Filter { #[default] All, SportsCar, Formula, Oval, DirtOval, DirtRoad, Unranked }
impl Filter {
    const ALL: [Self; 7] = [Self::All, Self::SportsCar, Self::Formula, Self::Oval, Self::DirtOval, Self::DirtRoad, Self::Unranked];
    pub(super) fn category(self) -> Option<Category> { match self { Self::All => None, Self::SportsCar => Some(Category::SportsCar), Self::Formula => Some(Category::Formula), Self::Oval => Some(Category::Oval), Self::DirtOval => Some(Category::DirtOval), Self::DirtRoad => Some(Category::DirtRoad), Self::Unranked => Some(Category::Unranked) } }
    pub(super) fn label(self) -> &'static str { self.category().map(Category::label).unwrap_or("All disciplines") }
}

pub(super) struct View { pub weekly: bool, pub week: Option<u8>, pub filter: Filter, pub favorites_only: bool, pub conditional: bool, pub horizontal: i32, pub horizontal_max: i32, pub column_width: i32, pub body_top: i32, pub bar: Option<(RECT, RECT)>, pub drag: Option<i32>, pub columns: usize, pub week_heights: Vec<i32>, pub planner_height: i32, pub planner_card_scroll: BTreeMap<String, i32>, pub planner_card_regions: Vec<(RECT, String, i32)>, measure_signature: u64, heights: std::collections::HashMap<usize, i32> }
impl Default for View { fn default() -> Self { Self { weekly: false, week: None, filter: Filter::All, favorites_only: true, conditional: false, horizontal: 0, horizontal_max: 0, column_width: 240, body_top: 244, bar: None, drag: None, columns: 3, week_heights: vec![112; 12], planner_height: 140, planner_card_scroll: BTreeMap::new(), planner_card_regions: vec![], measure_signature: 0, heights: Default::default() } } }
pub(super) fn favorite_ids(store: &Store, schedule: &Schedule) -> BTreeSet<String> {
    store.config.favorite_series.clone().unwrap_or_else(|| schedule.weeks.iter().flat_map(|week| store.plans.week(&schedule.key(week.number))).filter(|plan| plan.season == schedule.season).filter_map(|plan| schedule.plan_index(plan).and_then(|index| schedule.catalog.get(index)).map(|series| series.id.clone())).collect())
}
impl View {
    pub(super) fn row_height(&self, index: usize) -> i32 { self.heights.get(&index).copied().unwrap_or(card_pitch()) }
    pub(super) fn invalidate_measurement(&mut self) { self.measure_signature = 0; }
    pub fn content_height(&self, schedule: &Schedule, store: &Store) -> i32 {
        if self.weekly { return self.planner_height; }
        if self.favorites_only { if self.indices(schedule, store).is_empty() { 160 } else { self.week_heights.iter().sum() } }
        else { self.indices(schedule, store).chunks(self.columns).map(|indices| indices.iter().map(|index| self.row_height(*index)).max().unwrap_or(0)).sum() }
    }
    pub(super) fn indices(&self, schedule: &Schedule, store: &Store) -> Vec<usize> { self.visible_groups(schedule, store).into_iter().flat_map(|(_, indices)| indices).collect() }
    pub(super) fn week_offset(&self, number: u8) -> i32 { self.week_heights.iter().take(number.saturating_sub(1) as usize).sum() }
    pub(super) fn week_number(&self, schedule: &Schedule) -> u8 {
        self.week.filter(|number| schedule.weeks.iter().any(|week| week.number == *number)).or_else(|| schedule.active_week()).unwrap_or(1)
    }
    pub(super) fn groups(&self, schedule: &Schedule, licenses: &Licenses) -> Vec<(Category, Vec<usize>)> {
        Category::ALL.into_iter().filter(|category| self.filter.category().map(|filter| filter == *category).unwrap_or(true)).filter_map(|category| {
            let mut indices: Vec<_> = schedule.catalog.iter().enumerate().filter(|(_, series)| series.category == category && series.accepts_class(licenses.get(&category).copied()) && (self.conditional || licenses.get(&category).map(|license| series.class <= *license).unwrap_or(true)) && schedule.weeks.iter().any(|week| series.entry(&week.start).is_some())).map(|(index, _)| index).collect();
            indices.sort_by_key(|index| (schedule.catalog[*index].class, *index));
            (!indices.is_empty()).then_some((category, indices))
        }).collect()
    }
    pub(super) fn visible_groups(&self, schedule: &Schedule, store: &Store) -> Vec<(Category, Vec<usize>)> {
        let favorites = favorite_ids(store, schedule);
        if self.favorites_only {
            Category::ALL.into_iter().filter_map(|category| {
                let indices: Vec<_> = schedule.catalog.iter().enumerate().filter(|(_, series)| series.category == category && favorites.contains(&series.id) && schedule.weeks.iter().any(|week| series.entry(&week.start).is_some())).map(|(index, _)| index).collect();
                (!indices.is_empty()).then_some((category, indices))
            }).collect()
        } else { self.groups(schedule, &store.config.licenses) }
    }

    pub(super) fn scroll_to(&self, schedule: &Schedule, licenses: &Licenses, index: usize) -> i32 {
        let indices: Vec<_> = self.groups(schedule, licenses).into_iter().flat_map(|(_, indices)| indices).collect();
        let position = indices.iter().position(|candidate| *candidate == index).unwrap_or(0);
        indices.chunks(self.columns).take(position/self.columns).map(|row| row.iter().map(|index| self.row_height(*index)).max().unwrap_or(0)).sum()
    }
}

#[cfg(test)]
pub(super) fn saved_week_label(plan: Option<&Plan>) -> &str {
    let Some(plan) = plan else { return "Not planned"; };
    let track = plan.track.trim();
    if track.is_empty() { return "No saved track"; }
    track
}

#[cfg(test)]
pub(super) fn saved_series_label<'a>(schedule: &'a Schedule, plan: Option<&'a Plan>) -> &'a str {
    const SHORT: [&str; 8] = ["MX-5", "GR86", "F4 · Fixed", "Porsche · Fixed", "Porsche · Open", "F4 · America", "F4 · Asia Pacific", "F4 · Europe"];
    let Some(plan) = plan else { return ""; };
    if let Some(index) = schedule.plan_index(plan) {
        if let Some(label) = SHORT.get(index) { return label; }
        if plan.series_name.is_empty() {
            if let Some(series) = schedule.catalog.get(index) { return &series.name; }
        }
    }
    if plan.series_name.is_empty() { "Saved series" } else { &plan.series_name }
}
pub(super) fn series_title(name: &str) -> &str {
    let short = name.split(" by ").next().unwrap_or(name);
    match short {
        "Global Mazda MX-5 Cup" => "Mazda MX-5 Cup",
        "Toyota GR86 Cup" => "GR86 Cup",
        "FIA Formula 4 Challenge - Fixed" => "F4 Challenge · Fixed",
        "iRacing Porsche Cup - Fixed" => "Porsche Cup · Fixed",
        "iRacing Porsche Cup" => "Porsche Cup · Open",
        "FIA F4 Esports Regional Tour - America" => "F4 · America",
        "FIA F4 Esports Regional Tour - Asia Pacific" => "F4 · Asia Pacific",
        "FIA F4 Esports Regional Tour - Europe" => "F4 · Europe",
        _ => short,
    }
}

pub(super) fn catalog_label(series: &crate::race_catalog::Series) -> String {
    let vehicle = if series.vehicles.contains(',') { "Multiple cars · see details" } else if series.vehicles.contains("See race week") { "Cars vary by week · see details" } else { &series.vehicles };
    format!("{}\n{}\n{} · License {}", series_title(&series.name), vehicle, series.category.label(), series.class.label())
}

impl State {
    pub(super) fn schedule_horizontal(&mut self, delta: i32) {
        self.schedule_view.horizontal = (self.schedule_view.horizontal+delta).clamp(0, self.schedule_view.horizontal_max);
        self.input.pointer = None; self.input.hovered = None; self.input.pressed = None;
    }
    pub(super) fn drag_schedule_horizontal(&mut self, horizontal: i32, grab: i32) {
        if let Some((track, thumb)) = self.schedule_view.bar {
            self.schedule_view.horizontal = ((horizontal-track.left-grab)*self.schedule_view.horizontal_max/(track.right-track.left-(thumb.right-thumb.left)).max(1)).clamp(0, self.schedule_view.horizontal_max);
        }
    }
    pub(super) unsafe fn schedule_menu(&self, window: HWND, action: &Action, options: &[String], selected: Option<usize>) -> Option<usize> {
        if window.is_null() { return None; }
        let menu = CreatePopupMenu();
        for (index, label) in options.iter().enumerate() { AppendMenuW(menu, MF_STRING | if selected == Some(index) { MF_CHECKED } else { 0 }, index+1, wide(label).as_ptr()); }
        let mut point: POINT = std::mem::zeroed(); GetCursorPos(&mut point);
        if let Some(hit) = self.hits.iter().find(|hit| &hit.action == action) {
            let dpi = design::effective_dpi(GetDpiForWindow(window));
            point = POINT { x: (hit.rect.left-self.input.viewport.x)*dpi/96, y: (hit.rect.bottom-self.input.viewport.y)*dpi/96 }; ClientToScreen(window, &mut point);
        }
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, point.x, point.y, 0, window, null());
        DestroyMenu(menu); PostMessageW(window, WM_NULL, 0, 0);
        (chosen > 0).then(|| chosen as usize-1)
    }
    pub(super) unsafe fn schedule_action(&mut self, window: HWND, action: Action) {
        match action {
            Action::ConditionalSeries => { self.schedule_view.conditional = !self.schedule_view.conditional; self.scroll = 0; },
            Action::WeekMenu => {
                if let Some(schedule) = &self.schedule {
                    let options: Vec<_> = schedule.weeks.iter().map(|week| format!("Week {} · {} · {} planned", week.number, week.start, self.store.plans.week(&schedule.key(week.number)).len())).collect();
                    if let Some(index) = self.schedule_menu(window, &Action::WeekMenu, &options, None) { let number = schedule.weeks[index].number; self.action(window, Action::Week(number)); }
                }
            },
            Action::SeriesJump => {
                if let Some(schedule) = &self.schedule {
                    let indices = self.schedule_view.indices(schedule, &self.store);
                    let options: Vec<_> = indices.iter().map(|index| schedule.catalog[*index].name.clone()).collect();
                    if let Some(position) = self.schedule_menu(window, &Action::SeriesJump, &options, None) {
                        self.schedule_view.horizontal = (position as i32*self.schedule_view.column_width).min(self.schedule_view.horizontal_max);
                    }
                }
            },
            Action::FavoritesOnly(value) => { self.schedule_view.weekly = false; self.switch_schedule_filter(self.schedule_view.filter, value); self.planner_action(window, Action::PlannerMode(if value { crate::planner::Mode::Season } else { crate::planner::Mode::Catalog })); },
            Action::Favorite(index) => {
                if let Some(schedule) = &self.schedule {
                    if let Some(series) = schedule.catalog.get(index) {
                        let previous = self.store.config.favorite_series.clone();
                        let mut favorites = favorite_ids(&self.store, schedule);
                        if !favorites.remove(&series.id) { favorites.insert(series.id.clone()); }
                        self.store.config.favorite_series = Some(favorites);
                        let result = self.store.save_config();
                        if result.is_err() { self.store.config.favorite_series = previous; }
                        self.report(result, "Series saved · Planned races kept");
                    }
                }
            },
            Action::CategoryMenu => {
                let options: Vec<_> = Filter::ALL.iter().map(|filter| filter.label().to_string()).collect();
                if let Some(index) = self.schedule_menu(window, &Action::CategoryMenu, &options, Filter::ALL.iter().position(|filter| *filter == self.schedule_view.filter)) { self.action(window, Action::Filter(Filter::ALL[index])); }
            }
            Action::LicenseMenu => {
                let category = if let Some(category) = self.schedule_view.filter.category().filter(|category| *category != Category::Unranked) { category } else {
                    let categories: Vec<_> = Category::ALL.into_iter().filter(|category| *category != Category::Unranked).collect();
                    let labels: Vec<_> = categories.iter().map(|category| format!("{}: {}", category.label(), self.store.config.licenses.get(category).map(|license| license.label()).unwrap_or("Not set"))).collect();
                    let Some(choice) = self.schedule_menu(window, &Action::LicenseMenu, &labels, None) else { return; }; categories[choice].clone()
                };
                let options: Vec<_> = std::iter::once("Not set · All licenses".into()).chain(License::ALL.iter().map(|license| license.label().to_string())).collect();
                let selected = self.store.config.licenses.get(&category).and_then(|license| License::ALL.iter().position(|candidate| candidate == license)).map(|index| index+1).unwrap_or(0);
                if let Some(index) = self.schedule_menu(window, &Action::LicenseMenu, &options, Some(selected)) { self.action(window, Action::License(category, index.checked_sub(1).map(|index| License::ALL[index]))); }
            }
            Action::License(category, license) => {
                let previous = self.store.config.licenses.clone();
                if let Some(license) = license { self.store.config.licenses.insert(category, license); } else { self.store.config.licenses.remove(&category); }
                let result = self.store.save_config();
                if result.is_err() { self.store.config.licenses = previous; }
                self.schedule_view.favorites_only = false; self.scroll = 0; self.report(result, "Licenses saved · All series shown");
            }
            Action::SeriesInfo(index) => {
                if window.is_null() { return; }
                if let Some(schedule) = &self.schedule {
                    if let Some(series) = schedule.catalog.get(index) {
                        let week = &schedule.weeks[self.schedule_view.week_number(schedule) as usize-1];
                        if let Some(entry) = series.entry(&week.start).or_else(|| schedule.weeks.iter().find_map(|week| series.entry(&week.start))) {
                            let body = format!("{}\n{} · License {}\n\nCars\n{}\n\nStarts {}\n{}\n\nEligibility: {} {:.1} – {} {:.1}\n{}\n\n{}\n\nCheck safety rating, content ownership and team/event requirements in iRacing.", series.name, series.category.label(), series.class.label(), entry.vehicles, entry.start, entry.track, series.minimum.label(), series.minimum_sr, series.maximum.label(), series.maximum_sr, series.conditions, series.times);
                            dialogs::details(window, "Series details", &body);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) unsafe fn measure_schedule_rows(&mut self, dc: HDC, layout: &Layout) {
        if self.schedule_view.weekly { self.measure_planner(dc, layout); return; }
        let Some(schedule) = &self.schedule else { return; };
        self.schedule_view.columns = if layout.right-layout.left >= 1040 { 4 } else { 3 };
        let indices = self.schedule_view.indices(schedule, &self.store);
        let columns = self.schedule_view.columns;
        self.schedule_view.column_width = ((layout.right-layout.left-132)/columns as i32).max(228);
        self.schedule_view.horizontal_max = (indices.len() as i32*self.schedule_view.column_width-(layout.right-layout.left-132)).max(0);
        self.schedule_view.horizontal = self.schedule_view.horizontal.clamp(0, self.schedule_view.horizontal_max);
        let width = self.schedule_view.column_width-32;
        use std::hash::{Hash, Hasher};
        let mut signature = std::collections::hash_map::DefaultHasher::new();
        let mut extent: SIZE = std::mem::zeroed(); GetViewportExtEx(dc, &mut extent);
        (layout.right-layout.left, self.schedule_view.favorites_only, design::row_height(), extent.cx, extent.cy).hash(&mut signature);
        for index in &indices { let series = &schedule.catalog[*index]; (index, &series.id, &series.name, &series.vehicles).hash(&mut signature); }
        if self.schedule_view.favorites_only { for week in &schedule.weeks { for index in &indices { schedule.track(week.number, *index).hash(&mut signature); } } }
        let signature = signature.finish();
        if self.schedule_view.measure_signature != signature {
        let font = ui_font(FONT_BODY, true); let previous = SelectObject(dc, font);
        if self.schedule_view.favorites_only { self.schedule_view.week_heights = schedule.weeks.iter().map(|week| {
            let lines = indices.iter().filter_map(|index| schedule.track(week.number, *index)).map(|track| wrapped_lines(dc, track, width).len()).max().unwrap_or(1);
            (lines as i32*27+68).max(112)
        }).collect(); }
        if !self.schedule_view.favorites_only { for index in &indices {
            let series = &schedule.catalog[*index];
            let label = catalog_label(series);
            let lines = wrapped_lines(dc, &label, (layout.right-layout.left)/columns as i32-40).len();
            self.schedule_view.heights.insert(*index, lines as i32*27+192);
        }
        }
        self.schedule_view.body_top = if self.schedule_view.favorites_only {
            let lines = indices.iter().map(|index| wrapped_lines(dc, series_title(&schedule.catalog[*index].name), width).len()).max().unwrap_or(1) as i32;
            168+(lines*27+24).max(56)+20
        } else { 340 };
        SelectObject(dc, previous); DeleteObject(font);
        self.schedule_view.measure_signature = signature;
        }
        if !self.schedule_view.favorites_only {
            if let Some(Action::SeriesInfo(target)) = self.input.locate_after_draw.take() {
                if let Some(position) = indices.iter().position(|index| *index == target) { self.scroll = indices.chunks(columns).take(position/columns).map(|row| row.iter().map(|index| self.schedule_view.row_height(*index)).max().unwrap_or(0)).sum(); }
            }
        }
        if self.schedule_view.favorites_only {
            if let Some(Action::SavedInfo(number)) = self.input.locate_after_draw.take() { self.scroll = self.schedule_view.week_offset(number); }
        }

    }
    pub(super) fn license_editable(&self) -> bool { true }
    pub(super) unsafe fn draw_schedule(&mut self, dc: HDC, layout: &Layout) {
        if self.schedule_view.weekly { self.draw_planner(dc, layout); return; }
        let Some(schedule) = self.schedule.clone() else { text(dc, "Refresh the schedule to continue.", rect(layout.left, 132, layout.right, 220), FONT_BODY, TEXT, false, true); return; };
        let left = layout.left; let right = layout.right; let board = self.schedule_view.favorites_only;
        self.schedule_view.bar = None;
        let indices = self.schedule_view.indices(&schedule, &self.store);
        let columns = self.schedule_view.columns;
        let favorites = favorite_ids(&self.store, &schedule);
        self.button(dc, "This week", rect(left, 124, left+100, 160), Action::PlannerMode(crate::planner::Mode::Week), false);
        self.button(dc, "Season", rect(left+108, 124, left+208, 160), Action::FavoritesOnly(true), board);
        self.button(dc, "Series library", rect(left+216, 124, left+336, 160), Action::FavoritesOnly(false), !board);
        if board {
            self.button(dc, "Today", rect(left+348, 124, left+428, 160), Action::Week(schedule.active_week().unwrap_or(1)), false);
            self.button(dc, "Choose week", rect(left+436, 124, left+552, 160), Action::WeekMenu, false);
            self.button(dc, "Find a series", rect(left+560, 124, left+704, 160), Action::SeriesJump, false);
            let header_bottom = self.schedule_view.body_top-20;
            text(dc, "WEEK / PLAN", rect(left, 174, left+124, 204), FONT_BODY, MUTED, true, false);
            for (column, index) in indices.iter().enumerate() {
                let cell = self.season_cell(layout, column, 168, header_bottom-168);
                if cell.right <= left+132 || cell.left >= right { continue; }
                let clipped = SaveDC(dc); IntersectClipRect(dc, left+132, 168, right, header_bottom);
                let series = &schedule.catalog[*index];
                self.surface(dc, cell, &Action::SeriesInfo(*index), false);
                text_format(dc, series_title(&series.name), rect(cell.left+8, 172, cell.right-8, header_bottom-24), FONT_BODY, TEXT, true, DT_WORDBREAK);
                text(dc, &format!("{} · License {}", series.category.label(), series.class.label()), rect(cell.left+8, header_bottom-24, cell.right-8, header_bottom), FONT_CAPTION, MUTED, false, false);
                self.hits.push(Hit { rect: rect(cell.left.max(left+132), cell.top, cell.right.min(right), cell.bottom), action: Action::SeriesInfo(*index), label: series.name.clone() });
                RestoreDC(dc, clipped);
            }
        } else {
            self.button(dc, self.schedule_view.filter.label(), rect(left, 172, left+152, 208), Action::CategoryMenu, false);
            self.button(dc, "My license", rect(left+160, 172, left+312, 208), Action::LicenseMenu, false);
            self.button(dc, if self.schedule_view.conditional { "Conditional eligibility: on" } else { "Conditional eligibility: off" }, rect(left+320, 172, left+532, 208), Action::ConditionalSeries, self.schedule_view.conditional);
            text(dc, &format!("{} series", indices.len()), rect(left+544, 172, right, 208), FONT_CAPTION, MUTED, false, false);
            text_format(dc, "Your license and conditional eligibility are shown separately. Promotion requires SR 4.0 + MPR for D–B, or SR 3.0 + MPR for Rookie.\nUpdate your license after promotion. Content ownership and special requirements are not checked.", rect(left, 220, right, 304), FONT_BODY, MUTED, false, DT_WORDBREAK);
        }
        if board && self.schedule_view.horizontal_max > 0 {
            let track = rect(left+132, self.schedule_view.body_top-20, right, self.schedule_view.body_top-8);
            let available = track.right-track.left; let extent = available+self.schedule_view.horizontal_max;
            let thumb_width = (available*available/extent).max(32);
            let position = self.schedule_view.horizontal*(available-thumb_width)/self.schedule_view.horizontal_max;
            let thumb = rect(track.left+position, track.top, track.left+position+thumb_width, track.bottom);
            fill(dc, track, LINE); fill(dc, thumb, MUTED); self.schedule_view.bar = Some((track, thumb));
        }
        let top = self.schedule_view.body_top; let clip = SaveDC(dc); IntersectClipRect(dc, left, top, right, layout.bottom); let start_hits = self.hits.len();
        if indices.is_empty() {
            text(dc, if board { "Choose series, then plan your race weeks." } else { "No matching series. Change the discipline or license." }, rect(left+8, top+8, right-8, top+76), FONT_BODY, MUTED, false, true);
            if board { self.button(dc, "Choose series", rect(left+8, top+84, left+208, top+124), Action::FavoritesOnly(false), false); }
        } else if board {
            for week in &schedule.weeks {
                let row_top = top+self.schedule_view.week_offset(week.number)-self.scroll;
                let height = self.schedule_view.week_heights[week.number as usize-1]-8;
                if row_top+height < top || row_top >= layout.bottom { continue; }
                let key = schedule.key(week.number); let count = self.store.plans.week(&key).len();
                fill(dc, rect(left, row_top, right, row_top+height), if schedule.active_week() == Some(week.number) { SIDE } else { BG });
                let label = format!("Week {} · {}\n{}", week.number, &week.start[5..], if schedule.active_week() == Some(week.number) { format!("This week · {count} planned") } else { format!("{count} planned") });
                let area = rect(left, row_top, left+124, row_top+56);
                let (foreground, _) = self.surface(dc, area, &Action::SavedInfo(week.number), count > 0);
                text_format(dc, &label, rect(left+8, row_top+8, left+116, row_top+56), FONT_CAPTION, foreground, false, DT_WORDBREAK);
                self.hits.push(Hit { rect: area, action: Action::SavedInfo(week.number), label });

                if count > 0 { self.button(dc, "Clear", rect(left, row_top+64, left+80, row_top+100), Action::Remove(key.clone()), false); if let Some(hit) = self.hits.last_mut() { hit.label = format!("Week {} · Clear selections", week.number); } }
                if self.store.plans.can_undo(&key) {
                    let area = rect(left+88, row_top+64, left+124, row_top+100); let action = Action::UndoPlan(key.clone());
                    let (foreground, _) = self.surface(dc, area, &action, false); crate::icons::draw(dc, "undo-2", left+96, row_top+72, 20, foreground);
                    self.hits.push(Hit { rect: area, action, label: format!("Undo week {}", week.number) });
                }
                for (column, index) in indices.iter().enumerate() {
                    let cell = self.season_cell(layout, column, row_top, height);
                    if cell.right <= left+132 || cell.left >= right { continue; }
                    let clipped = SaveDC(dc); IntersectClipRect(dc, left+132, row_top, right, row_top+height);
                    let action = Action::Select(week.number, *index); let selected = self.selected(&action);
                    rounded(dc, cell, if selected { SELECTED } else { CARD }, if selected { ACCENT } else { LINE });
                    let track = schedule.track(week.number, *index).unwrap_or("No race");
                    let changed = self.store.plans.week(&key).iter().any(|plan| schedule.plan_index(plan) == Some(*index) && plan.track != track);
                    text_format(dc, track, rect(cell.left+12, cell.top+8, cell.right-12, cell.bottom-52), FONT_BODY, TEXT, true, DT_WORDBREAK);
                    self.button(dc, if selected { "Planned" } else if changed { "Update track" } else { "Plan race" }, rect(cell.right-108, cell.bottom-44, cell.right-8, cell.bottom-8), action, selected);
                    if let Some(hit) = self.hits.last_mut() { hit.label = format!("Week {} · {} · {track} · {}", week.number, schedule.catalog[*index].name, if selected { "Planned · remove" } else { "Plan race" }); }
                    RestoreDC(dc, clipped);
                }
            }
        } else {
            let mut row_top = top-self.scroll;
            for row in indices.chunks(columns) {
                let height = row.iter().map(|index| self.schedule_view.row_height(*index)).max().unwrap_or(0)-12;
                for (column, index) in row.iter().enumerate() {
                    let start = left+(right-left)*column as i32/columns as i32;
                    let end = left+(right-left)*(column+1) as i32/columns as i32-12;
                    if row_top+height < top || row_top >= layout.bottom { continue; }
                    let series = &schedule.catalog[*index]; let added = favorites.contains(&series.id);
                    rounded(dc, rect(start, row_top, end, row_top+height), CARD, if added { ACCENT } else { LINE });
                    self.car_art(dc, series, rect(start+8, row_top+8, end-8, row_top+100));
                    let label = catalog_label(series);
                    text_format(dc, &label, rect(start+12, row_top+108, end-12, row_top+height-72), FONT_BODY, TEXT, true, DT_WORDBREAK);
                    let condition = self.series_condition(series);
                    text(dc, &condition, rect(start+12, row_top+height-72, end-12, row_top+height-40), FONT_CAPTION, if condition.starts_with("Conditional") { WARNING } else { MUTED }, false, false);
                    let middle = (start+end)/2;
                    self.button(dc, "Details", rect(start+8, row_top+height-36, middle-4, row_top+height-4), Action::SeriesInfo(*index), false);
                    self.button(dc, if added { "✓ Added" } else { "+ Add to comparison" }, rect(middle+4, row_top+height-36, end-8, row_top+height-4), Action::Favorite(*index), added);
                }
                row_top += height+12;
            }
        }
        for hit in &mut self.hits[start_hits..] {
            hit.rect.top = hit.rect.top.max(top); hit.rect.bottom = hit.rect.bottom.min(layout.bottom);
            if matches!(hit.action, Action::Select(_, _)) { hit.rect.left = hit.rect.left.max(left+132); hit.rect.right = hit.rect.right.min(right); }
        }
        self.hits.retain(|hit| hit.rect.bottom > hit.rect.top && hit.rect.right > hit.rect.left);
        RestoreDC(dc, clip);
    }
    pub(super) fn season_cell(&self, layout: &Layout, column: usize, top: i32, height: i32) -> RECT {
        let left = layout.left+132-self.schedule_view.horizontal;
        rect(left+self.schedule_view.column_width*column as i32, top, left+self.schedule_view.column_width*(column+1) as i32-8, top+height)
    }
    pub(super) fn series_condition(&self, series: &crate::race_catalog::Series) -> String {
        let license = self.store.config.licenses.get(&series.category).copied();
        if license.map(|license| license < series.class).unwrap_or(false) { format!("Conditional eligibility · {}", series.minimum_label()) }
        else { series.sr_condition(license).unwrap_or_else(|| if license.is_some() { "Within your license".into() } else { format!("Not set · {}", series.minimum_label()) }) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str) -> State {
        let root = std::env::temp_dir().join(format!("paddock-schedule-{name}-{}-{}", std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap()));
        State { store: Store::open(root).unwrap(), schedule: Some({ let mut schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap(); schedule.expand_catalog(include_str!("../assets/schedule-full-fixture.txt")).unwrap(); schedule }), schedule_view: Default::default(), controller: Controller::new(), page: 0, hits: vec![], scroll: 0, message: String::new(), refresh: None, tray_ready: false, auto_tray_done: false, fixture: false, ignore_close: false, mutex: null_mut(), input: Interaction::default() }
    }

    #[test]
    fn week_labels_use_saved_tracks_without_rewriting_or_guessing_series() {
        let mut plan: Plan = serde_json::from_str(r#"{"season":"2026-S4","week":5,"date":"2026-10-13","series":999,"track":"Road America - Full Course"}"#).unwrap();
        for (track, label) in [
            ("Road America - Full Course", "Road America - Full Course"),
            ("Shell V-Power Motorsport Park at The Bend - West Circuit", "Shell V-Power Motorsport Park at The Bend - West Circuit"),
            ("Circuit de Lédenon", "Circuit de Lédenon"),
            ("Charlotte Motor Speedway - Oval - 2018", "Charlotte Motor Speedway - Oval - 2018"),
            ("", "No saved track"),
        ] {
            plan.track = track.into();
            assert_eq!(saved_week_label(Some(&plan)), label);
            assert_eq!(plan.track, track);
        }
        assert_eq!(saved_week_label(None), "Not planned");
    }

    #[test]
    fn week_series_labels_preserve_variants_legacy_choices_and_saved_names() {
        let state = state("series-labels");
        let schedule = state.schedule.as_ref().unwrap();
        let mut plan: Plan = serde_json::from_str(r#"{"season":"2026-S4","week":5,"date":"2026-10-13","series":3,"track":"Road America - Full Course"}"#).unwrap();
        assert_eq!(saved_series_label(schedule, Some(&plan)), "Porsche · Fixed");
        plan.series = 4;
        assert_eq!(saved_series_label(schedule, Some(&plan)), "Porsche · Open");
        plan.series_id = schedule.catalog[6].id.clone();
        plan.series_name = schedule.catalog[6].name.clone();
        assert_eq!(saved_series_label(schedule, Some(&plan)), "F4 · Asia Pacific");
        plan.series_id = "missing-series".into();
        plan.series_name = "Saved series - Fixed".into();
        assert_eq!(saved_series_label(schedule, Some(&plan)), "Saved series - Fixed");
        plan.series_name.clear();
        assert_eq!(saved_series_label(schedule, Some(&plan)), "Saved series");
        assert_eq!(saved_series_label(schedule, None), "");
        assert_eq!(saved_week_label(Some(&plan)), "Road America - Full Course");
        std::fs::remove_dir_all(state.store.root).unwrap();
    }

    #[test]
    fn favorites_seed_from_existing_plans_and_remain_stable_and_independent() {
        let mut state = state("favorites");
        unsafe {
            state.action(null_mut(), Action::Select(5, 3));
            let schedule = state.schedule.clone().unwrap();
            let plans = std::fs::read(state.store.root.join("plans.json")).unwrap();
            let config = std::fs::read(state.store.root.join("config.json")).unwrap();
            assert_eq!(favorite_ids(&state.store, &schedule), [schedule.catalog[3].id.clone()].into_iter().collect());
            assert_eq!(state.schedule_view.visible_groups(&schedule, &state.store)[0].1, vec![3]);
            assert_eq!(std::fs::read(state.store.root.join("config.json")).unwrap(), config);
            state.action(null_mut(), Action::Favorite(2));
            state.action(null_mut(), Action::Favorite(3));
            let reopened = Store::open(state.store.root.clone()).unwrap();
            assert_eq!(reopened.config.favorite_series.unwrap(), [schedule.catalog[2].id.clone()].into_iter().collect());
            assert_eq!(std::fs::read(state.store.root.join("plans.json")).unwrap(), plans);
            state.schedule.as_mut().unwrap().catalog.swap(2, 9);
            assert!(state.schedule_view.visible_groups(state.schedule.as_ref().unwrap(), &state.store).iter().any(|(_, indices)| indices.contains(&9)));
            state.action(null_mut(), Action::Favorite(9));
            assert!(favorite_ids(&state.store, state.schedule.as_ref().unwrap()).is_empty());
            state.action(null_mut(), Action::FavoritesOnly(false));
            assert!(state.schedule_view.visible_groups(state.schedule.as_ref().unwrap(), &state.store).iter().map(|(_, indices)| indices.len()).sum::<usize>() > 100);
        }
        std::fs::remove_dir_all(state.store.root).unwrap();
    }

    #[test]
    fn all_week_rows_are_reachable_and_catalog_filters_do_not_hide_registered_series() {
        let mut state = state("browse");
        unsafe {
            state.action(null_mut(), Action::Select(5, 3)); state.action(null_mut(), Action::Select(5, 2)); state.action(null_mut(), Action::Select(6, 6));
            let plans = std::fs::read(state.store.root.join("plans.json")).unwrap();
            let dc = CreateCompatibleDC(null_mut());
            for filter in Filter::ALL {
                state.action(null_mut(), Action::Filter(filter));
                for number in 1..=12 {
                    state.action(null_mut(), Action::Week(number)); state.draw(dc, 1034, 800);
                    assert!(state.hits.iter().any(|hit| hit.action == Action::SavedInfo(number)));
                    assert!(state.hits.iter().any(|hit| hit.action == Action::WeekMenu));
                    let visible = state.schedule_view.indices(state.schedule.as_ref().unwrap(), &state.store);
                    assert!(visible.contains(&2) && visible.contains(&3) && visible.contains(&6));
                }
            }
            assert_eq!(state.store.plans.week("2026-S4:5").len(), 2);
            assert_eq!(std::fs::read(state.store.root.join("plans.json")).unwrap(), plans);
            DeleteDC(dc);
        }
        std::fs::remove_dir_all(state.store.root).unwrap();
    }

    #[test]
    fn multiple_selections_toggle_independently_and_survive_reopen() {
        let mut state = state("selection");
        unsafe {
            state.action(null_mut(), Action::Select(5, 3));
            state.action(null_mut(), Action::Select(6, 6));
            state.action(null_mut(), Action::Filter(Filter::Formula));
            state.action(null_mut(), Action::Select(5, 2));
            let restored = Store::open(state.store.root.clone()).unwrap();
            assert_eq!(restored.plans.choices.len(), 2);
            assert_eq!(restored.plans.week("2026-S4:5").iter().map(|plan| plan.series).collect::<Vec<_>>(), vec![3, 2]);
            assert_eq!(restored.plans.choices["2026-S4:5"].series, 2);
            assert_eq!(restored.plans.choices["2026-S4:6"].series, 6);
            state.action(null_mut(), Action::Select(5, 2));
            assert_eq!(state.store.plans.choices.len(), 2);
            assert!(state.store.plans.choices.contains_key("2026-S4:6"));
            state.action(null_mut(), Action::Select(0, 0)); state.action(null_mut(), Action::Select(6, usize::MAX));
            assert_eq!(state.store.plans.choices.len(), 2);
            state.action(null_mut(), Action::Remove("2026-S4:5".into()));
            state.action(null_mut(), Action::Remove("2026-S4:6".into()));
            assert!(Store::open(state.store.root.clone()).unwrap().plans.choices.is_empty());
        }
        std::fs::remove_dir_all(state.store.root).unwrap();
    }

    #[test]
    fn removed_plans_are_restored_without_overwriting_new_choices() {
        let mut state = state("undo-plans");
        unsafe {
            state.action(null_mut(), Action::Select(5, 3)); state.action(null_mut(), Action::Remove("2026-S4:5".into()));
            state.store = Store::open(state.store.root.clone()).unwrap();
            assert_eq!(state.store.plans.selection_history["2026-S4:5"].last().unwrap()[0].series, 3);
            state.store.warnings.push("fixture write failure".into());
            state.action(null_mut(), Action::UndoPlan("2026-S4:5".into())); assert!(!state.store.plans.choices.contains_key("2026-S4:5"));
            assert!(state.store.plans.can_undo("2026-S4:5")); state.store.warnings.clear();
            state.action(null_mut(), Action::UndoPlan("2026-S4:5".into())); assert_eq!(state.store.plans.choices["2026-S4:5"].series, 3);
            state.action(null_mut(), Action::Remove("2026-S4:5".into())); state.action(null_mut(), Action::Select(5, 1));
            state.action(null_mut(), Action::UndoPlan("2026-S4:5".into())); assert!(!state.store.plans.choices.contains_key("2026-S4:5"));
            state.action(null_mut(), Action::UndoPlan("2026-S4:5".into())); assert_eq!(state.store.plans.choices["2026-S4:5"].series, 3);
        }
        std::fs::remove_dir_all(state.store.root).unwrap();
    }

    #[test]
    fn appearance_save_failure_restores_previous_preferences() {
        let mut state = state("appearance");
        unsafe {
            state.action(null_mut(), Action::Theme(design::Theme::Dark)); state.action(null_mut(), Action::Density(design::Density::Compact));
            let restored = Store::open(state.store.root.clone()).unwrap(); assert_eq!(restored.config.appearance, state.store.config.appearance);
            std::fs::remove_file(state.store.root.join("config.json")).unwrap(); std::fs::create_dir(state.store.root.join("config.json")).unwrap();
            let previous = state.store.config.appearance; state.action(null_mut(), Action::Theme(design::Theme::Light));
            assert_eq!(state.store.config.appearance, previous); assert!(state.message.contains("failed"));
        }
        design::set(design::Preferences::legacy()); std::fs::remove_dir_all(state.store.root).unwrap();
    }

    #[test]
    fn changed_schedule_preserves_old_track_until_reselected_and_failed_saves_roll_back() {
        let mut state = state("changes");
        unsafe {
            state.action(null_mut(), Action::Select(5, 3));
            let original = state.store.plans.choices["2026-S4:5"].track.clone();
            state.schedule.as_mut().unwrap().weeks[4].tracks[3] = "Updated circuit".into();
            state.action(null_mut(), Action::Filter(Filter::Formula));
            assert_eq!(state.store.plans.choices["2026-S4:5"].track, original);
            assert_eq!(saved_week_label(state.store.plans.choices.get("2026-S4:5")), "Road America - Full Course");
            state.action(null_mut(), Action::Select(5, 3));
            assert_eq!(state.store.plans.choices["2026-S4:5"].track, "Updated circuit");
            assert_eq!(saved_week_label(state.store.plans.choices.get("2026-S4:5")), "Updated circuit");
            state.store.warnings.push("fixture save guard".into());
            state.action(null_mut(), Action::Select(5, 1));
            assert_eq!(state.store.plans.choices["2026-S4:5"].series, 3);
            state.action(null_mut(), Action::Remove("2026-S4:5".into()));
            assert!(state.store.plans.choices.contains_key("2026-S4:5"));
        }
        std::fs::remove_dir_all(state.store.root).unwrap();
    }

    #[test]
    fn upper_class_requires_explicit_conditional_filter_and_never_filters_my_series() {
        let mut state = state("conditional");
        unsafe { state.action(null_mut(), Action::Favorite(3)); state.action(null_mut(), Action::License(Category::SportsCar, Some(License::D))); }
        let schedule = state.schedule.as_ref().unwrap();
        assert!(!state.schedule_view.indices(schedule, &state.store).contains(&3));
        state.schedule_view.conditional = true;
        assert!(state.schedule_view.indices(schedule, &state.store).contains(&3));
        assert!(state.series_condition(&schedule.catalog[3]).starts_with("Conditional eligibility"));
        state.schedule_view.favorites_only = true;
        state.store.config.licenses.insert(Category::SportsCar, License::Rookie);
        state.schedule_view.filter = Filter::Formula;
        assert!(state.schedule_view.indices(schedule, &state.store).contains(&3));
        std::fs::remove_dir_all(state.store.root).unwrap();
    }
    #[test]
    fn licenses_are_separate_persisted_and_include_all_matching_series() {
        let mut state = state("licenses");
        unsafe {
            state.action(null_mut(), Action::Week(5));
            state.action(null_mut(), Action::License(Category::SportsCar, Some(License::D)));
            state.action(null_mut(), Action::License(Category::Formula, Some(License::B)));
        }
        state.schedule_view.filter = Filter::All;
        let schedule = state.schedule.as_ref().unwrap();
        for (category, indices) in state.schedule_view.groups(schedule, &state.store.config.licenses) {
            let expected: std::collections::HashSet<_> = schedule.catalog.iter().enumerate().filter(|(_, series)| series.category == category && series.accepts_class(state.store.config.licenses.get(&category).copied()) && (state.schedule_view.conditional || state.store.config.licenses.get(&category).map(|license| series.class <= *license).unwrap_or(true)) && schedule.weeks.iter().any(|week| series.entry(&week.start).is_some())).map(|(index, _)| index).collect();
            assert_eq!(indices.into_iter().collect::<std::collections::HashSet<_>>(), expected);
        }
        let reopened = Store::open(state.store.root.clone()).unwrap();
        assert_eq!(reopened.config.licenses, state.store.config.licenses);
        assert_eq!(reopened.config.licenses.len(), 2);
        unsafe {
            state.action(null_mut(), Action::Select(5, 3));
            let plans = std::fs::read(state.store.root.join("plans.json")).unwrap();
            state.action(null_mut(), Action::License(Category::SportsCar, Some(License::Rookie)));
            state.action(null_mut(), Action::Filter(Filter::SportsCar));
            assert!(!state.schedule_view.groups(state.schedule.as_ref().unwrap(), &state.store.config.licenses)[0].1.contains(&3));
            assert_eq!(std::fs::read(state.store.root.join("plans.json")).unwrap(), plans);
            state.action(null_mut(), Action::License(Category::SportsCar, None));
            assert!(!Store::open(state.store.root.clone()).unwrap().config.licenses.contains_key(&Category::SportsCar));
            std::fs::remove_file(state.store.root.join("config.json")).unwrap();
            std::fs::create_dir(state.store.root.join("config.json")).unwrap();
            let previous = state.store.config.licenses.clone();
            state.action(null_mut(), Action::License(Category::SportsCar, Some(License::A)));
            assert_eq!(state.store.config.licenses, previous);
        }
        std::fs::remove_dir_all(state.store.root).unwrap();
    }
    #[test]
    fn stable_series_identity_survives_reordering_and_legacy_plans() {
        let mut state = state("stable");
        let index = state.schedule.as_ref().unwrap().catalog.iter().position(|series| series.name.starts_with("BMW M2 Cup")).unwrap();
        unsafe { state.action(null_mut(), Action::Select(5, index)); state.action(null_mut(), Action::Select(6, 6)); }
        let reopened = Store::open(state.store.root.clone()).unwrap();
        let schedule = state.schedule.as_mut().unwrap();
        schedule.catalog.swap(index, index+1);
        assert_eq!(schedule.plan_index(&reopened.plans.choices["2026-S4:5"]), Some(index+1));
        let legacy: Plan = serde_json::from_str(r#"{"season":"2026-S4","week":6,"date":"2026-10-20","series":6,"track":"old"}"#).unwrap();
        assert_eq!(schedule.plan_index(&legacy), Some(6));
        unsafe { state.action(null_mut(), Action::Select(5, index+1)); }
        assert_eq!(state.store.plans.choices.len(), 1);
        assert!(state.store.plans.week("2026-S4:5").is_empty());
        assert!(state.store.plans.choices.contains_key("2026-S4:6"));
        std::fs::remove_dir_all(state.store.root).unwrap();
    }
}
