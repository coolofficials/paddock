use super::*;
use std::{fs, time::{Duration, Instant}};

#[test]
fn dense_schedule_keeps_track_names_and_participation_clipped_and_cancelable() {
    unsafe {
        let mut state = review_state("track-selection-targets");
        let mut schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
        schedule.expand_catalog(include_str!("../assets/schedule-full-fixture.txt")).unwrap();
        state.store.config.favorite_series = Some(schedule.catalog.iter().take(8).map(|series| series.id.clone()).collect());
        state.schedule = Some(schedule);
        let dc = CreateCompatibleDC(null_mut());
        let original = serde_json::to_vec(&state.store.plans).unwrap();
        for width in [1040, 1360] { for offset in [0, 55, 140, 330] { for scroll in [0, 37, 100] {
            state.schedule_view.horizontal = offset; state.scroll = scroll; state.draw(dc, width, 600);
            let top = state.schedule_view.body_top;
            assert!(top < 300);
            let targets: Vec<_> = state.hits.iter().filter(|hit| matches!(hit.action, Action::Select(_, _))).collect();
            assert!(!targets.is_empty());
            for hit in targets {
                assert!(hit.rect.left >= Layout::new(width, 600).left+132 && hit.rect.right <= Layout::new(width, 600).right);
                assert!(hit.rect.top >= top && hit.rect.bottom <= 552);
                assert_eq!(state.hit_at(((hit.rect.left+hit.rect.right)/2, (hit.rect.top+hit.rect.bottom)/2)), Some(hit.action.clone()));
                if let Action::Select(week, index) = hit.action {
                    assert!(hit.label.contains(state.schedule.as_ref().unwrap().track(week, index).unwrap()));
                }
            }
            assert_eq!(serde_json::to_vec(&state.store.plans).unwrap(), original);
        } } }
        let window = review_window(&mut state);
        state.schedule_view.horizontal = 0; state.scroll = 0; state.draw(dc, 1360, 600);
        let target = state.hits.iter().find(|hit| hit.action == Action::Select(1, 0)).unwrap().rect;
        let dpi = design::effective_dpi(GetDpiForWindow(window));
        let x = (target.left+target.right)/2*dpi/96; let y = (target.top+target.bottom)/2*dpi/96;
        let point = ((y as isize) << 16) | x as isize;
        SendMessageW(window, WM_LBUTTONDOWN, 0, point);
        assert_eq!(state.input.pressed, Some(Action::Select(1, 0)));
        SendMessageW(window, WM_CANCELMODE, 0, 0);
        SendMessageW(window, WM_LBUTTONUP, 0, point);
        assert!(state.input.pressed.is_none()); assert_ne!(GetCapture(), window);
        assert_eq!(serde_json::to_vec(&state.store.plans).unwrap(), original);
        state.action(null_mut(), Action::Select(1, 0));
        assert!(state.selected(&Action::Select(1, 0)));
        state.action(null_mut(), Action::Select(1, 0));
        assert!(!state.selected(&Action::Select(1, 0)));
        DestroyWindow(window); DeleteDC(dc);
    }
}





#[test]
fn schedule_horizontal_drag_and_keyboard_preserve_every_week_selection() {
    unsafe {
        let mut state = review_state("horizontal");
        let mut schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
        schedule.expand_catalog(include_str!("../assets/schedule-full-fixture.txt")).unwrap();
        state.store.config.favorite_series = Some(schedule.catalog.iter().take(8).map(|series| series.id.clone()).collect()); state.schedule = Some(schedule);
        let screen = GetDC(null_mut()); let dc = CreateCompatibleDC(screen); let bitmap = CreateCompatibleBitmap(screen, 1400, 900); ReleaseDC(null_mut(), screen); let previous = SelectObject(dc, bitmap);
        state.draw(dc, 1360, 850); let original = serde_json::to_vec(&state.store.plans).unwrap();
        let (track, thumb) = state.schedule_view.bar.unwrap();
        let window = review_window(&mut state); state.input.reflow = false;
        let point = |horizontal: i32, vertical: i32| ((vertical as isize) << 16) | horizontal as isize;
        SendMessageW(window, WM_LBUTTONDOWN, 0, point(thumb.left+2, track.top+3));
        assert!(state.schedule_view.drag.is_some());
        SendMessageW(window, WM_MOUSEMOVE, 0, point(track.right, track.top+3));
        assert_eq!(state.schedule_view.horizontal, state.schedule_view.horizontal_max);
        SendMessageW(window, WM_LBUTTONUP, 0, point(track.right, track.top+3)); assert!(state.schedule_view.drag.is_none());
        state.draw(dc, 1360, 850);
        assert!(state.hits.iter().filter(|hit| matches!(hit.action, Action::Select(_, _) | Action::SeriesInfo(_))).all(|hit| hit.rect.left >= Layout::new(1360, 850).left+132 && hit.rect.right <= Layout::new(1360, 850).right));
        let end = state.schedule_view.horizontal; SendMessageW(window, WM_KEYDOWN, VK_LEFT as usize, 0); assert!(state.schedule_view.horizontal < end);
        assert_eq!(serde_json::to_vec(&state.store.plans).unwrap(), original);
        state.input.reflow = true; state.draw(dc, 640, 360); assert!(state.schedule_view.bar.is_none());
        DestroyWindow(window); SelectObject(dc, previous); DeleteObject(bitmap); DeleteDC(dc);
    }
}



pub(super) fn review_state(label: &str) -> Box<State> {
    let root = std::env::temp_dir().join(format!("paddock-review-{}-{label}", std::process::id()));
    let mut store = Store::open(root).unwrap();
    store.config.apps.clear();
    store.config.launcher_to_tray = true;
    Box::new(State { schedule_view: Default::default(), store, schedule: None, controller: Controller::new(), page: PAGE_SCHEDULE, hits: vec![], scroll: 0, message: String::new(), refresh: None, tray_ready: true, auto_tray_done: false, fixture: true, ignore_close: false, mutex: null_mut(), input: Interaction::default() })
}



unsafe fn review_window(state: &mut State) -> HWND {
    let instance = GetModuleHandleW(null());
    let class = wide("PaddockIsolatedReviewWindow");
    let mut description: WNDCLASSW = std::mem::zeroed();
    description.hInstance = instance;
    description.lpszClassName = class.as_ptr();
    description.lpfnWndProc = Some(procedure);
    RegisterClassW(&description);
    let window = CreateWindowExW(0, class.as_ptr(), wide("Isolated review fixture").as_ptr(), WS_OVERLAPPEDWINDOW, -32000, -32000, 120, 100, null_mut(), null_mut(), instance, state as *mut State as _);
    assert!(!window.is_null());
    ShowWindow(window, SW_SHOWNOACTIVATE);
    window
}

#[test]
fn review_reopening_launcher_must_cancel_pending_auto_hide() {
    let mut state = review_state("reopen");
    unsafe {
        let window = review_window(&mut state);
        state.controller.phase = Phase::Running;
        state.controller.seen_main = true;
        state.controller.start_time = Instant::now() - Duration::from_secs(11);
        state.hide(window);
        SendMessageW(window, TRAY_MESSAGE, 0, WM_LBUTTONUP as isize);
        assert_ne!(IsWindowVisible(window), 0);
        SendMessageW(window, WM_TIMER, 1, 0);
        let still_visible = IsWindowVisible(window) != 0;
        DestroyWindow(window);
        assert!(still_visible, "Explicit tray restore was overridden by the pending ten-second auto-hide");
    }
}

#[test]
fn review_idle_minimize_must_not_apply_start_all_tray_policy() {
    let mut state = review_state("minimize");
    unsafe {
        let window = review_window(&mut state);
        state.fixture = false;
        SendMessageW(window, WM_SIZE, SIZE_MINIMIZED as usize, 0);
        let not_hidden = IsWindowVisible(window) != 0;
        state.fixture = true;
        DestroyWindow(window);
        assert!(not_hidden, "Idle minimize must keep the normal taskbar behavior");
    }
}

#[test]
fn review_failed_helper_preference_save_must_restore_memory() {
    let mut state = review_state("save-failure");
    state.store.config.apps.push(storage::AppSpec { id: "isolated-helper".into(), name: "Fixture".into(), path: state.store.root.join("never-executed.exe").to_string_lossy().into_owned(), working_dir: state.store.root.to_string_lossy().into_owned(), enabled: true, tray: true, rehide_after_game: false, extra_paths: vec![] });
    let path = state.store.root.join("config.json");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    unsafe { state.action(null_mut(), Action::TrayMode(0)); }
    let preserved = state.store.config.apps[0].tray;
    fs::remove_dir(&path).unwrap();
    assert!(preserved, "Save failure still changes the in-memory policy used by the next Start All");
}

#[test]
fn review_replacing_schedule_choice_should_be_undoable() {
    let mut state = review_state("schedule-undo");
    let mut schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
    schedule.expand_catalog(include_str!("../assets/schedule-full-fixture.txt")).unwrap(); state.schedule = Some(schedule);
    unsafe {
        state.action(null_mut(), Action::Select(5, 0));
        let previous = state.store.plans.choices["2026-S4:5"].series;
        state.action(null_mut(), Action::Select(5, 1));
        state.action(null_mut(), Action::UndoPlan("2026-S4:5".into()));
        assert_eq!(state.store.plans.choices["2026-S4:5"].series, previous, "Replacement cannot be undone although deletion can");
    }
}

#[test]
fn search_locates_without_changing_plans_and_navigation_preserves_scroll() {
    let mut state = review_state("search");
    let mut schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
    schedule.expand_catalog(include_str!("../assets/schedule-full-fixture.txt")).unwrap(); state.schedule = Some(schedule);
    unsafe {
        state.action(null_mut(), Action::Select(5, 0));
        let before = fs::read(state.store.root.join("plans.json")).unwrap();
        state.action(null_mut(), Action::Locate(5, 3));
        assert_eq!(fs::read(state.store.root.join("plans.json")).unwrap(), before);
        state.scroll = 168;
        state.action(null_mut(), Action::Week(6)); let week_offset = state.schedule_view.week_offset(6); assert_eq!(state.scroll, week_offset);
        state.action(null_mut(), Action::Page(PAGE_APPS));
        state.action(null_mut(), Action::Page(PAGE_SCHEDULE)); assert_eq!(state.scroll, week_offset);
    }
}




#[test]
fn filters_remember_independent_scroll_and_undo_has_no_stale_removal() {
    let mut state = review_state("filter-undo");
    let mut schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
    schedule.expand_catalog(include_str!("../assets/schedule-full-fixture.txt")).unwrap(); state.schedule = Some(schedule);
    unsafe {
        state.scroll = 152;
        state.action(null_mut(), Action::Filter(schedule_ui::Filter::Formula));
        state.scroll = 268;
        state.action(null_mut(), Action::Filter(schedule_ui::Filter::All));
        assert_eq!(state.scroll, 152);
        state.action(null_mut(), Action::Filter(schedule_ui::Filter::Formula));
        assert_eq!(state.scroll, 268);
        state.store.plans = Default::default();
        state.action(null_mut(), Action::Select(5, 0));
        state.action(null_mut(), Action::Remove("2026-S4:5".into()));
        state.action(null_mut(), Action::UndoPlan("2026-S4:5".into()));
        state.action(null_mut(), Action::UndoPlan("2026-S4:5".into()));
        assert!(!state.enabled(&Action::UndoPlan("2026-S4:5".into())));
        assert!(!state.store.plans.choices.contains_key("2026-S4:5"));
    }
}

#[test]
fn narrow_schedule_search_reveals_result_without_selecting_it() {
    let mut state = review_state("narrow-search");
    let mut schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
    schedule.expand_catalog(include_str!("../assets/schedule-full-fixture.txt")).unwrap(); state.schedule = Some(schedule);
    unsafe {
        let dc = CreateCompatibleDC(null_mut());
        state.draw(dc, 320, 568);
        state.action(null_mut(), Action::Locate(5, 3));
        state.draw(dc, 320, 568);
        assert!(state.hits.iter().any(|hit| hit.action == Action::SeriesInfo(3)));
        assert!(state.store.plans.choices.is_empty());
        DeleteDC(dc);
    }
}
#[test]
fn scrollbar_drag_changes_scroll_and_releases_capture() {
    let mut state = review_state("scroll-drag");
    unsafe {
        let window = review_window(&mut state);
        state.input.scrollbar = Some((rect(20, 10, 36, 110), rect(20, 10, 36, 30), 400));
        let dpi = design::effective_dpi(GetDpiForWindow(window));
        let position = |horizontal: i32, vertical: i32| ((horizontal*dpi/96) | ((vertical*dpi/96)<<16)) as isize;
        SendMessageW(window, WM_LBUTTONDOWN, 1, position(25, 15));
        SendMessageW(window, WM_MOUSEMOVE, 1, position(25, 75));
        let moved = state.scroll > 200 && GetCapture() == window;
        SendMessageW(window, WM_LBUTTONUP, 0, position(25, 75));
        let released = state.input.scroll_drag.is_none() && GetCapture() != window;
        DestroyWindow(window);
        assert!(moved && released);
    }
}
#[test]
fn bundled_font_resolves_all_hangul_at_three_weights() {
    unsafe {
        assert!(design::ensure_font());
        let dc = CreateCompatibleDC(null_mut());
        let characters: Vec<u16> = (0xac00..=0xd7a3).collect();
        for weight in [400, 500, 600] {
            let font = CreateFontW(-15, 0, 0, 0, weight, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32, wide(FONT_FAMILY).as_ptr());
            let previous = SelectObject(dc, font);
            let mut face = vec![0u16; 128]; let count = GetTextFaceW(dc, face.len() as i32, face.as_mut_ptr());
            let selected_face = String::from_utf16_lossy(&face[..count.max(1) as usize-1]);
            let mut glyphs = vec![0u16; characters.len()];
            let mapped = GetGlyphIndicesW(dc, characters.as_ptr(), characters.len() as i32, glyphs.as_mut_ptr(), GGI_MARK_NONEXISTING_GLYPHS);
            SelectObject(dc, previous); DeleteObject(font);
            assert_eq!(selected_face, FONT_FAMILY);
            assert_eq!(mapped as usize, characters.len());
            assert!(glyphs.iter().all(|glyph| *glyph != 0xffff), "weight {weight}");
        }
        DeleteDC(dc);
    }
}



