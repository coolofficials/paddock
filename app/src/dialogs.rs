use super::*;
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED, ODS_FOCUS};
use windows_sys::Win32::UI::Input::Ime::{ImmGetContext, ImmGetCompositionStringW, ImmReleaseContext, GCS_COMPSTR};

struct Picker {
    descriptions: Vec<String>,
    options: Vec<String>, visible: Vec<usize>, result: Option<usize>, filter: HWND, list: HWND, detail: HWND,
    font: HFONT, detail_font: HFONT, brush: HBRUSH, done: bool, detail_mode: bool,
}
unsafe fn entered(window: HWND) -> String {
    let length = GetWindowTextLengthW(window).clamp(0, 32768) as usize;
    let mut buffer = vec![0u16; length+1]; let copied = GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..copied.max(0) as usize])
}
unsafe fn composing(window: HWND) -> bool {
    let context = ImmGetContext(window); if context.is_null() { return false; }
    let result = ImmGetCompositionStringW(context, GCS_COMPSTR, null_mut(), 0) > 0; ImmReleaseContext(window, context); result
}
impl Picker {
    unsafe fn update(&mut self) {
        let query = normalized(&entered(self.filter)).trim().to_lowercase();
        self.visible = self.options.iter().enumerate().filter(|(index, label)| normalized(label).to_lowercase().contains(&query) || self.descriptions.get(*index).map(|description| normalized(description).to_lowercase().contains(&query)).unwrap_or(false)).map(|(index, _)| index).collect();
        SendMessageW(self.list, LB_RESETCONTENT, 0, 0);
        for index in &self.visible { SendMessageW(self.list, LB_ADDSTRING, 0, wide(&self.options[*index]).as_ptr() as isize); }
        if !self.visible.is_empty() { SendMessageW(self.list, LB_SETCURSEL, 0, 0); }
        EnableWindow(GetDlgItem(GetParent(self.list), IDOK), i32::from(!self.visible.is_empty()));
        self.describe();
    }
    unsafe fn describe(&self) {
        let selected = SendMessageW(self.list, LB_GETCURSEL, 0, 0);
        let value = if selected >= 0 { self.visible.get(selected as usize).map(|index| self.descriptions.get(*index).unwrap_or(&self.options[*index]).as_str()).unwrap_or("") } else { "No results. Clear your search to try again." };
        SetWindowTextW(self.detail, wide(value).as_ptr()); reading_text::format(self.detail);
    }
    unsafe fn accept(&mut self) {
        let selected = SendMessageW(self.list, LB_GETCURSEL, 0, 0);
        if selected >= 0 { self.result = self.visible.get(selected as usize).copied(); self.done = self.result.is_some(); }
    }
}
unsafe fn layout_picker(window: HWND, picker: &Picker) {
    if picker.list.is_null() { return; }
    let mut bounds = RECT::default(); GetClientRect(window, &mut bounds);
    let dpi = design::effective_dpi(GetDpiForWindow(window)); let scale = |value: i32| value*dpi/96;
    let right = bounds.right*96/dpi-12; let bottom = bounds.bottom*96/dpi-12;
    let place = |control: HWND, area: RECT| { MoveWindow(control, scale(area.left), scale(area.top), scale((area.right-area.left).max(1)), scale((area.bottom-area.top).max(1)), 1); };
    let compact = bottom < 280; let query_top = if compact { 4 } else { 32 };
    let detail_height = if bottom >= 340 { 72 } else if bottom >= 240 { 48 } else { 0 };
    let list_top = query_top+44; let list_bottom = bottom-44-if detail_height > 0 { detail_height+8 } else { 0 };
    ShowWindow(GetDlgItem(window, 9), if compact || picker.detail_mode { SW_HIDE } else { SW_SHOWNA });
    for control in [picker.filter, picker.list] { ShowWindow(control, if picker.detail_mode { SW_HIDE } else { SW_SHOWNA }); }
    place(GetDlgItem(window, 9), rect(12, 4, right, 28));
    place(picker.filter, rect(12, query_top, right, query_top+36));
    place(picker.list, rect(12, list_top, right, list_bottom));
    ShowWindow(picker.detail, if picker.detail_mode || detail_height > 0 { SW_SHOWNA } else { SW_HIDE });
    place(picker.detail, if picker.detail_mode { rect(12, 12, right, bottom-48) } else { rect(12, list_bottom+8, right, bottom-48) });
    place(GetDlgItem(window, IDOK), rect(12, bottom-36, (right+12)/2-4, bottom));
    place(GetDlgItem(window, IDCANCEL), rect((right+12)/2+4, bottom-36, right, bottom));
}
unsafe extern "system" fn procedure(window: HWND, message: u32, parameter: WPARAM, data: LPARAM) -> LRESULT {
    if message == WM_NCCREATE { SetWindowLongPtrW(window, GWLP_USERDATA, (*(data as *const CREATESTRUCTW)).lpCreateParams as isize); }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Picker;
    if pointer.is_null() { return DefWindowProcW(window, message, parameter, data); }
    let picker = &mut *pointer;
    match message {
        WM_SIZE => { layout_picker(window, picker); 0 }
        WM_COMMAND => {
            match (parameter & 65535) as i32 {
                IDOK => picker.accept(), IDCANCEL => picker.done = true,
                10 if (parameter>>16) as u32 == EN_CHANGE => picker.update(),
                11 if (parameter>>16) as u32 == LBN_SELCHANGE => picker.describe(),
                11 if (parameter>>16) as u32 == LBN_DBLCLK => picker.accept(), _ => {},
            } 0
        }
        WM_CLOSE => { picker.done = true; 0 }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => { let dc = parameter as HDC; SetTextColor(dc, design::resolve(TEXT)); SetBkColor(dc, design::resolve(CARD)); picker.brush as isize }
        WM_ERASEBKGND => { let mut bounds: RECT = std::mem::zeroed(); GetClientRect(window, &mut bounds); FillRect(parameter as HDC, &bounds, picker.brush); 1 }
        WM_DRAWITEM => {
            let item = &*(data as *const DRAWITEMSTRUCT); let dc = item.hDC; let dpi = design::effective_dpi(GetDpiForWindow(window));
            let saved = SaveDC(dc); logical_scale(dc, dpi);
            let area = rect(0, 0, (item.rcItem.right-item.rcItem.left)*96/dpi, (item.rcItem.bottom-item.rcItem.top)*96/dpi);
            let primary = item.CtlID == IDOK as u32 && IsWindowEnabled(item.hwndItem) != 0;
            rounded(dc, area, if primary { if item.itemState & ODS_SELECTED != 0 { design::ACCENT_HOVER } else { design::ACCENT } } else if item.itemState & ODS_SELECTED != 0 { PRESSED } else { CARD }, design::CONTROL_BORDER);
            text_format(dc, if item.CtlID == IDOK as u32 { if picker.detail_mode { "OK" } else { "PLAN" } } else { "Cancel" }, area, FONT_BODY, if primary { design::ON_ACCENT } else { TEXT }, false, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
            if item.itemState & ODS_FOCUS != 0 { let mut focus = rect(3, 3, area.right-3, area.bottom-3); DrawFocusRect(dc, &mut focus); }
            RestoreDC(dc, saved); 1
        }
        _ => DefWindowProcW(window, message, parameter, data),
    }
}
pub(super) unsafe fn choose(owner: HWND, title: &str, options: Vec<String>) -> Option<usize> {
    choose_with(owner, title, options, None)
}
unsafe fn choose_with(owner: HWND, title: &str, options: Vec<String>, inspect: Option<&mut dyn FnMut(HWND, *mut Picker)>) -> Option<usize> {
    if owner.is_null() { return None; }
    let instance = GetModuleHandleW(null()); let class = wide("PaddockCommandPicker");
    let mut description: WNDCLASSW = std::mem::zeroed(); description.hInstance = instance; description.lpszClassName = class.as_ptr(); description.lpfnWndProc = Some(procedure); description.hCursor = LoadCursorW(null_mut(), IDC_ARROW); RegisterClassW(&description);
    let dpi = design::effective_dpi(GetDpiForWindow(owner)); let scale = |value: i32| value*dpi/96;
    let mut picker = Box::new(Picker { descriptions: vec![], options, visible: vec![], result: None, filter: null_mut(), list: null_mut(), detail: null_mut(), font: ui_font(scale(FONT_BODY), false), detail_font: ui_font(scale(16), false), brush: CreateSolidBrush(design::resolve(CARD)), done: false, detail_mode: false });
    let mut owner_area: RECT = std::mem::zeroed(); GetWindowRect(owner, &mut owner_area);
    let mut monitor = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..MONITORINFO::default() }; GetMonitorInfoW(MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST), &mut monitor);
    let width = scale(680).min(monitor.rcWork.right-monitor.rcWork.left-24);
    let height = scale(500).min(monitor.rcWork.bottom-monitor.rcWork.top-24);
    let left = (owner_area.left+scale(32)).clamp(monitor.rcWork.left, monitor.rcWork.right-width);
    let top = (owner_area.top+scale(48)).clamp(monitor.rcWork.top, monitor.rcWork.bottom-height);
    let window = CreateWindowExW(WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT, class.as_ptr(), wide(title).as_ptr(), WS_CAPTION | WS_SYSMENU, left, top, width, height, owner, null_mut(), instance, &mut *picker as *mut Picker as _);
    if window.is_null() { DeleteObject(picker.font); DeleteObject(picker.detail_font); DeleteObject(picker.brush); return None; }
    let child = |kind: &str, label: &str, style: u32, area: RECT, identifier: i32| {
        let control = CreateWindowExW(0, wide(kind).as_ptr(), wide(label).as_ptr(), WS_CHILD | WS_VISIBLE | style, scale(area.left), scale(area.top), scale(area.right-area.left), scale(area.bottom-area.top), window, identifier as usize as HMENU, instance, null());
        SendMessageW(control, WM_SETFONT, picker.font as usize, 1); control
    };
    let mut client = RECT::default(); GetClientRect(window, &mut client); let right = client.right*96/dpi-12; let bottom = client.bottom*96/dpi-12;
    let detail_height = if bottom >= 340 { 72 } else { 40 }; let list_bottom = (bottom-detail_height-56).max(104);
    child("STATIC", "Search query", 0, rect(12, 4, right, 28), 9);
    picker.filter = child("EDIT", "", WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32, rect(12, 32, right, 68), 10);
    picker.list = child("LISTBOX", "Search results", WS_TABSTOP | WS_BORDER | WS_VSCROLL | LBS_NOTIFY as u32, rect(12, 76, right, list_bottom), 11);
    picker.detail = child(reading_text::control_class(), "", WS_TABSTOP | WS_VSCROLL | ES_READONLY as u32 | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32, rect(12, list_bottom+8, right, bottom-48), 12);
    SendMessageW(picker.detail, WM_SETFONT, picker.detail_font as usize, 1);
    child("BUTTON", "PLAN", WS_TABSTOP | BS_OWNERDRAW as u32, rect(12, bottom-36, (right+12)/2-4, bottom), IDOK);
    child("BUTTON", "Cancel", WS_TABSTOP | BS_OWNERDRAW as u32, rect((right+12)/2+4, bottom-36, right, bottom), IDCANCEL);
    layout_picker(window, &picker);
    picker.update(); EnableWindow(owner, 0); apply_window_theme(window); ShowWindow(window, SW_SHOW); SetFocus(picker.filter);
    if let Some(inspect) = inspect { inspect(window, &mut *picker); }
    let mut message: MSG = std::mem::zeroed();
    while !picker.done {
        let result = GetMessageW(&mut message, null_mut(), 0, 0);
        if result <= 0 { if result == 0 { PostQuitMessage(message.wParam as i32); } break; }
        if message.message == WM_KEYDOWN && composing(GetFocus()) { TranslateMessage(&message); DispatchMessageW(&message); continue; }
        if message.message == WM_KEYDOWN && message.wParam == VK_RETURN as usize { if GetFocus() == GetDlgItem(window, IDCANCEL) { picker.done = true; } else if !picker.detail_mode || GetFocus() == GetDlgItem(window, IDOK) { picker.accept(); } continue; }
        if message.message == WM_KEYDOWN && message.wParam == VK_ESCAPE as usize { picker.done = true; continue; }
        if message.message == WM_KEYDOWN && message.wParam == VK_DOWN as usize && GetFocus() == picker.filter { SetFocus(picker.list); continue; }
        if IsDialogMessageW(window, &message) == 0 { TranslateMessage(&message); DispatchMessageW(&message); }
    }
    EnableWindow(owner, 1); DestroyWindow(window); SetForegroundWindow(owner); SetFocus(owner); DeleteObject(picker.font); DeleteObject(picker.detail_font); DeleteObject(picker.brush);
    picker.result
}
pub(super) unsafe fn verify(owner: HWND) -> Vec<String> {
    let mut report = Vec::new();
    let mut inspect = |window: HWND, picker: *mut Picker| {
        let entered_filter = GetFocus() == (*picker).filter;
        SetWindowTextW((*picker).filter, wide("없는 검색어").as_ptr());
        let no_results = (*picker).visible.is_empty() && IsWindowEnabled(GetDlgItem(window, IDOK)) == 0 && entered((*picker).detail).contains("No results");
        SetWindowTextW((*picker).filter, wide("한국어").as_ptr());
        let matched = (*picker).visible == [1] && entered((*picker).detail).contains("긴 경로");
        SetFocus(GetDlgItem(window, IDCANCEL));
        let mut message = MSG { hwnd: GetFocus(), message: WM_KEYDOWN, wParam: VK_TAB as usize, ..MSG::default() }; IsDialogMessageW(window, &mut message);
        report.push(format!("Reading text uses the 16px 1.8 paragraph rhythm: {}", reading_text::verified((*picker).detail)));
        report.push(format!("Search dialog initially focuses the query: {entered_filter}"));
        report.push(format!("Search dialog explains empty results and disables acceptance: {no_results}"));
        report.push(format!("Search dialog filters Korean text and exposes complete paths: {matched}"));
        report.push(format!("Search dialog wraps Tab from cancel to query: {}", GetFocus() == (*picker).filter));
        SetWindowPos(window, null_mut(), 0, 0, 320, 240, SWP_NOMOVE | SWP_NOZORDER);
        let mut bounds = RECT::default(); GetClientRect(window, &mut bounds);
        let within = [(*picker).filter, (*picker).list, GetDlgItem(window, IDOK), GetDlgItem(window, IDCANCEL)].iter().all(|control| {
            let mut area = RECT::default(); GetWindowRect(*control, &mut area);
            let mut top = POINT { x: area.left, y: area.top }; let mut bottom = POINT { x: area.right, y: area.bottom };
            ScreenToClient(window, &mut top); ScreenToClient(window, &mut bottom);
            top.x >= 0 && top.y >= 0 && bottom.x <= bounds.right && bottom.y <= bounds.bottom && bottom.y > top.y
        });
        report.push(format!("Low-height search keeps query results and actions inside the client: {within}"));
        (*picker).detail_mode = true; layout_picker(window, &*picker);
        report.push(format!("Low-height confirmation preserves scrollable text and visible cancel: {}", IsWindowVisible((*picker).detail) != 0 && IsWindowVisible(GetDlgItem(window, IDCANCEL)) != 0 && IsWindowVisible((*picker).list) == 0));
        (*picker).detail_mode = false; layout_picker(window, &*picker);
        (*picker).descriptions = vec!["일반 항목".into(), "전체 음량 · app.ini [Audio] masterVolumedB · 조회 전용 사유".into()];
        SetWindowTextW((*picker).filter, wide("masterVolumedB").as_ptr());
        report.push(format!("Settings catalog searches INI details while keeping concise list labels: {}", (*picker).visible == [1] && entered((*picker).detail).contains("조회 전용 사유")));
        PostMessageW((*picker).list, WM_KEYDOWN, VK_RETURN as usize, 0);
    };
    let result = choose_with(owner, "검색 검증", vec!["English series".into(), "한국어 시리즈 · C:\\긴 경로\\iRacing 설정\\rendererDX11Monitor.ini".into()], Some(&mut inspect));
    report.push(format!("Search dialog accepts Enter and restores enabled owner focus: {}", result == Some(1) && GetFocus() == owner && IsWindowEnabled(owner) != 0));
    report
}
pub(super) unsafe fn details(owner: HWND, title: &str, body: &str) { let _ = confirm(owner, title, body); }
pub(super) unsafe fn confirm(owner: HWND, title: &str, body: &str) -> bool {
    let mut inspect = |window: HWND, picker: *mut Picker| {
        (*picker).detail_mode = true;
        SetWindowTextW(GetDlgItem(window, IDOK), wide("OK").as_ptr());
        for control in [GetDlgItem(window, 9), (*picker).filter, (*picker).list] { ShowWindow(control, SW_HIDE); }
        layout_picker(window, &*picker);
        SetFocus(GetDlgItem(window, IDCANCEL));
    };
    choose_with(owner, title, vec![body.into()], Some(&mut inspect)) == Some(0)
}
fn normalized(value: &str) -> String {
    use windows_sys::Win32::Globalization::{NormalizeString, NormalizationC};
    let source: Vec<_> = value.encode_utf16().collect();
    if source.is_empty() { return String::new(); }
    unsafe {
        let length = NormalizeString(NormalizationC, source.as_ptr(), source.len() as i32, null_mut(), 0);
        if length <= 0 { return value.into(); }
        let mut output = vec![0u16; length as usize];
        let written = NormalizeString(NormalizationC, source.as_ptr(), source.len() as i32, output.as_mut_ptr(), length);
        if written <= 0 { value.into() } else { String::from_utf16_lossy(&output[..written as usize]) }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_normalizes_without_rewriting_original_unicode() {
        let original = "\u{1100}\u{1161}\u{11a8}";
        assert_eq!(normalized(original), "각");
        assert_eq!(original.chars().count(), 3);
    }
}


