use crate::{app_control::{self, Controller, Phase}, season_schedule::{self, Schedule}, storage::{self, Plan, Store, wide}};
use crate::design;
use std::{path::{Path, PathBuf}, ptr::{null, null_mut}, sync::mpsc::{self, Receiver}, thread};
use windows_sys::Win32::{Foundation::*, Graphics::{Gdi::*, Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE}}, System::{LibraryLoader::GetModuleHandleW, Threading::{CreateMutexW, ReleaseMutex}}, UI::{Controls::{Dialogs::*, WM_MOUSELEAVE}, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*}};
#[path = "car_art.rs"]
mod car_art;
#[path = "planner_search.rs"]
mod planner_search;
#[path = "schedule_ui.rs"]
mod schedule_ui;
#[path = "planner_ui.rs"]
mod planner_ui;
#[path = "query_dialog.rs"]
mod query_dialog;
#[path = "profiles_ui.rs"]
mod profiles_ui;
#[path = "reading_text.rs"]
mod reading_text;
#[path = "dialogs.rs"]
mod dialogs;
#[path = "accessibility.rs"]
mod accessibility;
#[path = "viewport.rs"]
mod viewport;
#[path = "responsive_ui.rs"]
mod responsive_ui;
#[cfg(test)]
#[path = "regression_tests.rs"]
mod regression_tests;

thread_local! { static READ_TEXT: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) }; }

const TRAY_MESSAGE: u32 = WM_APP + 1;
const RESTORE_MESSAGE: u32 = WM_APP + 2;
const BG: u32 = design::CANVAS;
const SIDE: u32 = design::SUBTLE;
const CARD: u32 = design::SURFACE;
const TEXT: u32 = design::TEXT;
const MUTED: u32 = design::SECONDARY;
const LINE: u32 = design::BORDER;
const ACCENT: u32 = design::LINK;
const SELECTED: u32 = design::SELECTION;
const POSITIVE: u32 = design::SUCCESS_TEXT;
const DANGER: u32 = design::DANGER_TEXT;
const WARNING: u32 = design::WARNING_TEXT;
const HOVER: u32 = design::HOVER;
const PRESSED: u32 = design::PRESSED;
const DISABLED: u32 = design::SECONDARY;
const FONT_FAMILY: &str = design::FONT;
const FONT_CAPTION: i32 = 13;
const FONT_BODY: i32 = 15;
const FONT_SECTION: i32 = 18;
const FONT_TITLE: i32 = 26;
const SIDEBAR_WIDTH: i32 = 208;
const CONTENT_TOP: i32 = 124;
const TABLE_TOP: i32 = 216;
const FOOTER_HEIGHT: i32 = 48;
fn app_pitch() -> i32 { design::row_height()*2 }
const PAGE_SCHEDULE: u8 = 0;
const PAGE_APPS: u8 = 1;
const PAGE_LEARN:u8=2;
const PAGE_TITLES: [&str; 3] = ["Planner", "Apps", "Learn"];
fn read_only_review() -> bool {
    static REVIEW: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REVIEW.get_or_init(|| std::env::args().any(|argument| argument == "--review-ui-read-only"))
}

struct Layout { left: i32, right: i32, bottom: i32 }
impl Layout {
    fn planner(width: i32, height: i32) -> Self {
        let available = width-SIDEBAR_WIDTH-64;
        let content_width = available.min(1440);
        let left = SIDEBAR_WIDTH+32+(available-content_width)/2;
        Self { left, right: left+content_width, bottom: height-FOOTER_HEIGHT }
    }
    fn new(width: i32, height: i32) -> Self {
        let available = width-SIDEBAR_WIDTH-64;
        let content_width = available.min(design::dimension("layout", "content-max"));
        let left = SIDEBAR_WIDTH+32+(available-content_width)/2;
        Self { left, right: left+content_width, bottom: height-FOOTER_HEIGHT }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum Action { ProfileMenu, ProfileSelect(usize), ProfileNew, ProfileDuplicate, ProfileRename, AppAdd, AppRename(usize), AppRemove(usize), LearnOpen(usize), PlannerMode(crate::planner::Mode), PlannerClass(Option<crate::race_catalog::License>), PlannerBeginner, PlannerReset, PlannerFavorites, PlannerSort, PlannerCategory, PlannerSearch, PlannerWeek(i8), PlannerToday, PlannerFilters, PlannerCategoryToggle(crate::race_catalog::Category), PlannerHidden, PlannerHide(usize), PlannerCollapse(crate::race_catalog::License), PlannerQueryClear, ToolsMenu, ScheduleTools, Inspect(String), SavedInfo(u8), Locate(u8, usize), Hide(usize), AppDetails(usize), MessageDetails, WeekMenu, ConditionalSeries, SeriesJump, ThemeMenu, Theme(design::Theme), #[allow(dead_code)] Density(design::Density), UndoPlan(String), Page(u8), Week(u8), Filter(schedule_ui::Filter), CategoryMenu, LicenseMenu, License(crate::race_catalog::Category, Option<crate::race_catalog::License>), SeriesInfo(usize), Select(u8, usize), Remove(String), Refresh, Start, RetryStop, Tray, Restore(usize), HideAll, Enable(usize), TrayMode(usize), Browse(usize), LauncherTray, Source, Data, Exit, Favorite(usize), FavoritesOnly(bool) }
struct Hit { rect: RECT, action: Action, label: String }
#[derive(Default)]
struct Interaction {
    page_scroll: [i32; 3],
    schedule_scroll: std::collections::HashMap<(schedule_ui::Filter, bool), i32>,
    locate_after_draw: Option<Action>,
    reflow_targets: Vec<(Action, i32, i32)>,
    tray_attempts: u8,
    scrollbar: Option<(RECT, RECT, i32)>,
    scroll_drag: Option<i32>,
    reflow_height: i32,
    reflow_top: i32,
    reflow: bool,
    viewport: viewport::Viewport,
    reading: String,
    ime_composing: bool,
    pointer: Option<(i32, i32)>,
    hovered: Option<Action>,
    pressed: Option<Action>,
    focused: Option<Action>,
    keyboard_key: Option<u32>,
    focus_visible: bool,
}
struct State {
    store: Store,
    schedule: Option<Schedule>,
    controller: Controller,
    page: u8,
    hits: Vec<Hit>,
    scroll: i32,
    message: String,
    refresh: Option<Receiver<Result<Schedule, String>>>,
    tray_ready: bool,
    auto_tray_done: bool,
    fixture: bool,
    ignore_close: bool,
    mutex: HANDLE,
    input: Interaction,
    schedule_view: schedule_ui::View,
}

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT { RECT { left, top, right, bottom } }
unsafe fn modal_button(data: LPARAM) -> LRESULT {
    use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED, ODS_FOCUS, ODS_DISABLED};
    let item = &*(data as *const DRAWITEMSTRUCT);
    let mut buffer = [0u16; 128];
    let length = GetWindowTextW(item.hwndItem, buffer.as_mut_ptr(), buffer.len() as i32);
    let label = String::from_utf16_lossy(&buffer[..length.max(0) as usize]);
    let dpi = design::effective_dpi(GetDpiForWindow(item.hwndItem));
    let saved = SaveDC(item.hDC); logical_scale(item.hDC, dpi);
    let area = rect(item.rcItem.left*96/dpi, item.rcItem.top*96/dpi, item.rcItem.right*96/dpi, item.rcItem.bottom*96/dpi);
    fill(item.hDC,area,CARD);
    let primary=GetDlgCtrlID(item.hwndItem)==IDOK&&item.itemState&ODS_DISABLED==0;
    let background=if primary{if item.itemState&ODS_SELECTED!=0{design::ACCENT_HOVER}else{design::ACCENT}}else if item.itemState&ODS_SELECTED!=0{PRESSED}else{CARD};
    let foreground=if item.itemState&ODS_DISABLED!=0{MUTED}else if primary{design::ON_ACCENT}else{TEXT};
    rounded(item.hDC, area,background,if primary{background}else{LINE});
    text_format(item.hDC, &label, rect(area.left+4, area.top, area.right-4, area.bottom), FONT_BODY,foreground,primary, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    if item.itemState & ODS_FOCUS != 0 {
        let pen=CreatePen(PS_SOLID,2,design::resolve(if primary{design::ON_ACCENT}else{design::FOCUS}));let old_pen=SelectObject(item.hDC,pen);let old_brush=SelectObject(item.hDC,GetStockObject(NULL_BRUSH));
        RoundRect(item.hDC,area.left+3,area.top+3,area.right-3,area.bottom-3,12,12);SelectObject(item.hDC,old_brush);SelectObject(item.hDC,old_pen);DeleteObject(pen);
    }
    RestoreDC(item.hDC, saved); 1
}
unsafe fn logical_scale(dc: HDC, dpi: i32) {
    SetMapMode(dc, MM_ANISOTROPIC); SetWindowExtEx(dc, 96, 96, null_mut()); SetViewportExtEx(dc, dpi, dpi, null_mut());
}
unsafe fn apply_window_theme(window: HWND) {
    if window.is_null() { return; }
    let dark = i32::from(design::dark());
    DwmSetWindowAttribute(window, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, &dark as *const _ as _, 4);
}
unsafe fn mouse_point(window: HWND, data: LPARAM) -> (i32, i32) {
    let dpi = design::effective_dpi(GetDpiForWindow(window));
    let state = &*(GetWindowLongPtrW(window, GWLP_USERDATA) as *const State);
    ((data as i16 as i32)*96/dpi+state.input.viewport.x, ((data >> 16) as i16 as i32)*96/dpi+state.input.viewport.y)
}
unsafe fn fill(dc: HDC, area: RECT, color: u32) {
    let brush = CreateSolidBrush(design::resolve(color)); FillRect(dc, &area, brush); DeleteObject(brush);
}
unsafe fn rounded(dc: HDC, area: RECT, color: u32, border: u32) {
    let radius = design::dimension("radius", if area.bottom-area.top > 56 { "panel" } else { "control" });
    rounded_radius(dc,area,color,border,radius);
}
unsafe fn rounded_radius(dc: HDC, area: RECT, color: u32, border: u32, radius: i32) {
    let brush = CreateSolidBrush(design::resolve(color)); let pen = CreatePen(PS_SOLID, 1, design::resolve(border));
    let old_brush = SelectObject(dc, brush); let old_pen = SelectObject(dc, pen);
    RoundRect(dc, area.left, area.top, area.right, area.bottom, radius*2, radius*2);
    SelectObject(dc, old_brush); SelectObject(dc, old_pen); DeleteObject(brush); DeleteObject(pen);
}
unsafe fn ui_font(size: i32, bold: bool) -> HFONT {
    design::ensure_font();
    CreateFontW(-size, 0, 0, 0, if bold { 600 } else { 400 }, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32, wide(FONT_FAMILY).as_ptr())
}
#[cfg(test)]
unsafe fn measured_text(dc: HDC, value: &str, area: RECT, format: u32) -> RECT {
    let mut measured = area;
    DrawTextW(dc, wide(value).as_ptr(), -1, &mut measured, DT_NOPREFIX | DT_CALCRECT | (format & !(DT_VCENTER | DT_END_ELLIPSIS | DT_PATH_ELLIPSIS)));
    measured
}
unsafe fn text_format(dc: HDC, value: &str, area: RECT, size: i32, color: u32, bold: bool, format: u32) {
    READ_TEXT.with(|reading| reading.borrow_mut().push(value.into()));
    let font = ui_font(size, bold);
    let previous = SelectObject(dc, font); SetTextColor(dc, design::resolve(color)); SetBkMode(dc, TRANSPARENT as i32);
    if format & DT_WORDBREAK != 0 {
        let role = if size < FONT_BODY { "caption" } else if size < FONT_SECTION { "body" } else { "heading" };
        let line_height = (size as f64*design::tokens()["typography"]["lineHeight"][role].as_str().unwrap().parse::<f64>().unwrap()).round() as i32;
        let lines = wrapped_lines(dc, value, (area.right-area.left).max(1));
        let count = lines.len().min(((area.bottom-area.top)/line_height).max(0) as usize);
        let top = area.top+if format & DT_VCENTER != 0 { ((area.bottom-area.top)-count as i32*line_height).max(0)/2 } else { 0 };
        for (index, line) in lines.iter().take(count).enumerate() {
            let label = if index+1 == count && count < lines.len() { format!("{line}…") } else { line.clone() };
            let mut line_area = rect(area.left, top+index as i32*line_height, area.right, top+(index as i32+1)*line_height);
            DrawTextW(dc, wide(&label).as_ptr(), -1, &mut line_area, DT_NOPREFIX | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | (format & (DT_CENTER | DT_RIGHT)));
        }
    } else {
        let mut area = area;
        DrawTextW(dc, wide(value).as_ptr(), -1, &mut area, DT_NOPREFIX | format);
    }
    SelectObject(dc, previous); DeleteObject(font);
}
unsafe fn wrapped_lines(dc: HDC, value: &str, width: i32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in value.split('\n') {
        let mut remaining = paragraph.trim_end_matches('\r');
        if remaining.is_empty() { lines.push(String::new()); }
        while !remaining.is_empty() {
            let mut fitted = 0;
            for (offset, character) in remaining.char_indices() {
                let end = offset+character.len_utf8(); let encoded = wide(&remaining[..end]); let mut extent = SIZE::default();
                GetTextExtentPoint32W(dc, encoded.as_ptr(), encoded.len() as i32-1, &mut extent);
                if extent.cx > width && fitted > 0 { break; } fitted = end;
                if extent.cx > width { break; }
            }
            let end = if fitted < remaining.len() { remaining[..fitted].rfind(char::is_whitespace).filter(|offset| *offset > 0).unwrap_or(fitted) } else { fitted };
            lines.push(remaining[..end].trim_end().into()); remaining = remaining[end..].trim_start();
        }
    }
    lines
}
unsafe fn text(dc: HDC, value: &str, area: RECT, size: i32, color: u32, bold: bool, wrap: bool) {
    text_format(dc, value, area, size, color, bold, DT_END_ELLIPSIS | if wrap { DT_WORDBREAK } else { DT_SINGLELINE | DT_VCENTER });
}
impl State {
    fn switch_schedule_filter(&mut self, filter: schedule_ui::Filter, favorites: bool) {
        self.input.schedule_scroll.insert((self.schedule_view.filter, self.schedule_view.favorites_only), self.scroll);
        self.schedule_view.filter = filter; self.schedule_view.favorites_only = favorites;
        self.scroll = self.input.schedule_scroll.get(&(filter, favorites)).copied().unwrap_or(0);
    }
    unsafe fn save_appearance(&mut self, window: HWND, previous: design::Preferences) {
        let result = self.store.save_config().map_err(|issue| format!("Save failed. Check data folder permissions: {issue}"));
        if result.is_err() { self.store.config.appearance = previous; }
        design::set(self.store.config.appearance); self.scroll = 0;
        apply_window_theme(window); self.report(result, "Appearance saved");
    }
    fn selected(&self, action: &Action) -> bool {
        match action {
            Action::PlannerCategoryToggle(category) => self.store.config.planner.categories.contains(category) || (self.store.config.planner.categories.is_empty() && self.store.config.planner.category == Some(*category)),
            Action::PlannerHidden => self.store.config.planner.hidden_only,
            Action::PlannerHide(index) => self.schedule.as_ref().and_then(|schedule| schedule.catalog.get(*index)).map(|series| self.store.config.planner.hidden_series.contains(&series.id)).unwrap_or(false),
            Action::PlannerCollapse(class) => self.store.config.planner.collapsed_classes.contains(class),
            Action::PlannerMode(mode) => self.store.config.planner.mode == *mode,
            Action::PlannerClass(class) => class.map(|class| self.store.config.planner.classes.contains(&class)).unwrap_or(self.store.config.planner.classes.is_empty()),
            Action::PlannerFavorites => self.store.config.planner.favorites_only,
            Action::PlannerBeginner => self.store.config.planner.classes == crate::planner::Preferences::beginner().classes,
            Action::Page(page) => self.page == *page,
            Action::FavoritesOnly(value) => !self.schedule_view.weekly && self.schedule_view.favorites_only == *value,
            Action::ConditionalSeries => self.schedule_view.conditional,
            Action::LauncherTray => self.store.config.launcher_to_tray,
            Action::Enable(index) => self.store.config.apps.get(*index).map(|spec| spec.enabled).unwrap_or(false),
            Action::TrayMode(index) => self.store.config.apps.get(*index).map(|spec| spec.tray).unwrap_or(false),
            Action::Week(number) => self.schedule.as_ref().map(|schedule| self.schedule_view.week_number(schedule) == *number).unwrap_or(false),
            Action::Favorite(index) => self.schedule.as_ref().and_then(|schedule| schedule.catalog.get(*index).map(|series| schedule_ui::favorite_ids(&self.store, schedule).contains(&series.id))).unwrap_or(false),
            Action::Select(number, index) => self.schedule.as_ref().map(|schedule| self.store.plans.week(&schedule.key(*number)).iter().any(|plan| schedule.plan_index(plan) == Some(*index) && schedule.track(*number, *index) == Some(plan.track.as_str()))).unwrap_or(false),
            _ => false
        }
    }
    fn enabled(&self, action: &Action) -> bool {
        if read_only_review() && matches!(action, Action::Start | Action::Exit | Action::RetryStop | Action::Restore(_) | Action::Hide(_) | Action::HideAll) { return false; }
        match action {
            Action::PlannerWeek(delta) => self.schedule.as_ref().map(|schedule| schedule.weeks.iter().any(|week| week.number as i16 == self.schedule_view.week_number(schedule) as i16 + *delta as i16)).unwrap_or(false),
            Action::PlannerToday => self.schedule.as_ref().and_then(|schedule| schedule.active_week()).is_some(),
            Action::Page(page) => (*page as usize) < PAGE_TITLES.len(),
            Action::Week(number) => self.schedule.as_ref().map(|schedule| schedule.weeks.iter().any(|week| week.number == *number)).unwrap_or(false),
            Action::LicenseMenu => self.license_editable(),
            Action::Select(number, series) => self.schedule.as_ref().and_then(|schedule| schedule.track(*number, *series)).is_some(),
            Action::Remove(key) => self.store.plans.choices.contains_key(key),
            Action::UndoPlan(key) => self.store.plans.can_undo(key),
            Action::Refresh => self.refresh.is_none(),
            Action::Start => matches!(self.controller.phase, Phase::Idle | Phase::Running),
            Action::TrayMode(index) => self.controller.phase == Phase::Idle && self.store.config.apps.get(*index).is_some_and(|app|app.id!="iracing"),
            Action::ProfileMenu | Action::ProfileSelect(_) | Action::ProfileNew | Action::ProfileDuplicate | Action::ProfileRename | Action::AppAdd | Action::AppRename(_) | Action::AppRemove(_) | Action::AppDetails(_) | Action::Enable(_) | Action::Browse(_) => self.controller.phase == Phase::Idle,
            Action::Exit => self.controller.phase != Phase::Stopping,
            Action::RetryStop => self.controller.phase == Phase::CleanupFailed,
            Action::HideAll => self.store.config.apps.iter().any(|spec| spec.id != "iracing" && spec.enabled && self.controller.status.get(&spec.id).copied().unwrap_or(0) > 0),
            Action::Restore(index) | Action::Hide(index) => self.store.config.apps.get(*index).and_then(|spec| self.controller.status.get(&spec.id)).copied().unwrap_or(0) > 0,
            _ => true,
        }
    }
    fn hit_at(&self, point: (i32, i32)) -> Option<Action> {
        self.hits.iter().rev().find(|hit| self.enabled(&hit.action) && point.0 >= hit.rect.left && point.0 < hit.rect.right && point.1 >= hit.rect.top && point.1 < hit.rect.bottom).map(|hit| hit.action.clone())
    }
    fn sync_hover(&mut self) -> bool {
        let hovered = self.input.pointer.and_then(|point| self.hit_at(point));
        if hovered == self.input.hovered { return false; }
        self.input.hovered = hovered; true
    }
    fn focus_next(&mut self, backwards: bool) {
        let mut actions: Vec<_> = self.hits.iter().filter(|hit| self.enabled(&hit.action) && (!self.input.reflow || hit.rect.top < self.input.reflow_top)).map(|hit| hit.action.clone()).collect();
        if self.input.reflow { actions.extend(self.input.reflow_targets.iter().filter(|(action, _, _)| self.enabled(action)).map(|(action, _, _)| action.clone())); }
        if actions.is_empty() { return; }
        let current = actions.iter().position(|action| Some(action) == self.input.focused.as_ref());
        let next = match current { Some(index) if backwards => (index+actions.len()-1)%actions.len(), Some(index) => (index+1)%actions.len(), None if backwards => actions.len()-1, None => 0 };
        self.input.focused = Some(actions[next].clone()); self.input.focus_visible = true;
        if self.input.reflow {
            if let Some((_, top, size)) = self.input.reflow_targets.iter().find(|(action, _, _)| action == &actions[next]) {
                let available = (self.input.viewport.height()-self.input.reflow_top-8).max(1);
                if *top < self.scroll { self.scroll = *top; } else if top+size > self.scroll+available { self.scroll = (top+size-available).max(*top); }
            }
        }
        self.input.pressed = None; self.input.keyboard_key = None;
    }
    fn scroll_metrics(&self, _width: i32, _height: i32) -> (i32, i32) {
        if self.input.reflow { return (self.input.reflow_top, self.input.reflow_height); }
        match self.page {
            PAGE_SCHEDULE => (self.schedule_view.body_top, self.schedule.as_ref().map(|schedule| self.schedule_view.content_height(schedule, &self.store)).unwrap_or(0)),
            PAGE_APPS => (TABLE_TOP, self.store.config.apps.len() as i32*app_pitch()),
            PAGE_LEARN => (CONTENT_TOP, 480),
            _ => (CONTENT_TOP, 0),
        }
    }
    unsafe fn scroll_by(&mut self, window: HWND, delta: i32) {
        let mut bounds: RECT = std::mem::zeroed(); GetClientRect(window, &mut bounds);
        let height = (bounds.bottom*96/design::effective_dpi(GetDpiForWindow(window))).max(120);
        let width = (bounds.right*96/design::effective_dpi(GetDpiForWindow(window))).max(160);
        let (top, content) = self.scroll_metrics(width, height);
        let fixed_footer = 0;
        let bottom = if self.input.reflow { height-8 } else { Layout::new(width, height).bottom };
        let maximum = (content-(bottom-top-fixed_footer)).max(0);
        self.scroll = (self.scroll+delta).clamp(0, maximum);
        self.input.pressed = None; self.input.keyboard_key = None;
        if GetCapture() == window { ReleaseCapture(); }
    }
    unsafe fn surface(&self, dc: HDC, area: RECT, action: &Action, selected: bool) -> (u32, i32) {
        let enabled = self.enabled(action);
        let hovered = enabled && self.input.hovered.as_ref() == Some(action);
        let pressed = enabled && self.input.pressed.as_ref() == Some(action) && (hovered || self.input.keyboard_key.is_some());
        let navigation = matches!(action, Action::Page(_));
        let app_toggle = matches!(action, Action::Enable(_) | Action::TrayMode(_) | Action::LauncherTray);
        let ghost = matches!(action, Action::Source | Action::Data | Action::Browse(_) | Action::Restore(_) | Action::Hide(_) | Action::AppDetails(_) | Action::SeriesInfo(_) | Action::Favorite(_) | Action::MessageDetails);
        let base = if matches!(action, Action::Browse(_) | Action::Restore(_) | Action::Hide(_) | Action::AppDetails(_) | Action::Enable(_) | Action::TrayMode(_)) { CARD } else if navigation || matches!(action, Action::Exit) { SIDE } else { BG };
        let (background, border, foreground) = if !enabled { (base, base, DISABLED) }
        else if matches!(action, Action::Start) { (if pressed || hovered { design::ACCENT_HOVER } else { design::ACCENT }, design::ACCENT, design::ON_ACCENT) }
        else if matches!(action, Action::Exit | Action::Remove(_)) { let color = if hovered || pressed { design::DANGER_SURFACE } else { base }; (color, color, if hovered || pressed { DANGER } else { MUTED }) }
        else if matches!(action, Action::RetryStop) { (if hovered || pressed { design::WARNING_SURFACE } else { CARD }, design::CONTROL_BORDER, WARNING) }
        else if navigation && selected { (HOVER, HOVER, TEXT) }
        else if app_toggle { let color = if pressed { PRESSED } else if hovered { HOVER } else { base }; (color, color, if selected { TEXT } else { MUTED }) }
        else if navigation || ghost { let color = if pressed { PRESSED } else if hovered { HOVER } else { base }; (color, color, if navigation || matches!(action, Action::AppDetails(_)) { TEXT } else { MUTED }) }
        else if selected { (if pressed { PRESSED } else { HOVER }, HOVER, TEXT) }
        else { (if pressed { PRESSED } else if hovered { HOVER } else { CARD }, LINE, TEXT) };
        rounded(dc, area, background, border);
        if enabled && self.input.focus_visible && self.input.focused.as_ref() == Some(action) {
            let pen = CreatePen(PS_SOLID, 2, design::resolve(if matches!(action, Action::Start) { design::ON_ACCENT } else { design::FOCUS })); let old_pen = SelectObject(dc, pen); let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
            RoundRect(dc, area.left+3, area.top+3, area.right-3, area.bottom-3, 8, 8);
            SelectObject(dc, old_brush); SelectObject(dc, old_pen); DeleteObject(pen);
        }
        (foreground, if pressed { 1 } else { 0 })
    }
    unsafe fn button(&mut self, dc: HDC, label: &str, area: RECT, action: Action, selected: bool) {
        let mut area = area;
        let height = design::control_height();
        area.top += ((area.bottom-area.top)-height)/2; area.bottom = area.top+height;
        let (foreground, offset) = self.surface(dc, area, &action, selected);
        if matches!(action, Action::Enable(_) | Action::LauncherTray) {
            let track = rect(area.right-36, (area.top+area.bottom-16)/2, area.right-8, (area.top+area.bottom+16)/2);
            let color = if selected { design::ACCENT } else { design::CONTROL_BORDER };
            rounded(dc, track, color, color);
            let x = if selected { track.right-13 } else { track.left+3 };
            let brush = CreateSolidBrush(design::resolve(if selected { design::ON_ACCENT } else { CARD }));
            let old = SelectObject(dc, brush); let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
            Ellipse(dc,x,track.top+3,x+10,track.bottom-3);
            SelectObject(dc,old_pen);SelectObject(dc,old);DeleteObject(brush);
            text_format(dc, label.trim_start_matches(['✓', ' ']),rect(area.left+8,area.top,track.left-4,area.bottom),FONT_CAPTION,foreground,false,DT_SINGLELINE|DT_VCENTER|DT_LEFT|DT_END_ELLIPSIS);
            self.hits.push(Hit { rect: area, action, label: label.into() }); return;
        }
        if matches!(action, Action::TrayMode(_)) {
            crate::icons::draw(dc,if selected { "chevron-down" } else { "monitor" },area.left+8,(area.top+area.bottom-14)/2,14,foreground);
            text_format(dc,label,rect(area.left+27,area.top,area.right-6,area.bottom),FONT_CAPTION,foreground,false,DT_SINGLELINE|DT_VCENTER|DT_LEFT);
            self.hits.push(Hit { rect:area,action,label:label.into() });return;
        }
        let dropdown = area.right-area.left >= 100 && (matches!(action, Action::ProfileMenu | Action::ToolsMenu | Action::ScheduleTools | Action::ThemeMenu | Action::WeekMenu | Action::CategoryMenu | Action::LicenseMenu | Action::SeriesJump));
        let icon = match action { Action::Page(0) => Some("layout-grid"), Action::Page(1) => Some("monitor"), Action::Page(2) => Some("arrow-up-right"), Action::Start => Some("play"), Action::Exit => Some("square"), Action::FavoritesOnly(true) => Some("star"), _ => None }.filter(|_| area.right-area.left >= 120);
        let left_aligned = dropdown || icon.is_some() || matches!(action, Action::AppDetails(_));
        let label = label.trim_start_matches(['★', '▶', '■', '✓', ' ']).trim_end_matches(['↗', ' ']);
        if let Some(icon) = icon { crate::icons::draw(dc, icon, area.left+12+offset, (area.top+area.bottom-16)/2+offset, 16, foreground); }
        let inset = if area.right-area.left < 80 { 8 } else { 12 };
        text_format(dc, label, rect(area.left + if icon.is_some() { 36 } else { inset } + offset, area.top + offset, area.right - if dropdown { 32 } else { inset } + offset, area.bottom + offset), if matches!(action,Action::Browse(_) | Action::Restore(_) | Action::Hide(_)) { FONT_CAPTION } else { FONT_BODY }, foreground, matches!(action, Action::Start | Action::RetryStop | Action::AppDetails(_)), DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | if left_aligned { DT_LEFT } else { DT_CENTER });
        if dropdown {
            crate::icons::draw(dc, "chevron-down", area.right-24+offset, (area.top+area.bottom-16)/2+offset, 16, foreground);
        }
        self.hits.push(Hit { rect: area, action, label: label.into() });
    }
    fn redraw(window: HWND) { if !window.is_null() { unsafe { InvalidateRect(window, null(), 0); } } }
    fn report(&mut self, result: Result<(), String>, success: &str) { self.message = match result { Ok(()) => success.into(), Err(error) => error }; }
    fn refresh_schedule(&mut self) {
        if self.refresh.is_some() { return; }
        self.message = "Refreshing schedule…".into();
        let root = self.store.root.clone(); let previous = self.schedule.clone();
        let (sender, receiver) = mpsc::channel(); self.refresh = Some(receiver);
        thread::spawn(move || { let result = season_schedule::refresh(&root, previous.as_ref()); let _ = sender.send(result); });
    }
    unsafe fn hide(&mut self, window: HWND) {
        if !self.tray_ready { self.register_tray(window); }
        if self.tray_ready { ShowWindow(window, SW_HIDE); } else { self.message = "Tray unavailable. Keeping Paddock visible. Try again shortly.".into(); }
    }
    unsafe fn register_tray(&mut self, window: HWND) {
        self.input.tray_attempts = self.input.tray_attempts.saturating_add(1);
        SetLastError(0); self.tray_ready = notify(window, NIM_ADD);
        if !self.tray_ready { app_control::log(&self.store.root, &format!("Tray registration failed · Attempt {} · Win32 {}", self.input.tray_attempts, GetLastError())); }
    }
    unsafe fn restore_launcher(&mut self, window: HWND) {
        self.auto_tray_done = true; ShowWindow(window, SW_RESTORE); SetForegroundWindow(window);
    }
    fn save_app_config(&mut self, previous: storage::Config) {
        let result = self.store.save_config();
        if result.is_err() { self.store.config = previous; }
        self.report(result.map_err(|issue| format!("Save failed. Previous preferences kept. Check folder permissions: {issue}")), "Saved");
    }
    unsafe fn complete_session(&mut self, window: HWND) {
        if self.controller.phase != Phase::Finished { return; }
        self.controller.reset_finished();
        self.auto_tray_done = false;
        self.message = "Apps stopped".into();
        ShowWindow(window, SW_RESTORE);
    }
    unsafe fn action(&mut self, window: HWND, action: Action) {
        if !self.enabled(&action) { return; }
        if !matches!(action, Action::Start) && matches!(self.controller.phase, Phase::Starting | Phase::Running) { self.auto_tray_done = true; }
        match action {
            Action::Hide(index) => if let Some(spec) = self.store.config.apps.get(index).cloned() { self.controller.hide_one(&spec); self.message = self.controller.hide_status(&spec.id).into(); },
            Action::ProfileMenu | Action::ProfileSelect(_) | Action::ProfileNew | Action::ProfileDuplicate | Action::ProfileRename | Action::AppAdd | Action::AppRename(_) | Action::AppRemove(_) | Action::AppDetails(_) => self.profile_action(window,action),
            Action::LearnOpen(index)=>if let Some((_,_,url))=profiles_ui::LESSONS.get(index){ShellExecuteW(window,wide("open").as_ptr(),wide(url).as_ptr(),null(),null(),SW_SHOWNORMAL);},
            Action::SavedInfo(number) => if let Some(schedule) = &self.schedule {
                let body = format!("Week {} · {} planned\n\n{}", number, self.store.plans.week(&schedule.key(number)).len(), self.store.plans.week(&schedule.key(number)).iter().map(|plan| format!("{}\n{}", schedule.plan_name(plan), plan.track)).collect::<Vec<_>>().join("\n\n"));
                dialogs::details(window, "Saved schedule", &body);
            },
            Action::ToolsMenu | Action::ScheduleTools => self.compact_menu(window, action),
            Action::Inspect(body) => dialogs::details(window, "Search results · Read only", &body),
            Action::MessageDetails => dialogs::details(window, "Notice", &self.message),
            Action::Locate(number, index) => {
                self.planner_action(window, Action::PlannerMode(crate::planner::Mode::Catalog));
                if self.schedule_view.weekly || self.schedule_view.favorites_only { return; }
                if let Some(schedule) = &self.schedule {
                    self.schedule_view.week = Some(number);
                    self.schedule_view.favorites_only = false; self.schedule_view.filter = schedule_ui::Filter::All;
                    self.scroll = self.schedule_view.scroll_to(schedule, &self.store.config.licenses, index);
                    self.input.focused = Some(Action::SeriesInfo(index)); self.input.focus_visible = true;
                    self.input.locate_after_draw = Some(Action::SeriesInfo(index));
                }
            }
            Action::Page(page) => { self.input.page_scroll[self.page as usize] = self.scroll; self.page = page; self.scroll = self.input.page_scroll[page as usize]; }
            Action::Week(number) => { self.schedule_view.week = Some(number); if self.schedule_view.weekly { self.scroll = 0; self.schedule_view.planner_card_scroll.clear(); } else { self.planner_action(window, Action::PlannerMode(crate::planner::Mode::Season)); if self.schedule_view.favorites_only { self.scroll = self.schedule_view.week_offset(number); self.input.locate_after_draw = Some(Action::SavedInfo(number)); } } }
            Action::Filter(filter) => { self.switch_schedule_filter(filter, self.schedule_view.favorites_only); }
            Action::ConditionalSeries | Action::WeekMenu | Action::SeriesJump | Action::CategoryMenu | Action::LicenseMenu | Action::License(_, _) | Action::SeriesInfo(_) | Action::Favorite(_) | Action::FavoritesOnly(_) => self.schedule_action(window, action),
            Action::PlannerCategoryToggle(_) | Action::PlannerHidden | Action::PlannerHide(_) | Action::PlannerCollapse(_) | Action::PlannerQueryClear | Action::PlannerMode(_) | Action::PlannerClass(_) | Action::PlannerBeginner | Action::PlannerReset | Action::PlannerFavorites | Action::PlannerSort | Action::PlannerCategory | Action::PlannerSearch | Action::PlannerWeek(_) | Action::PlannerToday | Action::PlannerFilters => self.planner_action(window, action),
            Action::Select(number, series) => {
                if let Some(schedule) = &self.schedule {
                    if let Some(week) = schedule.weeks.iter().find(|week| week.number == number) {
                        let key = schedule.key(number); let previous = self.store.plans.clone();
                        let track = schedule.track(number, series).unwrap().to_string();
                        self.schedule_view.week = Some(number);
                        self.store.plans.remember_week(&key);
                        let mut plans: Vec<_> = self.store.plans.week(&key).into_iter().cloned().collect();
                        if let Some(position) = plans.iter().position(|plan| schedule.plan_index(plan) == Some(series)) { if plans[position].track == track { plans.remove(position); } else { plans[position].track = track; } }
                        else { plans.push(Plan { season: schedule.season.clone(), week: number, date: week.start.clone(), series, series_id: schedule.catalog.get(series).map(|series| series.id.clone()).unwrap_or_default(), series_name: schedule.catalog.get(series).map(|series| series.name.clone()).unwrap_or_default(), track }); }
                        self.store.plans.set_week(&key, plans);
                        let result = self.store.save_plans();
                        if result.is_err() { self.store.plans = previous; }
                        self.report(result, "Saved");
                    }
                }
            }
            Action::Remove(key) => {
                let previous = self.store.plans.clone();
                self.store.plans.remember_week(&key);
                self.store.plans.set_week(&key, vec![]);
                let result = self.store.save_plans(); if result.is_err() { self.store.plans = previous; }
                self.report(result, "Selection cleared · Undo is available");
            }
            Action::UndoPlan(key) => {
                let previous = self.store.plans.clone();
                self.store.plans.undo(&key);
                let result = self.store.save_plans(); if result.is_err() { self.store.plans = previous; }
                self.report(result, "Selection restored");
            }
            Action::ThemeMenu => {
                let options: Vec<_> = design::Theme::ALL.iter().map(|theme| theme.label().into()).collect();
                if let Some(index) = self.schedule_menu(window, &Action::ThemeMenu, &options, design::Theme::ALL.iter().position(|theme| *theme == self.store.config.appearance.theme)) { self.action(window, Action::Theme(design::Theme::ALL[index])); }
            }
            Action::Theme(theme) => { let previous = self.store.config.appearance; self.store.config.appearance.theme = theme; self.save_appearance(window, previous); }
            Action::Density(density) => { let previous = self.store.config.appearance; self.store.config.appearance.density = density; self.save_appearance(window, previous); }
            Action::Refresh => self.refresh_schedule(),
            Action::Start => {
                self.auto_tray_done = false;
                let result = self.controller.start(&self.store.root, &self.store.config);
                self.report(result, "Launching apps…");
            }
            Action::Tray => self.hide(window),
            Action::RetryStop => { self.controller.retry_stop(&self.store.root); self.message = "Retrying stop…".into(); }
            Action::Restore(index) => if let Some(spec) = self.store.config.apps.get(index).cloned() {
                let count = self.controller.restore(&spec);
                self.message = if count > 0 { String::new() } else { format!("{}: Open this app from its tray icon.", spec.name) };
            },
            Action::HideAll => {
                for spec in self.store.config.apps.iter().filter(|spec| spec.enabled && spec.id != "iracing") { self.controller.hide_one(spec); } self.message = "App windows hidden".into();
            }
            Action::Enable(index) => if self.controller.phase == Phase::Idle {
                let previous = self.store.config.clone();
                self.store.config.apps[index].enabled = !self.store.config.apps[index].enabled;
                self.save_app_config(previous);
            },
            Action::TrayMode(index) => if self.controller.phase == Phase::Idle && self.store.config.apps[index].id != "iracing" {
                let previous = self.store.config.clone();
                self.store.config.apps[index].tray = !self.store.config.apps[index].tray;
                self.save_app_config(previous);
            },
            Action::Browse(index) => if self.controller.phase == Phase::Idle {
                if let Some(path) = choose_exe(window, &self.store.config.apps[index].path) {
                    let previous = self.store.config.clone();
                    let spec = &mut self.store.config.apps[index]; spec.path = path.clone(); spec.working_dir = Path::new(&path).parent().unwrap().to_string_lossy().into_owned();
                    spec.extra_paths.clear();
                    if spec.id == "garage61" { spec.extra_paths.push(format!("{}\\garage61-agent.exe", spec.working_dir)); }
                    spec.add_known_processes();
                    self.save_app_config(previous);
                }
            },
            Action::LauncherTray => { let previous = self.store.config.clone(); self.store.config.launcher_to_tray = !self.store.config.launcher_to_tray; self.save_app_config(previous); }
            Action::Source => { ShellExecuteW(window, wide("open").as_ptr(), wide(season_schedule::SOURCE).as_ptr(), null(), null(), SW_SHOWNORMAL); }
            Action::Data => { ShellExecuteW(window, wide("open").as_ptr(), wide(&self.store.root.to_string_lossy()).as_ptr(), null(), null(), SW_SHOWNORMAL); }
            Action::Exit => {
                let running = app_control::processes();
                if running.iter().any(|process| Path::new(&process.path).file_name().map(|name| name.to_string_lossy().to_lowercase().starts_with("iracingsim")).unwrap_or(false)) {
                    self.message = "Simulator running · Finish driving before stopping all apps".into(); ShowWindow(window, SW_RESTORE);
                } else if self.controller.phase != Phase::Stopping {
                    if self.controller.phase == Phase::CleanupFailed { self.controller.retry_stop(&self.store.root); }
                    else if self.controller.phase == Phase::Starting || running.iter().any(|process| self.store.config.apps.iter().any(|spec| spec.enabled && spec.matches(&process.path))) { self.controller.stop_all(&self.store.root, &self.store.config); self.message = "Stopping apps…".into(); }
                    else { self.controller=Controller::new();self.message = "No managed apps running".into(); }
                }
            }
        }
        Self::redraw(window);
    }
    unsafe fn draw(&mut self, dc: HDC, width: i32, height: i32) {
        self.input.reflow = width < 1000 || height < 600;
        if self.input.reflow { self.draw_reflow(dc, width, height); return; }
        design::set(self.store.config.appearance);
        READ_TEXT.with(|reading| reading.borrow_mut().clear());
        let layout = if self.page == PAGE_SCHEDULE && self.schedule_view.weekly { Layout::planner(width, height) } else { Layout::new(width, height) }; let left = layout.left; let right = layout.right;
        self.hits.clear(); fill(dc, rect(0, 0, width, height), BG); fill(dc, rect(0, 0, SIDEBAR_WIDTH, height), SIDE);
        rounded(dc, rect(24, 24, 56, 56), design::ACCENT, design::ACCENT);
        text_format(dc, "P", rect(24,24,56,56), 22, design::ON_ACCENT, true, DT_CENTER|DT_VCENTER|DT_SINGLELINE);
        text(dc, "Paddock", rect(68, 24, SIDEBAR_WIDTH-16, 56), FONT_TITLE, TEXT, true, false);
        text(dc, "iRacing Companion", rect(24, 64, 172, 84), FONT_CAPTION, MUTED, false, false);
        for (index, label) in PAGE_TITLES.iter().enumerate() {
            self.button(dc, label, rect(16, 112 + index as i32 * 48, SIDEBAR_WIDTH-16, 152 + index as i32 * 48), Action::Page(index as u8), self.page == index as u8);
        }
        text(dc, "WORKSPACE", rect(24, 88, SIDEBAR_WIDTH-16, 106), 11, MUTED, false, false);
        text(dc,"PROFILE",rect(24,268,SIDEBAR_WIDTH-16,288),11,MUTED,false,false);
        self.button(dc,&self.store.config.profile_name().to_string(),rect(16,296,SIDEBAR_WIDTH-16,332),Action::ProfileMenu,false);
        let (status, status_color) = match self.controller.phase {
            Phase::Starting => ("◌  Launching", WARNING),
            Phase::Running => ("●  Running", POSITIVE),
            Phase::Stopping => ("◌  Stopping", WARNING),
            Phase::CleanupFailed => ("!  Stop failed", DANGER),
            _ => ("○  Ready", MUTED),
        };
        fill(dc, rect(16, height-216, SIDEBAR_WIDTH-16, height-215), LINE);
        crate::icons::draw(dc, "circle", 24, height-198, 16, status_color);
        text(dc, status.trim_start_matches(['◌', '●', '!', '○', ' ']), rect(48, height-204, SIDEBAR_WIDTH-16, height-176), FONT_BODY, status_color, false, false);
        text(dc, concat!("v", env!("CARGO_PKG_VERSION")), rect(24, height-48, 172, height-24), FONT_CAPTION, MUTED, false, false);
        text(dc, match self.page { PAGE_SCHEDULE=>"Season planner", PAGE_APPS=>"App manager", _=>"Learn" }, rect(left, 24, right-284, 68), FONT_TITLE, TEXT, true, false);
        let failed = self.controller.phase == Phase::CleanupFailed;
        self.button(dc, if failed { "Retry stop" } else if self.controller.phase == Phase::Starting { "Launching…" } else { "Launch all" }, rect(16, height-164, SIDEBAR_WIDTH-16, height-124), if failed { Action::RetryStop } else { Action::Start }, false);
        self.button(dc, if self.controller.phase == Phase::Stopping { "Stopping…" } else { "Stop all" }, rect(16, height-116, SIDEBAR_WIDTH-16, height-80), Action::Exit, false);
        if self.page == PAGE_SCHEDULE {
            let season = self.schedule.as_ref().map(|schedule| format!("{}  ·  {} weeks  ·  {} planned", schedule.season, schedule.weeks.len(), schedule.weeks.iter().map(|week| self.store.plans.week(&schedule.key(week.number)).len()).sum::<usize>())).unwrap_or_else(|| "No race".into());
            text(dc, &season, rect(left, 76, left+284, 100), FONT_BODY, TEXT, false, false);
            self.button(dc, "Official PDF", rect(right-276, 28, right-152, 64), Action::Source, false);
            self.button(dc, if self.refresh.is_some() { "Refreshing…" } else { "Refresh schedule" }, rect(right-144, 28, right, 64), Action::Refresh, false);
            let caption = self.schedule.as_ref().map(|schedule| format!("Updated {}{}  ·  Resets Tue 09:00 KST", &schedule.fetched_at[..10.min(schedule.fetched_at.len())], if schedule.is_current() { "" } else { " · Refresh needed" })).unwrap_or_default();
            text_format(dc, &caption, rect(left+300, 76, right, 100), FONT_CAPTION, MUTED, false, DT_SINGLELINE | DT_VCENTER | DT_RIGHT | DT_END_ELLIPSIS);
        } else if self.page==PAGE_APPS {
            let included = self.store.config.apps.iter().filter(|spec| spec.enabled).count();
            let running = self.store.config.apps.iter().filter(|spec| self.controller.status.get(&spec.id).copied().unwrap_or(0) > 0).count();
            text(dc, &format!("{} included  ·  {} running  ·  Manual control", included, running), rect(left, 76, right, 100), FONT_BODY, MUTED, false, false);
            self.button(dc, "Data folder", rect(right-128, 28, right, 64), Action::Data, false);
        }
        if self.page==PAGE_LEARN {text(dc,"Driving technique, courses and training tools",rect(left,76,right,100),FONT_BODY,MUTED,false,false);}
        fill(dc, rect(left, 111, right, 112), LINE);
        if self.page == PAGE_SCHEDULE { self.measure_schedule_rows(dc, &layout); }
        let (scroll_top, scroll_content) = self.scroll_metrics(width, height);
        let available = layout.bottom-scroll_top;
        self.scroll = self.scroll.clamp(0, (scroll_content-available).max(0));
        let fixed_hits = self.hits.len();
        let clip = SaveDC(dc); IntersectClipRect(dc, left, CONTENT_TOP, right+1, layout.bottom);
        match self.page {
            PAGE_SCHEDULE => self.draw_schedule(dc, &layout),
            PAGE_APPS => self.draw_apps(dc, &layout),
            PAGE_LEARN => self.draw_learn(dc,&layout),
            _ => {},
        }
        RestoreDC(dc, clip);
        self.input.scrollbar = None;
        if available > 0 && scroll_content > available {
            let thumb_height = (available*available/scroll_content).max(35).min(available);
            let offset = self.scroll*(available-thumb_height)/(scroll_content-available);
            let track = rect(right+2, scroll_top, right+18, scroll_top+available);
            let thumb = rect(right+2, scroll_top+offset, right+18, scroll_top+offset+thumb_height);
            self.input.scrollbar = Some((track, thumb, scroll_content-available));
            fill(dc, track, LINE);
            fill(dc, thumb, MUTED);
        }
        for hit in &mut self.hits[fixed_hits..] { hit.rect.top = hit.rect.top.max(CONTENT_TOP); hit.rect.bottom = hit.rect.bottom.min(layout.bottom); }
        self.hits.retain(|hit| hit.rect.top < hit.rect.bottom);
        if !self.message.is_empty() {
            let failed = self.message.contains("failed") || self.message.contains("unable");
            fill(dc, rect(left, height-44, right, height-4), if failed { design::DANGER_SURFACE } else { design::INFO_SURFACE });
            text(dc, &self.message, rect(left+8, height-42, right-112, height-6), FONT_BODY, if failed { DANGER } else { design::INFO_TEXT }, false, true);
            self.button(dc, "Details", rect(right-104, height-42, right, height-6), Action::MessageDetails, false);
        }
        self.input.reading = READ_TEXT.with(|reading| reading.borrow().join("\n"));
    }
    unsafe fn draw_apps(&mut self, dc: HDC, layout: &Layout) {
        let left = layout.left; let right = layout.right;
        self.button(dc, "Auto-hide Paddock", rect(left, CONTENT_TOP, left+176, CONTENT_TOP+36), Action::LauncherTray, self.store.config.launcher_to_tray);
        self.button(dc, "Hide app windows", rect(left+188, CONTENT_TOP, left+354, CONTENT_TOP+36), Action::HideAll, false);
        self.button(dc, &format!("Theme: {}", self.store.config.appearance.theme.label()), rect(left+370, CONTENT_TOP, left+550, CONTENT_TOP+36), Action::ThemeMenu, false);
        self.button(dc,"+ Add app",rect(right-124,CONTENT_TOP,right,CONTENT_TOP+36),Action::AppAdd,false);
        text(dc, "APPLICATION", rect(left+16, 176, right-472, 208), FONT_CAPTION, MUTED, false, false);
        for (label, start, end) in [("STATUS", 458, 350), ("INCLUDED", 339, 258), ("ON LAUNCH", 249, 154), ("CONTROLS", 145, 12)] {
            text_format(dc, label, rect(right-start, 176, right-end, 208), FONT_CAPTION, MUTED, false, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
        }
        let clip = SaveDC(dc); IntersectClipRect(dc, left, TABLE_TOP, right, layout.bottom);
        let header_hits = self.hits.len();
        let apps = self.store.config.apps.clone();
        for (index, spec) in apps.iter().enumerate() {
            let top = TABLE_TOP + index as i32 * app_pitch() - self.scroll;
            if top+app_pitch() < TABLE_TOP || top >= layout.bottom { continue; }
            let count = self.controller.status.get(&spec.id).copied().unwrap_or(0);
            let exists = Path::new(&spec.path).is_file();
            rounded(dc, rect(left, top, right, top+72), CARD, LINE);
            self.button(dc, &spec.name, rect(left+8, top+4, right-472, top+36), Action::AppDetails(index), false);
            let (status, foreground, background) = if count>0 { ("● Running", POSITIVE, design::SUCCESS_SURFACE) } else if !exists { ("! Check path", WARNING, design::WARNING_SURFACE) } else { ("○ Stopped", MUTED, SIDE) };
            rounded(dc, rect(right-458, top+23, right-350, top+49), background, background);
            text_format(dc, status, rect(right-458, top+23, right-350, top+49), FONT_CAPTION, foreground, false, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
            text_format(dc, &spec.path, rect(left+16, top+36, right-472, top+62), FONT_CAPTION, MUTED, false, DT_SINGLELINE | DT_VCENTER | DT_PATH_ELLIPSIS);
            if spec.id == "iracing" { self.button(dc, "Open", rect(right-74, top+21, right-12, top+51), Action::Restore(index), false); }
            else {
                self.button(dc, "Open", rect(right-74, top+2, right-12, top+34), Action::Restore(index), false);
                self.button(dc, "Hide", rect(right-74, top+38, right-12, top+70), Action::Hide(index), false);
            }
            self.button(dc, "Path", rect(right-145, top+21, right-83, top+51), Action::Browse(index), false);
            self.button(dc,"Include",rect(right-347,top+21,right-250,top+51),Action::Enable(index),spec.enabled);
            if spec.id != "iracing" {
                self.button(dc, if spec.tray { "Tray" } else { "Window" }, rect(right-249, top+21, right-154, top+51), Action::TrayMode(index), spec.tray);

            } else {

                text_format(dc, "Window", rect(right-249, top+21, right-154, top+51), FONT_BODY, MUTED, false, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
            }
        }
        for hit in &mut self.hits[header_hits..] { hit.rect.top = hit.rect.top.max(TABLE_TOP); }
        RestoreDC(dc, clip);
    }
    unsafe fn tray_menu(&mut self, window: HWND) {
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING, 1, wide("Open Paddock").as_ptr());
        AppendMenuW(menu, MF_STRING | if self.enabled(&Action::HideAll) { 0 } else { MF_GRAYED }, 2, wide("Hide app windows").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        for (index, spec) in self.store.config.apps.iter().enumerate() { AppendMenuW(menu, MF_STRING, 100+index, wide(&format!("Open {}", spec.name)).as_ptr()); }
        for (index, spec) in self.store.config.apps.iter().enumerate().filter(|(_, spec)| spec.id != "iracing") { AppendMenuW(menu, MF_STRING, 200+index, wide(&format!("Hide {}", spec.name)).as_ptr()); }
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, 4, wide("Stop all managed apps").as_ptr());
        AppendMenuW(menu, MF_STRING | if self.controller.phase == Phase::Idle { 0 } else { MF_GRAYED }, 5, wide("Quit Paddock").as_ptr());
        let mut point: POINT = std::mem::zeroed(); GetCursorPos(&mut point); SetForegroundWindow(window);
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, point.x, point.y, 0, window, null());
        DestroyMenu(menu); PostMessageW(window, WM_NULL, 0, 0);
        match chosen { 1 => self.restore_launcher(window), 2 => self.action(window, Action::HideAll), 4 => self.action(window, Action::Exit), 5 if self.controller.phase == Phase::Idle => { DestroyWindow(window); }, value if value>=200 => self.action(window, Action::Hide((value-200) as usize)), value if value>=100 => self.action(window, Action::Restore((value-100) as usize)), _ => {} }
    }
}

unsafe fn choose_exe(window: HWND, current: &str) -> Option<String> {
    let mut buffer = vec![0u16; 32768]; let value = wide(current); buffer[..value.len().min(32768)].copy_from_slice(&value[..value.len().min(32768)]);
    let filter = wide("Executable (*.exe)\0*.exe\0\0");
    let mut dialog: OPENFILENAMEW = std::mem::zeroed();
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32; dialog.hwndOwner = window; dialog.lpstrFilter = filter.as_ptr(); dialog.lpstrFile = buffer.as_mut_ptr(); dialog.nMaxFile = buffer.len() as u32;
    dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    if GetOpenFileNameW(&mut dialog) == 0 { None } else { Some(String::from_utf16_lossy(&buffer[..buffer.iter().position(|code| *code == 0).unwrap()])) }
}

unsafe fn notify(window: HWND, command: u32) -> bool {
    let mut icon: NOTIFYICONDATAW = std::mem::zeroed();
    icon.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32; icon.hWnd = window; icon.uID = 1;
    icon.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP; icon.uCallbackMessage = TRAY_MESSAGE;
    icon.hIcon = LoadIconW(GetModuleHandleW(null()), 1 as _);
    if icon.hIcon.is_null() { icon.hIcon = LoadIconW(null_mut(), IDI_APPLICATION); }
    let label = wide("Paddock · iRacing Companion"); icon.szTip[..label.len()].copy_from_slice(&label);
    Shell_NotifyIconW(command, &icon) != 0
}

unsafe extern "system" fn procedure(window: HWND, message: u32, parameter: WPARAM, data: LPARAM) -> LRESULT {
    if message == WM_NCCREATE { let creation = &*(data as *const CREATESTRUCTW); SetWindowLongPtrW(window, GWLP_USERDATA, creation.lpCreateParams as isize); }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut State;
    if pointer.is_null() { return DefWindowProcW(window, message, parameter, data); }
    if message == WM_LBUTTONDOWN && IsWindowEnabled(window) != 0 && GetFocus() != window { SetFocus(window); }
    let state = &mut *pointer;
    match message {
        WM_GETOBJECT if data as i32 == OBJID_CLIENT => accessibility::get_object(window, parameter),
        accessibility::ACTION_MESSAGE => { if data != 0 { state.action(window, (*(data as *const Action)).clone()); } 0 }
        WM_SETFOCUS => { state.input.focus_visible = state.input.focused.is_some(); State::redraw(window); 0 }
        WM_NCDESTROY => { SetWindowLongPtrW(window, GWLP_USERDATA, 0); DefWindowProcW(window, message, parameter, data) }
        WM_CREATE => { if !state.fixture { state.register_tray(window); SetTimer(window, 1, 1000, None); } 0 }
        WM_COMMAND => {
            if let Some(query) = planner_search::changed(window, parameter, data) {
                if query != state.store.config.planner.query {
                    let previous = std::mem::replace(&mut state.store.config.planner.query, query);
                    if let Err(error) = state.store.save_config() { state.store.config.planner.query = previous; state.message = error; }
                    state.scroll = 0; State::redraw(window);
                }
                0
            } else { DefWindowProcW(window, message, parameter, data) }
        }
        WM_PAINT => {
            if !state.fixture { state.input.viewport.update(window); }
            let mut paint: PAINTSTRUCT = std::mem::zeroed(); let screen = BeginPaint(window, &mut paint);
            let mut area: RECT = std::mem::zeroed(); GetClientRect(window, &mut area);
            let memory = CreateCompatibleDC(screen); let bitmap = CreateCompatibleBitmap(screen, area.right.max(1), area.bottom.max(1)); let previous = SelectObject(memory, bitmap);
            let dpi = design::effective_dpi(GetDpiForWindow(window));
            logical_scale(memory, dpi);
            if state.fixture { fill(memory, area, CARD); text(memory, "Lifecycle fixture", rect(20,20,600,80), 25, TEXT, true, false); }
            else {
                SetViewportOrgEx(memory, -state.input.viewport.x*dpi/96, -state.input.viewport.y*dpi/96, null_mut());
                state.draw(memory, state.input.viewport.width(), state.input.viewport.height());
            }
            SetMapMode(memory, MM_TEXT); SetViewportOrgEx(memory, 0, 0, null_mut()); BitBlt(screen, 0, 0, area.right, area.bottom, memory, 0, 0, SRCCOPY);
            SelectObject(memory, previous); DeleteObject(bitmap); DeleteDC(memory); EndPaint(window, &paint);
            let search = if !state.fixture && state.page == PAGE_SCHEDULE && state.schedule_view.weekly && !state.input.reflow {
                    let mut area = planner_ui::search_area(&Layout::planner(state.input.viewport.width(), state.input.viewport.height()));
                    OffsetRect(&mut area, -state.input.viewport.x, -state.input.viewport.y); Some(area)
                } else { None };
            let query = state.store.config.planner.query.clone();
            let sync_search = !state.fixture;
            let hover_changed = state.sync_hover();
            // Native child operations can synchronously send owner messages.
            // Finish using the State borrow before entering them.
            if sync_search { planner_search::sync(window, search, &query); }
            if hover_changed { State::redraw(window); }
            0
        }
        WM_ERASEBKGND => 1,
        WM_SETTINGCHANGE | WM_THEMECHANGED => { design::refresh_system(); apply_window_theme(window); State::redraw(window); 0 }
        WM_IME_STARTCOMPOSITION => { state.input.ime_composing = true; DefWindowProcW(window, message, parameter, data) }
        WM_IME_ENDCOMPOSITION => { state.input.ime_composing = false; DefWindowProcW(window, message, parameter, data) }
        WM_SIZE => { State::redraw(window); 0 }
        RESTORE_MESSAGE => { state.restore_launcher(window); 0 }
        WM_ACTIVATE => { if parameter as u32 & 65535 != WA_INACTIVE && matches!(state.controller.phase, Phase::Starting | Phase::Running) { state.auto_tray_done = true; } DefWindowProcW(window, message, parameter, data) }
        WM_DPICHANGED => { let suggested = &*(data as *const RECT); SetWindowPos(window, null_mut(), suggested.left, suggested.top, suggested.right-suggested.left, suggested.bottom-suggested.top, SWP_NOZORDER | SWP_NOACTIVATE); 0 }
        WM_GETMINMAXINFO => { let information = &mut *(data as *mut MINMAXINFO); information.ptMinTrackSize = POINT { x: 320, y: 240 }; 0 }
        WM_HSCROLL | WM_VSCROLL => { state.input.viewport.scroll(window, message == WM_VSCROLL, parameter as u32 & 65535); state.input.pointer = None; State::redraw(window); 0 }
        WM_MOUSEMOVE => {
            if let Some(grab) = state.schedule_view.drag { state.drag_schedule_horizontal(mouse_point(window, data).0, grab); State::redraw(window); return 0; }
            if let (Some(grab), Some((track, thumb, maximum))) = (state.input.scroll_drag, state.input.scrollbar) {
                let point = mouse_point(window, data);
                state.scroll = ((point.1-track.top-grab)*maximum/(track.bottom-track.top-(thumb.bottom-thumb.top)).max(1)).clamp(0, maximum);
                State::redraw(window); return 0;
            }
            state.input.pointer = Some(mouse_point(window, data));
            let mut tracking = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: window, dwHoverTime: 0 }; TrackMouseEvent(&mut tracking);
            if state.sync_hover() { State::redraw(window); }
            SetCursor(LoadCursorW(null_mut(), if state.input.hovered.is_some() { IDC_HAND } else { IDC_ARROW })); 0
        }
        WM_MOUSELEAVE => { state.input.pointer = None; state.input.hovered = None; State::redraw(window); 0 }
        WM_SETCURSOR if data as u16 as u32 == HTCLIENT => {
            let mut point: POINT = std::mem::zeroed(); GetCursorPos(&mut point); ScreenToClient(window, &mut point);
            let dpi = design::effective_dpi(GetDpiForWindow(window));
            SetCursor(LoadCursorW(null_mut(), if state.hit_at((point.x*96/dpi+state.input.viewport.x, point.y*96/dpi+state.input.viewport.y)).is_some() { IDC_HAND } else { IDC_ARROW })); 1
        }
        WM_LBUTTONDOWN => {
            let point = mouse_point(window, data);
            if state.page == PAGE_SCHEDULE && !state.input.reflow {
                if let Some((track, thumb)) = state.schedule_view.bar {
                    if point.0 >= track.left && point.0 < track.right && point.1 >= track.top && point.1 < track.bottom {
                        let grab = if point.0 >= thumb.left && point.0 < thumb.right { point.0-thumb.left } else { (thumb.right-thumb.left)/2 };
                        state.schedule_view.drag = Some(grab); SetCapture(window); state.drag_schedule_horizontal(point.0, grab); State::redraw(window); return 0;
                    }
                }
            }
            if let Some((track, thumb, maximum)) = state.input.scrollbar {
                if point.0 >= track.left && point.0 < track.right && point.1 >= track.top && point.1 < track.bottom {
                    let grab = if point.1 >= thumb.top && point.1 < thumb.bottom { point.1-thumb.top } else { (thumb.bottom-thumb.top)/2 };
                    state.input.scroll_drag = Some(grab); SetCapture(window);
                    state.scroll = ((point.1-track.top-grab)*maximum/(track.bottom-track.top-(thumb.bottom-thumb.top)).max(1)).clamp(0, maximum);
                    State::redraw(window); return 0;
                }
            }
            state.input.pointer = Some(mouse_point(window, data)); state.sync_hover();
            state.input.pressed = state.input.hovered.clone(); state.input.focused = state.input.hovered.clone();
            state.input.focus_visible = false; state.input.keyboard_key = None;
            if state.input.pressed.is_some() { SetCapture(window); }
            State::redraw(window); 0
        }
        WM_LBUTTONUP => {
            if state.schedule_view.drag.take().is_some() { ReleaseCapture(); State::redraw(window); return 0; }
            if state.input.scroll_drag.take().is_some() { ReleaseCapture(); State::redraw(window); return 0; }
            state.input.pointer = Some(mouse_point(window, data)); state.sync_hover();
            let pressed = state.input.pressed.take();
            let action = pressed.filter(|action| state.input.hovered.as_ref() == Some(action) && state.enabled(action));
            if GetCapture() == window { ReleaseCapture(); }
            if let Some(action) = action { state.action(window, action); }
            State::redraw(window); 0
        }
        WM_CAPTURECHANGED | WM_CANCELMODE => { state.schedule_view.drag = None; state.input.scroll_drag = None; state.input.pressed = None; state.input.keyboard_key = None; if message == WM_CANCELMODE && GetCapture() == window { ReleaseCapture(); } State::redraw(window); 0 }
        WM_KILLFOCUS => { state.input.pressed = None; state.input.keyboard_key = None; state.input.focus_visible = false; if GetCapture() == window { ReleaseCapture(); } State::redraw(window); 0 }
        WM_MOUSEHWHEEL => { if state.page == PAGE_SCHEDULE && state.schedule_view.favorites_only && !state.input.reflow { state.schedule_horizontal(((parameter >> 16) as i16 as i32)/2); State::redraw(window); } 0 }
        WM_MOUSEWHEEL => {
            if state.page == PAGE_SCHEDULE && state.schedule_view.weekly && !state.input.reflow {
                let mut point = POINT { x: data as i16 as i32, y: (data >> 16) as i16 as i32 };
                ScreenToClient(window, &mut point); let dpi = design::effective_dpi(GetDpiForWindow(window));
                point.x = point.x*96/dpi+state.input.viewport.x; point.y = point.y*96/dpi+state.input.viewport.y;
                if let Some((_, id, maximum)) = state.schedule_view.planner_card_regions.iter().find(|(area, _, _)| PtInRect(area, point) != 0).cloned() {
                    let offset = state.schedule_view.planner_card_scroll.entry(id).or_default();
                    let next = (*offset-((parameter >> 16) as i16 as i32)/2).clamp(0, maximum);
                    if *offset != next { *offset = next; state.input.hovered = None; state.input.pressed = None; State::redraw(window); return 0; }
                }
            }
            if GetKeyState(VK_SHIFT as i32) < 0 && state.page == PAGE_SCHEDULE && state.schedule_view.favorites_only && !state.input.reflow { state.schedule_horizontal(-((parameter >> 16) as i16 as i32)/2); }
            else { state.scroll_by(window, -((parameter >> 16) as i16 as i32)/2); }
            State::redraw(window); 0
        }
        WM_KEYDOWN => {
            if state.input.ime_composing || GetKeyState(VK_MENU as i32) < 0 { return DefWindowProcW(window, message, parameter, data); }
            if GetKeyState(VK_CONTROL as i32) < 0 {
                if parameter as u32 == 0x46 || parameter as u32 == 0x4b { if state.page==PAGE_SCHEDULE { state.planner_action(window,Action::PlannerSearch); } }
                else if parameter as u32 == 0x5a {
                    if state.page == PAGE_SCHEDULE { if let Some(schedule) = &state.schedule { state.action(window, Action::UndoPlan(schedule.key(state.schedule_view.week_number(schedule)))); } }
                }
                return 0;
            }
            match parameter as u32 {
                0x09 => state.focus_next(GetKeyState(VK_SHIFT as i32) < 0),
                0x0d | 0x20 if data & (1 << 30) == 0 => {
                    if let Some(action) = state.input.focused.clone().filter(|action| state.enabled(action)) { state.input.pressed = Some(action); state.input.keyboard_key = Some(parameter as u32); state.input.focus_visible = true; }
                }
                0x31..=0x33 => { state.input.pressed = None; state.input.keyboard_key = None; state.action(window, Action::Page(parameter as u8-0x31)); },
                0x74 => state.refresh_schedule(),
                0x1b => { state.input.pressed = None; state.input.keyboard_key = None; if GetCapture() == window { ReleaseCapture(); } },
                0x25 | 0x27 if state.page == PAGE_SCHEDULE && state.schedule_view.favorites_only && !state.input.reflow => state.schedule_horizontal(if parameter == 0x25 { -state.schedule_view.column_width } else { state.schedule_view.column_width }),
                0x26 => state.scroll_by(window, -61), 0x28 => state.scroll_by(window, 61), _ => {}
            }
            if let Some(area) = state.hits.iter().find(|hit| Some(&hit.action) == state.input.focused.as_ref()).map(|hit| hit.rect) { state.input.viewport.reveal(window, area); }
            accessibility::announce(window, state); State::redraw(window); 0
        }
        WM_KEYUP => {
            if state.input.keyboard_key == Some(parameter as u32) {
                state.input.keyboard_key = None;
                let action = state.input.pressed.take().filter(|action| state.input.focused.as_ref() == Some(action) && state.enabled(action));
                if let Some(action) = action { state.action(window, action); }
                State::redraw(window);
            } 0
        }
        WM_TIMER => {
            if !state.tray_ready && state.input.tray_attempts < 5 && !state.fixture { state.register_tray(window); }
            let previous_phase = state.controller.phase;
            state.controller.tick(&state.store.root, &state.store.config.apps);
            if state.controller.phase != previous_phase {
                if state.controller.phase == Phase::CleanupFailed { state.message = "Stop failed · Check remaining apps or retry".into(); ShowWindow(window, SW_RESTORE); }
                else if state.controller.phase == Phase::Running { state.message = state.controller.messages.iter().filter(|message| message.contains("failed") || message.contains("unable")).cloned().collect::<Vec<_>>().join(" / "); }
            }
            if let Some(result) = state.refresh.as_ref().and_then(|receiver| receiver.try_recv().ok()) {
                state.refresh = None;
                match result { Ok(schedule) => { state.message = format!("{} updated", schedule.season); state.schedule = Some(schedule); }, Err(error) => { state.message = format!("Update failed · Previous schedule kept: {error}"); app_control::log(&state.store.root, &state.message); } }
                let _ = storage::atomic_bytes(&state.store.root.join("refresh-status.txt"), state.message.as_bytes());
            }
            if state.controller.phase == Phase::Running && state.store.config.apps.iter().any(|app|app.enabled&&state.controller.status.get(&app.id).copied().unwrap_or(0)>0) && state.store.config.launcher_to_tray && !state.auto_tray_done && IsWindowEnabled(window) != 0 && state.controller.start_time.elapsed().as_secs() >= 10 { state.hide(window); state.auto_tray_done = true; }
            state.complete_session(window);
            State::redraw(window); 0
        }
        TRAY_MESSAGE => { if data as u32 == WM_RBUTTONUP { state.tray_menu(window); } else if data as u32 == WM_LBUTTONUP || data as u32 == WM_LBUTTONDBLCLK { state.restore_launcher(window); } 0 }
        WM_QUERYENDSESSION => 1,
        WM_CLOSE => { if state.fixture { if !state.ignore_close { DestroyWindow(window); } } else if matches!(state.controller.phase, Phase::Starting | Phase::Running | Phase::Stopping | Phase::CleanupFailed) { state.hide(window); } else { DestroyWindow(window); } 0 }
        WM_DESTROY => { if !state.fixture { notify(window, NIM_DELETE); KillTimer(window, 1); } PostQuitMessage(0); 0 }
        _ => {
            if message == RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) && !state.fixture { state.input.tray_attempts = 0; state.register_tray(window); if !state.tray_ready { ShowWindow(window, SW_RESTORE); } }
            DefWindowProcW(window, message, parameter, data)
        }
    }
}

unsafe fn event_loop(mut state: Box<State>, title: &str) -> Result<(), String> {
    let _apartment = accessibility::initialize();
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let instance = GetModuleHandleW(null()); let class = wide("PaddockWindow");
    let mut description: WNDCLASSW = std::mem::zeroed(); description.hInstance = instance; description.lpszClassName = class.as_ptr(); description.lpfnWndProc = Some(procedure); description.hCursor = LoadCursorW(null_mut(), IDC_ARROW); description.hIcon = LoadIconW(instance, 1 as _);
    RegisterClassW(&description);
    let width = 1310.min(GetSystemMetrics(SM_CXSCREEN)-80); let height = 1000.min(GetSystemMetrics(SM_CYSCREEN)-100);
    let window = CreateWindowExW(0, class.as_ptr(), wide(title).as_ptr(), WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_HSCROLL | WS_VSCROLL, CW_USEDEFAULT, CW_USEDEFAULT, width, height, null_mut(), null_mut(), instance, &mut *state as *mut State as _);
    if window.is_null() { return Err(std::io::Error::last_os_error().to_string()); }
    design::refresh_system(); design::set(state.store.config.appearance); apply_window_theme(window);
    ShowWindow(window, SW_SHOW); ShowWindow(window, SW_RESTORE); UpdateWindow(window);
    if std::env::args().any(|argument| argument == "--verify-ui") {
        let mut report = vec![format!("Tray icon registered: {}", state.tray_ready)];
        report.push(format!("MSAA exposes named controls and readable page content: {}", accessibility::verify(window, state.hits.len())));
        report.push(format!("Navigation contains only schedule and app settings: {}", PAGE_TITLES == ["Planner", "Apps", "Learn"] && state.hits.iter().filter_map(|hit| if let Action::Page(page) = hit.action { Some(page) } else { None }).collect::<Vec<_>>() == [0, 1, 2]));
        SendMessageW(window, WM_KEYDOWN, 0x34, 0);
        SendMessageW(window, WM_KEYDOWN, 0x35, 0);
        state.action(window, Action::Page(3));
        state.action(window, Action::Page(4));
        report.push(format!("Removed fourth and fifth shortcuts and pages cannot activate: {}", state.page == PAGE_SCHEDULE));
        report.push(format!("Official PDF remains accessible on schedule: {}", state.hits.iter().any(|hit| hit.action == Action::Source)));
        let held_left_button = 1usize;
        let button_point = |state: &State, action: Action| -> LPARAM {
            let hit = state.hits.iter().find(|hit| hit.action == action).unwrap();
            let dpi = design::effective_dpi(GetDpiForWindow(window));
            let horizontal = (hit.rect.left+hit.rect.right)/2*dpi/96;
            let vertical = (hit.rect.top+hit.rect.bottom)/2*dpi/96;
            (horizontal as u16 as isize) | ((vertical as u16 as isize) << 16)
        };
        let apps_button = button_point(&state, Action::Page(1));
        SendMessageW(window, WM_MOUSEMOVE, 0, apps_button);
        report.push(format!("Hover identifies enabled button: {}", state.input.hovered == Some(Action::Page(1))));
        report.push(format!("Enabled button uses hand cursor: {}", GetCursor() == LoadCursorW(null_mut(), IDC_HAND)));
        SendMessageW(window, WM_LBUTTONDOWN, held_left_button, apps_button);
        report.push(format!("Press captures pointer and shows pressed state: {}", state.input.pressed == Some(Action::Page(1)) && GetCapture() == window));
        SendMessageW(window, WM_MOUSEMOVE, held_left_button, 5 | (5 << 16));
        SendMessageW(window, WM_LBUTTONUP, 0, 5 | (5 << 16));
        report.push(format!("Release outside cancels activation: {}", state.page == 0 && state.input.pressed.is_none() && GetCapture() != window));
        SendMessageW(window, WM_LBUTTONUP, 0, apps_button);
        report.push(format!("Release without prior press does not activate: {}", state.page == 0));
        SendMessageW(window, WM_LBUTTONDOWN, held_left_button, apps_button);
        SendMessageW(window, WM_LBUTTONUP, 0, apps_button);
        report.push(format!("Click navigates once: {}", state.page == 1));
        UpdateWindow(window);
        state.input.focused = None;
        SendMessageW(window, WM_KEYDOWN, VK_TAB as usize, 0);
        report.push(format!("Tab reveals keyboard focus: {}", state.input.focus_visible && state.input.focused == Some(Action::Page(0))));
        SendMessageW(window, WM_KEYDOWN, VK_RETURN as usize, 0);
        report.push(format!("Keyboard press waits for release: {}", state.page == 1 && state.input.pressed == Some(Action::Page(0))));
        SendMessageW(window, WM_KEYUP, VK_RETURN as usize, 0);
        report.push(format!("Enter activates focused button: {}", state.page == 0));
        UpdateWindow(window);
        SendMessageW(window, WM_KEYDOWN, VK_TAB as usize, 0);
        SendMessageW(window, WM_KEYDOWN, VK_SPACE as usize, 0);
        SendMessageW(window, WM_KEYUP, VK_SPACE as usize, 0);
        report.push(format!("Space activates focused button: {}", state.page == 1));
        state.focus_next(true);
        report.push(format!("Reverse tab order: {}", state.input.focused == Some(Action::Page(0))));
        state.action(window, Action::Page(0)); state.action(window, Action::FavoritesOnly(false));
        state.action(window, Action::Filter(schedule_ui::Filter::SportsCar)); UpdateWindow(window);
        report.push(format!("Catalog exposes category and license controls: {}", [Action::CategoryMenu, Action::LicenseMenu].iter().all(|action| state.hits.iter().any(|hit| &hit.action == action))));
        state.action(window, Action::Locate(5, 3)); UpdateWindow(window);
        let add_button = button_point(&state, Action::Favorite(3));
        SendMessageW(window, WM_LBUTTONDOWN, held_left_button, add_button); SendMessageW(window, WM_LBUTTONUP, 0, add_button); UpdateWindow(window);
        report.push(format!("Catalog card registers series without selecting any race: {}", state.selected(&Action::Favorite(3)) && state.store.plans.week("2026-S4:5").is_empty()));
        state.action(window, Action::Favorite(2));
        state.action(window, Action::FavoritesOnly(true)); state.action(window, Action::Week(5)); UpdateWindow(window);
        let select_button = button_point(&state, Action::Select(5, 3));
        SendMessageW(window, WM_LBUTTONDOWN, held_left_button, select_button); SendMessageW(window, WM_LBUTTONUP, 0, select_button); UpdateWindow(window);
        report.push(format!("Circuit click selects series in the correct week: {}", state.selected(&Action::Select(5, 3))));
        let second = button_point(&state, Action::Select(5, 2));
        SendMessageW(window, WM_LBUTTONDOWN, held_left_button, second); SendMessageW(window, WM_LBUTTONUP, 0, second); UpdateWindow(window);
        report.push(format!("Two series coexist in one week and persist: {}", Store::open(state.store.root.clone())?.plans.week("2026-S4:5").len() == 2));
        SendMessageW(window, WM_LBUTTONDOWN, held_left_button, second); SendMessageW(window, WM_LBUTTONUP, 0, second); UpdateWindow(window);
        report.push(format!("Unchecking one race preserves the other: {}", state.store.plans.week("2026-S4:5").len() == 1 && state.selected(&Action::Select(5, 3))));
        state.action(window, Action::UndoPlan("2026-S4:5".into())); UpdateWindow(window);
        report.push(format!("Undo restores the complete multi-selection set: {}", state.store.plans.week("2026-S4:5").len() == 2));
        state.action(window, Action::License(crate::race_catalog::Category::SportsCar, Some(crate::race_catalog::License::D))); UpdateWindow(window);
        let saved_licenses = Store::open(state.store.root.clone())?.config.licenses;
        report.push(format!("License persists separately for sports cars: {}", saved_licenses.get(&crate::race_catalog::Category::SportsCar) == Some(&crate::race_catalog::License::D) && !saved_licenses.contains_key(&crate::race_catalog::Category::Formula)));
        let condition = state.series_condition(&state.schedule.as_ref().unwrap().catalog[3]);
        report.push(format!("Conditional upper class is explicitly labelled: {}", condition.starts_with("Conditional eligibility") && condition.contains("4.0")));
        state.action(window, Action::Filter(schedule_ui::Filter::Formula)); UpdateWindow(window);
        report.push(format!("Formula catalog filtering does not erase saved races: {}", state.schedule_view.groups(state.schedule.as_ref().unwrap(), &state.store.config.licenses)[0].1.len() > 4 && state.store.plans.week("2026-S4:5").len() == 2));
        state.action(window, Action::Week(6)); UpdateWindow(window);
        report.push(format!("Date navigation jumps to a week row instead of a tile strip: {}", state.hits.iter().any(|hit| hit.action == Action::SavedInfo(6)) && state.hits.iter().any(|hit| hit.action == Action::WeekMenu)));
        state.action(window, Action::Select(6, 2)); UpdateWindow(window);
        report.push(format!("Independent week selection preserves both earlier races: {}", state.store.plans.week("2026-S4:5").len() == 2 && state.store.plans.week("2026-S4:6").len() == 1));
        let saved_plans = std::fs::read(state.store.root.join("plans.json")).map_err(|error| error.to_string())?;
        let mut shortcuts_match = true;
        for (key, page) in [(0x32, PAGE_APPS), (0x31, PAGE_SCHEDULE)] {
            SendMessageW(window, WM_KEYDOWN, key, 0); UpdateWindow(window); shortcuts_match &= state.page == page;
        }
        report.push(format!("Number shortcuts retain two pages: {shortcuts_match}"));
        report.push(format!("Page switches never rewrite plan data: {}", std::fs::read(state.store.root.join("plans.json")).map_err(|error| error.to_string())? == saved_plans));
        let remove_button = button_point(&state, Action::Remove("2026-S4:6".into()));
        SendMessageW(window, WM_LBUTTONDOWN, held_left_button, remove_button); SendMessageW(window, WM_LBUTTONUP, 0, remove_button); UpdateWindow(window);
        report.push(format!("Clear week removes only its own selections: {}", state.store.plans.week("2026-S4:6").is_empty() && state.store.plans.week("2026-S4:5").len() == 2));
        state.action(window, Action::Week(12)); UpdateWindow(window);
        report.push(format!("Final week is directly reachable: {}", state.hits.iter().any(|hit| hit.action == Action::SavedInfo(12))));
        state.page = PAGE_APPS; state.controller.phase = Phase::Running;
        State::redraw(window); UpdateWindow(window);
        report.push(format!("Data folder remains accessible in settings: {}", state.hits.iter().any(|hit| hit.action == Action::Data)));
        let disabled_button = button_point(&state, Action::Browse(0));
        SendMessageW(window, WM_MOUSEMOVE, 0, disabled_button);
        SendMessageW(window, WM_LBUTTONDOWN, held_left_button, disabled_button);
        report.push(format!("Disabled button ignores hover and press: {}", state.input.hovered.is_none() && state.input.pressed.is_none()));
        SendMessageW(window, WM_LBUTTONUP, 0, disabled_button);
        let mut skips_disabled = true;
        for _ in 0..state.hits.len() { state.focus_next(false); skips_disabled &= state.input.focused.as_ref().map(|action| state.enabled(action)).unwrap_or(false); }
        report.push(format!("Keyboard navigation skips disabled controls: {skips_disabled}"));
        let schedule_button = button_point(&state, Action::Page(0));
        SendMessageW(window, WM_LBUTTONDOWN, held_left_button, schedule_button);
        SendMessageW(window, WM_CANCELMODE, 0, 0);
        SendMessageW(window, WM_LBUTTONUP, 0, schedule_button);
        report.push(format!("Cancelled capture cannot activate: {}", state.page == PAGE_APPS && state.input.pressed.is_none() && GetCapture() != window));
        SendMessageW(window, WM_LBUTTONDOWN, held_left_button, schedule_button);
        SendMessageW(window, WM_KILLFOCUS, 0, 0);
        SendMessageW(window, WM_LBUTTONUP, 0, schedule_button);
        report.push(format!("Focus loss cancels press and releases capture: {}", state.page == PAGE_APPS && state.input.pressed.is_none() && GetCapture() != window));
        SendMessageW(window, WM_MOUSELEAVE, 0, 0);
        report.push(format!("Pointer leave clears hover: {}", state.input.hovered.is_none()));
        state.controller = Controller::new();
        state.hide(window); report.push(format!("Window hidden: {}", IsWindowVisible(window) == 0));
        ShowWindow(window, SW_RESTORE); report.push(format!("Window restored: {}", IsWindowVisible(window) != 0));
        state.controller.phase = Phase::Finished;
        state.hide(window);
        SendMessageW(window, WM_TIMER, 1, 0);
        report.push(format!("Launcher survives cleanup and returns to Idle: {}", IsWindow(window) != 0 && IsWindowVisible(window) != 0 && state.controller.phase == Phase::Idle && state.tray_ready));
        state.store.config.apps.clear();
        state.action(window, Action::Exit);
        report.push(format!("Full stop with no apps keeps launcher open: {}", IsWindow(window) != 0 && state.controller.phase == Phase::Idle));
        state.action(window, Action::Theme(design::Theme::Dark)); state.action(window, Action::Density(design::Density::Compact));
        report.push(format!("Theme and density persist across reopening: {}", Store::open(state.store.root.clone())?.config.appearance == state.store.config.appearance));
        ShowWindow(window, SW_RESTORE); SetWindowPos(window, null_mut(), 0, 0, 640, 480, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE); State::redraw(window); UpdateWindow(window);
        let target = state.hits.iter().find(|hit| hit.action == Action::ThemeMenu).map(|hit| hit.rect).unwrap();
        state.input.viewport.reveal(window, target); State::redraw(window); UpdateWindow(window);
        report.push(format!("Small viewport reflows controls without horizontal panning or smaller text: {}", state.input.reflow && state.input.viewport.x == 0 && target.right <= state.input.viewport.width()));
        SendMessageW(window, WM_IME_STARTCOMPOSITION, 0, 0); SendMessageW(window, WM_KEYDOWN, 0x31, 0); SendMessageW(window, WM_IME_ENDCOMPOSITION, 0, 0);
        report.push(format!("IME composition suppresses page shortcuts: {}", state.page == PAGE_APPS && !state.input.ime_composing));
        report.extend(dialogs::verify(window));
        report.extend(query_dialog::verify(window));
        report.extend(planner_search::verify(window));
        storage::atomic_bytes(&state.store.root.join("ui-verification.txt"), report.join("\n").as_bytes())?;
        let passed = report.iter().all(|line| line.ends_with("true"));
        DestroyWindow(window);
        if !passed { return Err("Launcher window/tray verification failed".into()); }
    }
    let mut message: MSG = std::mem::zeroed();
    while GetMessageW(&mut message, null_mut(), 0, 0) > 0 { TranslateMessage(&message); DispatchMessageW(&message); }
    if !state.mutex.is_null() { ReleaseMutex(state.mutex); CloseHandle(state.mutex); }
    Ok(())
}

pub fn run(root: PathBuf) -> Result<(), String> {
    let store = Store::open(root)?;
    let schedule_result = season_schedule::cached(&store.root);
    let message = schedule_result.as_ref().err().cloned().or_else(|| store.warnings.first().cloned()).unwrap_or_default();
    unsafe {
        let mutex_name = if read_only_review() || std::env::args().any(|argument| argument == "--verify-ui") { format!("Local\\PaddockVerification-{}", std::process::id()) } else { "Local\\PaddockPersonalLauncher".into() };
        let mutex = CreateMutexW(null(), 1, wide(&mutex_name).as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mutex); let window = FindWindowW(wide("PaddockWindow").as_ptr(), wide("Paddock · iRacing Companion").as_ptr());
            if !window.is_null() { SendMessageW(window, RESTORE_MESSAGE, 0, 0); } return Ok(());
        }
        let mut state = State { schedule_view: Default::default(), store, schedule: schedule_result.ok(), controller: Controller::new(), page: 0, hits: vec![], scroll: 0, message, refresh: None, tray_ready: false, auto_tray_done: false, fixture: false, ignore_close: false, mutex, input: Interaction::default() };
        if !std::env::args().any(|argument| argument == "--verify-ui") { state.store.config.planner.mode=crate::planner::Mode::Week;state.apply_planner_mode(); }
        if state.store.config.favorite_series.is_none() {
            if let Some(schedule) = &state.schedule { state.store.config.favorite_series = Some(schedule_ui::favorite_ids(&state.store, schedule)); }
        }
        event_loop(Box::new(state), if read_only_review() { "Paddock · Read-only review" } else { "Paddock · iRacing Companion" })
    }
}

pub fn fixture(name: &str, ignore: bool) {
    let root = std::env::current_exe().unwrap().parent().unwrap().join("fixture-data");
    let store = Store::open(root).unwrap();
    let title = if name == "paddock-fixture-main.exe" { "iRacing UI · fixture" } else { name };
    let _ = unsafe { event_loop(Box::new(State { schedule_view: Default::default(), store, schedule: None, controller: Controller::new(), page: 0, hits: vec![], scroll: 0, message: String::new(), refresh: None, tray_ready: false, auto_tray_done: false, fixture: true, ignore_close: ignore, mutex: null_mut(), input: Interaction::default() }), title) };
}

unsafe fn render_bitmap(state: &mut State, path: &Path, width: i32, height: i32) -> Result<(), String> {
    render_bitmap_scaled(state, path, width, height, 96)
}

unsafe fn render_bitmap_scaled(state: &mut State, path: &Path, width: i32, height: i32, dpi: i32) -> Result<(), String> {
    let dc = CreateCompatibleDC(null_mut());
    let pixel_width = width*dpi/96; let pixel_height = height*dpi/96;
    let mut information: BITMAPINFO = std::mem::zeroed();
    information.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    information.bmiHeader.biWidth = pixel_width; information.bmiHeader.biHeight = -pixel_height;
    information.bmiHeader.biPlanes = 1; information.bmiHeader.biBitCount = 32;
    let mut pointer = null_mut();
    let bitmap = CreateDIBSection(dc, &information, DIB_RGB_COLORS, &mut pointer, null_mut(), 0);
    if bitmap.is_null() { DeleteDC(dc); return Err(std::io::Error::last_os_error().to_string()); }
    let previous = SelectObject(dc, bitmap);
    logical_scale(dc, dpi);
    state.draw(dc, width, height); GdiFlush();
    let pixels = std::slice::from_raw_parts(pointer as *const u8, (pixel_width*pixel_height*4) as usize);
    let mut bytes = vec![];
    bytes.extend_from_slice(b"BM"); bytes.extend_from_slice(&(54u32+pixels.len() as u32).to_le_bytes()); bytes.extend_from_slice(&0u32.to_le_bytes()); bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes()); bytes.extend_from_slice(&pixel_width.to_le_bytes()); bytes.extend_from_slice(&(-pixel_height).to_le_bytes()); bytes.extend_from_slice(&1u16.to_le_bytes()); bytes.extend_from_slice(&32u16.to_le_bytes()); bytes.extend_from_slice(&0u32.to_le_bytes()); bytes.extend_from_slice(&(pixels.len() as u32).to_le_bytes()); bytes.extend_from_slice(&[0u8;16]); bytes.extend_from_slice(pixels);
    SelectObject(dc, previous); DeleteObject(bitmap); DeleteDC(dc);
    storage::atomic_bytes(path, &bytes)
}

pub fn render_planner_preview(root: PathBuf, directory: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error|error.to_string())?;
    let store=Store::open(root.clone())?;let schedule=season_schedule::cached(&root)?;
    let mut state=State { schedule_view:Default::default(),store,schedule:Some(schedule),controller:Controller::new(),page:0,hits:vec![],scroll:0,message:String::new(),refresh:None,tray_ready:false,auto_tray_done:false,fixture:false,ignore_close:false,mutex:null_mut(),input:Interaction::default() };
    state.store.config.planner=crate::planner::Preferences::default();state.store.config.appearance.theme=design::Theme::Dark;state.apply_planner_mode();state.schedule_view.week=Some(9);
    unsafe {
        render_bitmap(&mut state,&directory.join("planner-week.bmp"),1360,1010)?;
        render_bitmap(&mut state,&directory.join("planner-wide.bmp"),1920,1080)?;
        render_bitmap(&mut state,&directory.join("planner-compact.bmp"),1034,700)?;
        render_bitmap(&mut state,&directory.join("planner-narrow.bmp"),640,480)?;
        render_bitmap_scaled(&mut state,&directory.join("planner-150.bmp"),1034,700,144)?;
        state.store.config.appearance.theme=design::Theme::Dark;
        render_bitmap(&mut state,&directory.join("planner-dark.bmp"),1360,1010)?;
        state.store.config.appearance.theme=design::Theme::Light;
        state.store.config.planner.query="GR86".into();
        render_bitmap(&mut state,&directory.join("planner-search.bmp"),1360,1010)?;
        state.store.config.planner.query="no-matching-race-987654".into();
        render_bitmap(&mut state,&directory.join("planner-empty.bmp"),1034,700)?;
        state.store.config.planner.mode=crate::planner::Mode::Season;state.apply_planner_mode();
        render_bitmap(&mut state,&directory.join("planner-season.bmp"),1360,1010)?;
    }
    Ok(())
}

pub fn render_preview(root: PathBuf, directory: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let store = Store::open(root.clone())?;
    let schedule = season_schedule::cached(&root)?;
    let mut state = State { schedule_view: Default::default(), store, schedule: Some(schedule), controller: Controller::new(), page: 0, hits: vec![], scroll: 0, message: String::new(), refresh: None, tray_ready: false, auto_tray_done: false, fixture: false, ignore_close: false, mutex: null_mut(), input: Interaction::default() };
    unsafe {
        let render = |state: &mut State, name: &str| render_bitmap(state, &directory.join(name), 1360, 1010);
        state.action(null_mut(), Action::Week(5));
        state.store.config.favorite_series = Some([1usize, 2, 3, 4].into_iter().map(|index| state.schedule.as_ref().unwrap().catalog[index].id.clone()).collect());
        render(&mut state, "season-comparison.bmp")?;
        state.input.hovered = Some(Action::Select(5, 1));
        render(&mut state, "interaction-hover.bmp")?;
        state.input.pressed = Some(Action::Select(5, 1));
        render(&mut state, "interaction-pressed.bmp")?;
        state.input = Interaction { focused: Some(Action::Week(5)), focus_visible: true, ..Interaction::default() };
        render(&mut state, "interaction-focus.bmp")?;
        state.input = Interaction::default();
        state.action(null_mut(), Action::Filter(schedule_ui::Filter::SportsCar));
        state.action(null_mut(), Action::Select(5, 3));
        state.action(null_mut(), Action::License(crate::race_catalog::Category::SportsCar, Some(crate::race_catalog::License::D)));
        render(&mut state, "license-sports-d.bmp")?;
        state.action(null_mut(), Action::ConditionalSeries);
        state.action(null_mut(), Action::Locate(5, 3));
        render(&mut state, "porsche-cup.bmp")?;
        state.action(null_mut(), Action::Filter(schedule_ui::Filter::Formula));
        render(&mut state, "schedule-hidden-selection.bmp")?;
        state.action(null_mut(), Action::Week(6));
        state.action(null_mut(), Action::Select(6, 6));
        state.action(null_mut(), Action::License(crate::race_catalog::Category::Formula, Some(crate::race_catalog::License::D)));
        render(&mut state, "f4-regional.bmp")?;
        let restored = Store::open(root)?;
        if restored.plans.choices.len() != 2 || restored.plans.choices.values().map(|plan| plan.series).collect::<Vec<_>>() != [3, 6] { return Err("UI plan persistence failed".into()); }
        state.action(null_mut(), Action::Filter(schedule_ui::Filter::All));
        state.action(null_mut(), Action::FavoritesOnly(true)); state.action(null_mut(), Action::Week(5));
        state.action(null_mut(), Action::Select(5, 2)); state.message.clear();
        render(&mut state, "schedule-week-selection.bmp")?;
        let saved_choices = state.store.plans.clone();
        let schedule = state.schedule.as_ref().unwrap();
        for week in &schedule.weeks {
            let series = (week.number as usize-1)%season_schedule::CARS.len();
            state.store.plans.choices.insert(schedule.key(week.number), Plan { season: schedule.season.clone(), week: week.number, date: week.start.clone(), series, series_id: schedule.catalog[series].id.clone(), series_name: schedule.catalog[series].name.clone(), track: week.tracks[series].clone() });
        }
        render(&mut state, "schedule-circuits.bmp")?;
        render_bitmap(&mut state, &directory.join("schedule-circuits-compact.bmp"), 1040, 600)?;
        render_bitmap_scaled(&mut state, &directory.join("schedule-circuits-150.bmp"), 1040, 600, 144)?;
        state.store.plans = saved_choices;
        state.page = PAGE_APPS; render(&mut state, "apps-and-tray.bmp")?;
        state.message.clear();
        state.controller.phase = Phase::Running;
        for spec in state.store.config.apps.iter().take(4) { state.controller.status.insert(spec.id.clone(), 1); }
        render(&mut state, "interaction-disabled.bmp")?;
        state.controller.phase = Phase::Stopping; render(&mut state, "session-stopping.bmp")?;
        state.controller.phase = Phase::CleanupFailed; render(&mut state, "session-retry.bmp")?;
        state.controller = Controller::new(); state.message.clear();
        render_bitmap(&mut state, &directory.join("compact-settings.bmp"), 1040, 600)?;
        state.page = 0; render_bitmap(&mut state, &directory.join("compact-schedule.bmp"), 1040, 600)?;
        state.scroll = 1000; render_bitmap(&mut state, &directory.join("compact-scrolled.bmp"), 1040, 600)?;
        state.scroll = 0;
        for dpi in [120, 144, 192] {
            render_bitmap_scaled(&mut state, &directory.join(format!("typography-schedule-{}.bmp", dpi*100/96)), 1040, 600, dpi)?;
        }
        state.page = PAGE_APPS; render_bitmap_scaled(&mut state, &directory.join("typography-settings-150.bmp"), 1040, 600, 144)?;
        state.page = 0; render_bitmap(&mut state, &directory.join("layout-wide-schedule.bmp"), 1920, 1080)?;
        state.page = PAGE_APPS; state.scroll = 1000; render_bitmap(&mut state, &directory.join("layout-settings-scrolled.bmp"), 1040, 600)?;
        for theme in [design::Theme::Light, design::Theme::Dark] {
            for density in [design::Density::Comfortable, design::Density::Compact] {
                state.store.config.appearance = design::Preferences { theme, density };
                state.message.clear(); state.input = Interaction::default();
                for page in [PAGE_SCHEDULE, PAGE_APPS, PAGE_LEARN] {
                    state.page = page; state.scroll = 0;
                    render_bitmap(&mut state, &directory.join(format!("framework-{theme:?}-{density:?}-{page}.bmp")), 1360, 900)?;
                }
                render_bitmap_scaled(&mut state, &directory.join(format!("framework-{theme:?}-{density:?}-200.bmp")), 1040, 600, 192)?;
            }
        }
        state.store.config.appearance = design::Preferences::legacy(); state.page = PAGE_SCHEDULE; state.store.config.favorite_series = Some(Default::default());
        render(&mut state, "framework-empty-favorites.bmp")?;
        state.message = "Update failed · Offline. Previous schedule kept. Reconnect and refresh to try again.".into();
        render(&mut state, "framework-offline.bmp")?;
        let saved_schedule = state.schedule.clone();
        state.store.config.favorite_series = Some([1usize, 2, 3, 4].into_iter().map(|index| state.schedule.as_ref().unwrap().catalog[index].id.clone()).collect());
        state.schedule_view.favorites_only = true;
        state.message.clear();
        for theme in [design::Theme::Light, design::Theme::Dark] { for density in [design::Density::Comfortable, design::Density::Compact] {
            state.store.config.appearance = design::Preferences { theme, density };
            for (width, height) in [(320, 568), (640, 360), (400, 600), (160, 180)] {
                for page in 0..3 {
                    state.page = page; state.scroll = 0;
                    render_bitmap(&mut state, &directory.join(format!("reflow-{theme:?}-{density:?}-{width}-{height}-{page}.bmp")), width, height)?;
                }
            }
        } }
        state.schedule = saved_schedule;
        state.page=PAGE_LEARN;state.scroll=0;state.store.config.appearance.theme=design::Theme::Dark;render(&mut state,"learn.bmp")?;
        state.page = PAGE_SCHEDULE;
        state.schedule = None; render(&mut state, "framework-no-schedule.bmp")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_fit_at_supported_scales() {
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            for dpi in [96, 120, 144, 192] {
                logical_scale(dc, dpi);
                for (label, width, height, size, bold) in [
                    ("▶  Launch all", 132, 40, FONT_BODY, true), ("Retry stop", 132, 40, FONT_BODY, true),
                    ("■  Stop all", 132, 40, FONT_BODY, false), ("Stop all", 132, 36, FONT_BODY, false),
                    ("Official PDF", 100, 36, FONT_BODY, false), ("Refresh schedule", 120, 36, FONT_BODY, false),
                    ("Refreshing…", 120, 36, FONT_BODY, false), ("Hide app windows", 142, 36, FONT_BODY, false),
                    ("✓  Auto-hide", 152, 36, FONT_BODY, false), ("Auto-hide: off", 152, 36, FONT_BODY, false),
                    ("Include", 57, 30, FONT_BODY, false), ("Hide", 71, 30, FONT_BODY, false),
                    ("Path", 38, 30, FONT_BODY, false), ("Open", 38, 30, FONT_BODY, false),
                    ("Sports Car 4", 88, 32, FONT_BODY, false), ("Clear", 62, 32, FONT_BODY, false),
                    ("● Running", 108, 26, FONT_CAPTION, false),
                    ("! Check path", 108, 26, FONT_CAPTION, false),
                    ("Apps", 128, 42, FONT_BODY, false), ("Data folder", 104, 36, FONT_BODY, false),
                    ("INCLUDED", 81, 32, FONT_CAPTION, false), ("2026-S4  ·  12 weeks  ·  12 planned", 284, 24, FONT_BODY, false),
                    ("Updated 2026-10-14 · Refresh needed · Resets Tue 09:00 KST", 482, 24, FONT_CAPTION, false),
                ] {
                    let font = ui_font(size, bold); assert!(!font.is_null());
                    let previous = SelectObject(dc, font);
                    let measured = measured_text(dc, label, rect(0, 0, width, height), DT_SINGLELINE);
                    SelectObject(dc, previous); DeleteObject(font);
                    assert!(measured.right <= width && measured.bottom <= height, "{label}, dpi={dpi}, {}x{} in {width}x{height}", measured.right, measured.bottom);
                }
            }
            DeleteDC(dc);
        }
    }

    #[test]
    fn schedule_cells_reserve_space_for_wrapped_circuits_and_full_catalog_labels() {
        let mut state = regression_tests::review_state("full-catalog");
        let mut schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
        schedule.expand_catalog(include_str!("../assets/schedule-full-fixture.txt")).unwrap();
        state.store.config.favorite_series = Some(schedule.catalog.iter().take(8).map(|series| series.id.clone()).collect());
        state.schedule = Some(schedule);
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            for dpi in [96, 120, 144, 192] {
                logical_scale(dc, dpi);
                for width in [1040, 1360, 1920] {
                    let layout = Layout::new(width, 900); state.schedule_view.favorites_only = true; state.measure_schedule_rows(dc, &layout);
                    let font = ui_font(FONT_BODY, true); let previous = SelectObject(dc, font);
                    let cell = state.season_cell(&layout, 0, 0, 160);
                    for week in 1..=12 {
                        for index in 0..8 {
                            if let Some(track) = state.schedule.as_ref().unwrap().track(week, index) {
                                let lines = wrapped_lines(dc, track, cell.right-cell.left-24);
                                assert!(lines.len() as i32*27+60 <= state.schedule_view.week_heights[week as usize-1], "{track}");
                                assert_eq!(lines.join("").replace(' ', ""), track.replace(' ', ""));
                            }
                        }
                    }
                    state.schedule_view.favorites_only = false; state.measure_schedule_rows(dc, &layout);
                    let text_width = (layout.right-layout.left)/state.schedule_view.columns as i32-40;
                    for (index, series) in state.schedule.as_ref().unwrap().catalog.iter().enumerate() {
                        let label = schedule_ui::catalog_label(series);
                        if state.schedule_view.indices(state.schedule.as_ref().unwrap(), &state.store).contains(&index) { assert!(wrapped_lines(dc, &label, text_width).len() as i32*27+180 <= state.schedule_view.row_height(index), "catalog {index} width={width} dpi={dpi} measured={} row={}", wrapped_lines(dc, &label, text_width).len(), state.schedule_view.row_height(index)); }
                    }
                    SelectObject(dc, previous); DeleteObject(font);
                }
            }
            DeleteDC(dc);
        }
    }

    #[test]
    fn saved_circuits_fit_in_the_schedule_summary() {
        let schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            for dpi in [96, 120, 144, 192] {
                logical_scale(dc, dpi);
                let font = ui_font(FONT_BODY, false); let previous = SelectObject(dc, font);
                for track in schedule.weeks.iter().flat_map(|week| &week.tracks) {
                    let layout = Layout::new(1034, 600); let available = layout.right-layout.left-126;
                    let measured = measured_text(dc, track, rect(0, 0, available, 36), DT_WORDBREAK);
                    assert!(measured.right <= available && measured.bottom <= 36, "{track}, dpi={dpi}, {}x{} in {available}x36", measured.right, measured.bottom);
                }
                SelectObject(dc, previous); DeleteObject(font);
            }
            DeleteDC(dc);
        }
    }

    #[test]
    fn season_columns_are_bounded_and_keep_the_week_column_separate() {
        let mut state = regression_tests::review_state("columns");
        for width in [1034, 1360, 1920, 2560] {
            let layout = Layout::new(width, 600);
            state.schedule_view.columns = if layout.right-layout.left >= 1040 { 4 } else { 3 };
            state.schedule_view.column_width = ((layout.right-layout.left-132)/state.schedule_view.columns as i32).max(228);
            let columns: Vec<_> = (0..state.schedule_view.columns).map(|column| state.season_cell(&layout, column, 320, 176)).collect();
            assert_eq!(columns[0].left, layout.left+132);
            assert_eq!(columns.last().unwrap().right, layout.left+132+state.schedule_view.column_width*state.schedule_view.columns as i32-8);
            for pair in columns.windows(2) { assert_eq!(pair[1].left-pair[0].right, 8); }
            assert!(columns.iter().all(|cell| cell.right-cell.left >= 190));
        }
    }

    #[test]
    fn responsive_hit_targets_do_not_overlap_or_escape_their_regions() {
        let mut schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
        schedule.expand_catalog(include_str!("../assets/schedule-full-fixture.txt")).unwrap();
        let mut store = Store { root: PathBuf::new(), config: Default::default(), plans: Default::default(), warnings: vec![] };
        for week in &schedule.weeks {
            store.plans.choices.insert(schedule.key(week.number), Plan { season: schedule.season.clone(), week: week.number, date: week.start.clone(), series: 0, series_id: String::new(), series_name: String::new(), track: week.tracks[0].clone() });
        }
        let mut state = State { schedule_view: Default::default(), store, schedule: Some(schedule), controller: Controller::new(), page: 0, hits: vec![], scroll: 0, message: String::new(), refresh: None, tray_ready: false, auto_tray_done: false, fixture: false, ignore_close: false, mutex: null_mut(), input: Interaction::default() };
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            for theme in [design::Theme::Light, design::Theme::Dark] { for density in [design::Density::Comfortable, design::Density::Compact] {
            state.store.config.appearance = design::Preferences { theme, density };
            for (width, height) in [(1000, 600), (1034, 600), (1034, 631), (1034, 632), (1360, 1010), (1920, 1080)] {
                let layout = Layout::new(width, height);
                for page in [PAGE_SCHEDULE, PAGE_APPS] {
                    for scroll in [0, 10000] {
                        state.page = page; state.scroll = scroll; state.draw(dc, width, height);
                        assert_eq!(state.hits.iter().filter_map(|hit| if let Action::Page(page) = hit.action { Some(page) } else { None }).collect::<Vec<_>>(), [PAGE_SCHEDULE, PAGE_APPS,PAGE_LEARN]);
                        assert!(!state.enabled(&Action::Page(3)));
                        let expected = match page { PAGE_SCHEDULE => Action::WeekMenu, PAGE_LEARN=>Action::LearnOpen(0), _ => Action::Data };
                        assert!(state.hits.iter().any(|hit| hit.action == expected));
                        for (index, hit) in state.hits.iter().enumerate() {
                            let area = hit.rect;
                            assert!(area.left >= 0 && area.right <= width && area.top >= 0 && area.bottom <= height && area.left < area.right && area.top < area.bottom, "out of viewport: {:?}", hit.action);
                            if matches!(hit.action, Action::Page(_) | Action::Start | Action::Exit) { assert!(area.right < SIDEBAR_WIDTH); }
                            if matches!(hit.action, Action::Select(_, _) | Action::Browse(_) | Action::Enable(_) | Action::TrayMode(_) | Action::Restore(_)) { assert!(area.top >= TABLE_TOP && area.bottom <= layout.bottom); }
                            if state.enabled(&hit.action) { assert_eq!(state.hit_at(((area.left+area.right)/2, (area.top+area.bottom)/2)), Some(hit.action.clone())); }
                            for other in &state.hits[index+1..] {
                                let intersection = area.left.max(other.rect.left) < area.right.min(other.rect.right) && area.top.max(other.rect.top) < area.bottom.min(other.rect.bottom);
                                assert!(!intersection, "overlapping targets: {:?}, {:?}", hit.action, other.action);
                            }
                        }
                    }
                }
            }
            } }
            DeleteDC(dc);
        }
    }
}
