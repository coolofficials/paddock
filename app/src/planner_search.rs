use super::*;
use windows_sys::Win32::UI::Controls::{EM_SETCUEBANNER, EM_SETLIMITTEXT, EM_SETMARGINS, EM_SETSEL};
use windows_sys::Win32::UI::Input::Ime::{ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext, GCS_COMPSTR};

const ID: i32 = 4010;
const SUBCLASS: usize = ID as usize;
type CleanupProbe = std::rc::Rc<std::cell::Cell<Option<(bool, bool)>>>;
struct Editor { owner: HWND, font: HFONT, brush: HBRUSH, dpi: i32, foreground: u32, background: u32, syncing: bool, composing: bool, cleanup: Option<CleanupProbe> }

unsafe fn state(edit: HWND) -> *mut Editor {
    let mut pointer = 0;
    if !edit.is_null() && GetWindowSubclass(edit, Some(edit_proc), SUBCLASS, &mut pointer) != 0 { pointer as *mut Editor } else { null_mut() }
}
unsafe fn entered(edit: HWND) -> String {
    let length = GetWindowTextLengthW(edit).clamp(0, 512) as usize;
    let mut buffer = vec![0u16; length+1];
    let copied = GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..copied.max(0) as usize])
}
unsafe fn composing(edit: HWND, pointer: *mut Editor) -> bool {
    if (*pointer).composing { return true; }
    let context = ImmGetContext(edit);
    if context.is_null() { return false; }
    let active = ImmGetCompositionStringW(context, GCS_COMPSTR, null_mut(), 0) > 0;
    ImmReleaseContext(edit, context); active
}
unsafe extern "system" fn owner_proc(owner: HWND, message: u32, parameter: WPARAM, data: LPARAM, _: usize, _: usize) -> LRESULT {
    if message == WM_CTLCOLOREDIT && data as HWND == GetDlgItem(owner, ID) {
        let pointer = state(data as HWND);
        if !pointer.is_null() {
            SetTextColor(parameter as HDC, (*pointer).foreground);
            SetBkColor(parameter as HDC, (*pointer).background);
            return (*pointer).brush as isize;
        }
    }
    if message == WM_NCDESTROY { RemoveWindowSubclass(owner, Some(owner_proc), SUBCLASS); }
    DefSubclassProc(owner, message, parameter, data)
}
unsafe extern "system" fn edit_proc(edit: HWND, message: u32, parameter: WPARAM, data: LPARAM, _: usize, reference: usize) -> LRESULT {
    let pointer = reference as *mut Editor;
    match message {
        WM_NCDESTROY => {
            RemoveWindowSubclass(edit, Some(edit_proc), SUBCLASS);
            if IsWindow((*pointer).owner) != 0 { RemoveWindowSubclass((*pointer).owner, Some(owner_proc), SUBCLASS); }
            let result = DefSubclassProc(edit, message, parameter, data);
            let editor = Box::from_raw(pointer);
            let font_deleted = editor.font.is_null() || DeleteObject(editor.font) != 0;
            let brush_deleted = editor.brush.is_null() || DeleteObject(editor.brush) != 0;
            if let Some(probe) = &editor.cleanup { probe.set(Some((font_deleted, brush_deleted))); }
            return result;
        }
        WM_IME_STARTCOMPOSITION => (*pointer).composing = true,
        WM_IME_ENDCOMPOSITION => (*pointer).composing = false,
        WM_KEYDOWN if !composing(edit, pointer) => {
            if parameter == VK_RETURN as usize || parameter == VK_ESCAPE as usize {
                SetFocus((*pointer).owner); return 0;
            }
            if parameter == VK_TAB as usize {
                let owner = (*pointer).owner;
                SetFocus(owner); PostMessageW(owner, WM_KEYDOWN, parameter, data); return 0;
            }
            if parameter == b'A' as usize && GetKeyState(VK_CONTROL as i32) < 0 {
                SendMessageW(edit, EM_SETSEL, 0, -1); return 0;
            }
        }
        WM_CHAR if !composing(edit, pointer) && (parameter == VK_RETURN as usize || parameter == VK_ESCAPE as usize || parameter == VK_TAB as usize || parameter == 1) => return 0,
        _ => {}
    }
    DefSubclassProc(edit, message, parameter, data)
}

/// Positions a real EDIT in logical coordinates. None hides it without losing its text.
pub(super) unsafe fn sync(owner: HWND, area: Option<RECT>, query: &str) {
    if owner.is_null() || IsWindow(owner) == 0 { return; }
    let mut edit = GetDlgItem(owner, ID);
    let Some(area) = area.filter(|area| area.right > area.left && area.bottom > area.top) else {
        if !edit.is_null() {
            if GetFocus() == edit { SetFocus(owner); }
            ShowWindow(edit, SW_HIDE);
        }
        return;
    };
    if edit.is_null() {
        edit = CreateWindowExW(0, wide("EDIT").as_ptr(), wide("").as_ptr(), WS_CHILD | WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            0, 0, 1, 1, owner, ID as usize as HMENU, GetModuleHandleW(null()), null());
        if edit.is_null() { return; }
        let pointer = Box::into_raw(Box::new(Editor { owner, font: null_mut(), brush: null_mut(), dpi: 0, foreground: 0, background: 0, syncing: false, composing: false, cleanup: None }));
        if SetWindowSubclass(edit, Some(edit_proc), SUBCLASS, pointer as usize) == 0 {
            drop(Box::from_raw(pointer)); DestroyWindow(edit); return;
        }
        if SetWindowSubclass(owner, Some(owner_proc), SUBCLASS, 0) == 0 { DestroyWindow(edit); return; }
        SendMessageW(edit, EM_SETLIMITTEXT, 512, 0);
        SendMessageW(edit, EM_SETCUEBANNER, 1, wide("Search series, cars or tracks  ·  Ctrl+K").as_ptr() as isize);
    }
    let pointer = state(edit);
    if pointer.is_null() { return; }
    let dpi = design::effective_dpi(GetDpiForWindow(owner));
    if (*pointer).dpi != dpi {
        let font = ui_font(FONT_BODY*dpi/96, false);
        if !font.is_null() {
            let previous = (*pointer).font; (*pointer).font = font;
            SendMessageW(edit, WM_SETFONT, font as usize, 1);
            if !previous.is_null() { DeleteObject(previous); }
        }
        let margin = 10*dpi/96;
        SendMessageW(edit, EM_SETMARGINS, (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize, (margin | margin << 16) as isize);
        (*pointer).dpi = dpi;
    }
    let foreground = design::resolve(TEXT); let background = design::resolve(CARD);
    if (*pointer).foreground != foreground || (*pointer).background != background || (*pointer).brush.is_null() {
        let brush = CreateSolidBrush(background);
        if !brush.is_null() {
            let previous = (*pointer).brush; (*pointer).brush = brush;
            (*pointer).foreground = foreground; (*pointer).background = background;
            if !previous.is_null() { DeleteObject(previous); }
            InvalidateRect(edit, null(), 1);
        }
    }
    let value: String = query.chars().scan(0usize, |units, character| { *units += character.len_utf16(); (*units <= 512).then_some(character) }).collect();
    if !composing(edit, pointer) && entered(edit) != value {
        (*pointer).syncing = true;
        SetWindowTextW(edit, wide(&value).as_ptr());
        (*pointer).syncing = false;
    }
    SetWindowPos(edit, null_mut(), area.left*dpi/96, area.top*dpi/96, (area.right-area.left)*dpi/96, (area.bottom-area.top)*dpi/96,
        SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW);
}

pub(super) unsafe fn focus(owner: HWND) {
    let edit = GetDlgItem(owner, ID);
    if !edit.is_null() && IsWindowEnabled(owner) != 0 && IsWindowVisible(edit) != 0 { SetFocus(edit); }
}

/// Call from the owner's WM_COMMAND; only genuine edit notifications return text.
pub(super) unsafe fn changed(owner: HWND, parameter: WPARAM, data: LPARAM) -> Option<String> {
    if (parameter & 65535) != ID as usize || parameter >> 16 != EN_CHANGE as usize { return None; }
    let edit = GetDlgItem(owner, ID);
    if edit.is_null() || data as HWND != edit { return None; }
    let pointer = state(edit);
    if pointer.is_null() || (*pointer).syncing { return None; }
    Some(entered(edit))
}

#[derive(Default)]
struct Verification { notifications: usize, suppressed: usize, accepted: Vec<String> }
unsafe extern "system" fn verify_proc(window: HWND, message: u32, parameter: WPARAM, data: LPARAM) -> LRESULT {
    if message == WM_NCCREATE { SetWindowLongPtrW(window, GWLP_USERDATA, (*(data as *const CREATESTRUCTW)).lpCreateParams as isize); }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Verification;
    if message == WM_COMMAND && !pointer.is_null() && (parameter & 65535) == ID as usize && parameter >> 16 == EN_CHANGE as usize {
        (*pointer).notifications += 1;
        if let Some(query) = changed(window, parameter, data) { (*pointer).accepted.push(query); }
        else { (*pointer).suppressed += 1; }
        return 0;
    }
    if message == WM_NCDESTROY { SetWindowLongPtrW(window, GWLP_USERDATA, 0); }
    DefWindowProcW(window, message, parameter, data)
}

/// An isolated owner receives real EDIT notifications; the app's query/config is untouched.
pub(super) unsafe fn verify(owner: HWND) -> Vec<String> {
    use windows_sys::Win32::UI::Controls::EM_GETSEL;
    let previous_focus = GetFocus();
    let instance = GetModuleHandleW(null()); let class = wide("PaddockPlannerSearchVerification");
    RegisterClassW(&WNDCLASSW { hInstance: instance, lpszClassName: class.as_ptr(), lpfnWndProc: Some(verify_proc), hCursor: LoadCursorW(null_mut(), IDC_ARROW), ..WNDCLASSW::default() });
    let mut observed = Box::<Verification>::default();
    let host = CreateWindowExW(WS_EX_TOOLWINDOW, class.as_ptr(), wide("Paddock inline search verification").as_ptr(), WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT, CW_USEDEFAULT, 640, 180, owner, null_mut(), instance, &mut *observed as *mut Verification as _);
    if host.is_null() { return vec!["Inline search verification host is available: false".into()]; }
    ShowWindow(host, SW_SHOW);
    let area = rect(12, 12, 360, 48);
    sync(host, Some(area), "로드 애틀랜타");
    let edit = GetDlgItem(host, ID);
    let mut report = vec![format!("Inline search shows initial Korean text and suppresses sync notifications: {}",
        !edit.is_null() && IsWindowVisible(edit) != 0 && entered(edit) == "로드 애틀랜타" && observed.notifications > 0 && observed.suppressed == observed.notifications && observed.accepted.is_empty())];
    if !edit.is_null() {
        let raw = "  GR86 로드 애틀랜타  ";
        let before = observed.notifications;
        SetWindowTextW(edit, wide(raw).as_ptr());
        report.push(format!("Inline search receives real EN_CHANGE and preserves input whitespace: {}",
            observed.notifications > before && observed.accepted.last().map(String::as_str) == Some(raw)));

        focus(host); SendMessageW(edit, EM_SETSEL, 2, 7);
        let before = observed.notifications;
        for _ in 0..4 { sync(host, Some(area), raw); focus(host); }
        let mut start = 0u32; let mut end = 0u32;
        SendMessageW(edit, EM_GETSEL, &mut start as *mut u32 as usize, &mut end as *mut u32 as isize);
        let mut children = 0; let mut child = GetWindow(host, GW_CHILD);
        while !child.is_null() { children += 1; child = GetWindow(child, GW_HWNDNEXT); }
        report.push(format!("Inline search redraw preserves caret and reuses one native edit: {}",
            GetFocus() == edit && start == 2 && end == 7 && GetDlgItem(host, ID) == edit && children == 1 && observed.notifications == before));

        sync(host, None, raw); focus(host);
        let hidden = IsWindowVisible(edit) == 0 && GetFocus() == host && IsWindowEnabled(host) != 0;
        sync(host, Some(area), raw); SetFocus(host); EnableWindow(host, 0); focus(host);
        let disabled = GetFocus() != edit;
        EnableWindow(host, 1); focus(host); SendMessageW(edit, WM_KEYDOWN, VK_ESCAPE as usize, 0);
        let escape = GetFocus() == host;
        focus(host); SendMessageW(edit, WM_KEYDOWN, VK_RETURN as usize, 0);
        report.push(format!("Inline search hide and keyboard exit restore enabled owner focus: {}",
            hidden && disabled && escape && GetFocus() == host && IsWindowEnabled(host) != 0));

        // GDI can still report an object's old type after successful DeleteObject.
        // Observe deletion results in the real destruction callback, not stale handles.
        let child_cleanup = CleanupProbe::default();
        let pointer = state(edit); let child_allocated = !(*pointer).font.is_null() && !(*pointer).brush.is_null();
        (*pointer).cleanup = Some(child_cleanup.clone());
        DestroyWindow(edit);
        let mut reference = 0;
        let removed = IsWindow(edit) == 0 && GetDlgItem(host, ID).is_null() && child_allocated && child_cleanup.get() == Some((true, true))
            && GetWindowSubclass(host, Some(owner_proc), SUBCLASS, &mut reference) == 0;
        sync(host, Some(area), "recreated");
        let recreated = GetDlgItem(host, ID); let pointer = state(recreated);
        let owner_cleanup = CleanupProbe::default();
        let owner_allocated = !pointer.is_null() && !(*pointer).font.is_null() && !(*pointer).brush.is_null();
        if !pointer.is_null() { (*pointer).cleanup = Some(owner_cleanup.clone()); }
        DestroyWindow(host);
        report.push(format!("Inline search child and owner destruction release controls and GDI objects: {}",
            removed && !recreated.is_null() && IsWindow(recreated) == 0 && owner_allocated && owner_cleanup.get() == Some((true, true))));
    }
    if IsWindow(host) != 0 { DestroyWindow(host); }
    if IsWindow(owner) != 0 && IsWindowEnabled(owner) != 0 {
        SetForegroundWindow(owner);
        SetFocus(if IsWindow(previous_focus) != 0 { previous_focus } else { owner });
    }
    report
}
