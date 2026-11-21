use super::*;
use windows_sys::Win32::UI::Controls::{EM_SETLIMITTEXT, EM_SETSEL};
use windows_sys::Win32::UI::Input::Ime::{ImmGetContext, ImmGetCompositionStringW, ImmReleaseContext, GCS_COMPSTR};

const QUERY: i32 = 10;
const CLEAR: i32 = 11;
struct QueryDialog { edit: HWND, font: HFONT, brush: HBRUSH, result: Option<String>, done: bool }

unsafe fn entered(edit: HWND) -> String {
    let length = GetWindowTextLengthW(edit).clamp(0, 32768) as usize;
    let mut buffer = vec![0u16; length+1];
    let copied = GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..copied.max(0) as usize]).trim().to_string()
}
unsafe fn composing(window: HWND) -> bool {
    let context = ImmGetContext(window); if context.is_null() { return false; }
    let active = ImmGetCompositionStringW(context, GCS_COMPSTR, null_mut(), 0) > 0;
    ImmReleaseContext(window, context); active
}
impl QueryDialog {
    unsafe fn command(&mut self, identifier: i32) {
        match identifier {
            IDOK => { self.result = Some(entered(self.edit)); self.done = true; }
            CLEAR => { self.result = Some(String::new()); self.done = true; }
            IDCANCEL => self.done = true,
            _ => {}
        }
    }
}
unsafe fn layout(window: HWND) {
    if GetDlgItem(window, QUERY).is_null() { return; }
    let dpi = design::effective_dpi(GetDpiForWindow(window)); let scale = |value: i32| value*dpi/96;
    let mut bounds = RECT::default(); GetClientRect(window, &mut bounds);
    let right = bounds.right*96/dpi-16; let bottom = bounds.bottom*96/dpi-12;
    let place = |id: i32, area: RECT| { MoveWindow(GetDlgItem(window, id), scale(area.left), scale(area.top), scale((area.right-area.left).max(1)), scale((area.bottom-area.top).max(1)), 1); };
    place(9, rect(16, 12, right, 36));
    place(QUERY, rect(16, 44, right, 80));
    place(12, rect(16, 88, right, bottom-44));
    if GetDlgItem(window,CLEAR).is_null(){
        place(IDOK,rect(right-208,bottom-36,right-108,bottom));
        place(IDCANCEL,rect(right-100,bottom-36,right,bottom));
    }else{
        let width = (right-16-16)/3;
        place(IDOK, rect(16, bottom-36, 16+width, bottom));
        place(CLEAR, rect(24+width, bottom-36, 24+width*2, bottom));
        place(IDCANCEL, rect(32+width*2, bottom-36, right, bottom));
    }
}
unsafe extern "system" fn procedure(window: HWND, message: u32, parameter: WPARAM, data: LPARAM) -> LRESULT {
    if message == WM_NCCREATE { SetWindowLongPtrW(window, GWLP_USERDATA, (*(data as *const CREATESTRUCTW)).lpCreateParams as isize); }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut QueryDialog;
    if pointer.is_null() { return DefWindowProcW(window, message, parameter, data); }
    let state = &mut *pointer;
    match message {
        WM_SIZE => { layout(window); 0 }
        WM_COMMAND => { if parameter >> 16 == BN_CLICKED as usize { state.command((parameter & 65535) as i32); } 0 }
        WM_CLOSE => { state.done = true; 0 }
        WM_NCDESTROY => { state.done = true; SetWindowLongPtrW(window, GWLP_USERDATA, 0); DefWindowProcW(window, message, parameter, data) }
        WM_DRAWITEM => modal_button(data),
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT => { SetTextColor(parameter as HDC, design::resolve(TEXT)); SetBkColor(parameter as HDC, design::resolve(CARD)); state.brush as isize }
        WM_ERASEBKGND => { let mut bounds = RECT::default(); GetClientRect(window, &mut bounds); FillRect(parameter as HDC, &bounds, state.brush); 1 }
        _ => DefWindowProcW(window, message, parameter, data),
    }
}

/// None cancels; Some("") explicitly removes the query filter.
pub(super) unsafe fn show(owner: HWND, current: &str) -> Option<String> { show_with(owner, current, None) }
unsafe fn show_with(owner: HWND, current: &str, inspect: Option<&mut dyn FnMut(HWND, &mut QueryDialog)>) -> Option<String> {
    show_labeled(owner,current,"Search planner","Search series, cars or tracks","All search terms must match.\nSearch covers cars and tracks across the entire season.",inspect)
}
pub(super) unsafe fn name(owner:HWND,current:&str,title:&str)->Option<String>{show_labeled(owner,current,title,"Name","Choose a short, recognizable name.",None).filter(|name|!name.is_empty())}
unsafe fn show_labeled(owner: HWND, current: &str, title:&str, label:&str, hint:&str, inspect: Option<&mut dyn FnMut(HWND, &mut QueryDialog)>) -> Option<String> {
    if owner.is_null() || IsWindow(owner) == 0 || IsWindowEnabled(owner) == 0 { return None; }
    let previous_focus = GetFocus(); let owner_enabled = IsWindowEnabled(owner);
    let instance = GetModuleHandleW(null()); let class = wide("PaddockPlannerQuery");
    let description = WNDCLASSW { hInstance: instance, lpszClassName: class.as_ptr(), lpfnWndProc: Some(procedure), hCursor: LoadCursorW(null_mut(), IDC_ARROW), ..WNDCLASSW::default() };
    RegisterClassW(&description);
    let dpi = design::effective_dpi(GetDpiForWindow(owner)); let scale = |value: i32| value*dpi/96;
    let mut monitor = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..MONITORINFO::default() };
    if GetMonitorInfoW(MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST), &mut monitor) == 0 { return None; }
    let width = scale(560).min(monitor.rcWork.right-monitor.rcWork.left-16).max(1);
    let height = scale(240).min(monitor.rcWork.bottom-monitor.rcWork.top-16).max(1);
    let mut state = Box::new(QueryDialog { edit: null_mut(), font: ui_font(scale(FONT_BODY), false), brush: CreateSolidBrush(design::resolve(CARD)), result: None, done: false });
    let window = CreateWindowExW(WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT, class.as_ptr(), wide(title).as_ptr(), WS_CAPTION | WS_SYSMENU,
        monitor.rcWork.left+(monitor.rcWork.right-monitor.rcWork.left-width)/2, monitor.rcWork.top+(monitor.rcWork.bottom-monitor.rcWork.top-height)/2,
        width, height, owner, null_mut(), instance, &mut *state as *mut QueryDialog as _);
    if window.is_null() { DeleteObject(state.font); DeleteObject(state.brush); return None; }
    let font = state.font;
    let child = |kind: &str, label: &str, style: u32, id: i32| {
        let control = CreateWindowExW(0, wide(kind).as_ptr(), wide(label).as_ptr(), WS_CHILD | WS_VISIBLE | style, 0, 0, 1, 1, window, id as usize as HMENU, instance, null());
        SendMessageW(control, WM_SETFONT, font as usize, 1); control
    };
    child("STATIC", label, 0, 9);
    state.edit = child("EDIT", current, WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32, QUERY);
    child("STATIC", hint, 0, 12);
    child("BUTTON", if title=="Search planner"{"Apply"}else{"Save"}, WS_TABSTOP | BS_OWNERDRAW as u32, IDOK);
    if title=="Search planner"{child("BUTTON", "Clear & apply", WS_TABSTOP | BS_OWNERDRAW as u32, CLEAR);}
    child("BUTTON", "Cancel", WS_TABSTOP | BS_OWNERDRAW as u32, IDCANCEL);
    if state.edit.is_null() { DestroyWindow(window); DeleteObject(state.font); DeleteObject(state.brush); return None; }
    SendMessageW(state.edit, EM_SETLIMITTEXT, 512, 0);
    layout(window); EnableWindow(owner, 0); apply_window_theme(window); ShowWindow(window, SW_SHOW);
    SetFocus(state.edit); SendMessageW(state.edit, EM_SETSEL, 0, -1);
    if let Some(inspect) = inspect { inspect(window, &mut state); }
    let mut message = MSG::default();
    while !state.done {
        let result = GetMessageW(&mut message, null_mut(), 0, 0);
        if result <= 0 { if result == 0 { PostQuitMessage(message.wParam as i32); } break; }
        let ours = message.hwnd == window || IsChild(window, message.hwnd) != 0;
        if ours && message.message == WM_KEYDOWN {
            if composing(GetFocus()) { TranslateMessage(&message); DispatchMessageW(&message); continue; }
            if message.wParam == VK_ESCAPE as usize { state.done = true; continue; }
            if message.wParam == VK_RETURN as usize {
                let focused = GetDlgCtrlID(GetFocus());
                state.command(if focused == CLEAR || focused == IDCANCEL { focused } else { IDOK }); continue;
            }
        }
        if !ours || IsDialogMessageW(window, &message) == 0 { TranslateMessage(&message); DispatchMessageW(&message); }
    }
    if IsWindow(owner) != 0 { EnableWindow(owner, owner_enabled); }
    if IsWindow(window) != 0 { DestroyWindow(window); }
    if owner_enabled != 0 && IsWindow(owner) != 0 {
        SetForegroundWindow(owner);
        SetFocus(if IsWindow(previous_focus) != 0 && (previous_focus == owner || IsChild(owner, previous_focus) != 0) { previous_focus } else { owner });
    }
    DeleteObject(state.font); DeleteObject(state.brush); state.result
}

pub(super) unsafe fn verify(owner: HWND) -> Vec<String> {
    let mut report = vec![];
    let mut inspect = |window: HWND, state: &mut QueryDialog| {
        report.push(format!("Planner query focuses native edit and keeps Korean text: {}", GetFocus() == state.edit && entered(state.edit) == "로드 애틀랜타"));
        report.push(format!("Planner query rejects nested dialogs and keeps owner disabled: {}", show(owner, "nested").is_none() && IsWindowEnabled(owner) == 0));
        SetWindowTextW(state.edit, wide("  GR86 Atlanta  ").as_ptr());
        SendMessageW(window, WM_COMMAND, IDOK as usize, 0);
    };
    let accepted = show_with(owner, "로드 애틀랜타", Some(&mut inspect));
    report.push(format!("Planner query applies trimmed text and restores owner: {}", accepted.as_deref() == Some("GR86 Atlanta") && IsWindowEnabled(owner) != 0));
    let cleared = show_with(owner, "GR86", Some(&mut |window, _| { SendMessageW(window, WM_COMMAND, CLEAR as usize, 0); }));
    report.push(format!("Planner query clear is an explicit empty filter: {}", cleared == Some(String::new())));
    let cancelled = show_with(owner, "GR86", Some(&mut |window, _| { SendMessageW(window, WM_CLOSE, 0, 0); }));
    report.push(format!("Planner query close cancels without changing the filter: {}", cancelled.is_none()));
    report
}
