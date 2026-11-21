//! Native window + local WebView2 renderer; no localhost server or remote page is used.
use crate::{
    app_control::{self, Phase},
    bridge::{Bridge, Reply, Request},
    storage::wide,
};
use include_dir::{include_dir, Dir};
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use std::{
    borrow::Cow,
    num::NonZeroIsize,
    path::PathBuf,
    ptr::{null, null_mut},
    sync::mpsc::{self, Receiver},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::{
        Com::*,
        LibraryLoader::GetModuleHandleW,
        Threading::{CreateMutexW, ReleaseMutex},
    },
    UI::{
        Controls::Dialogs::*, HiDpi::*, Input::KeyboardAndMouse::IsWindowEnabled, Shell::*,
        WindowsAndMessaging::*,
    },
};
use wry::{WebContext, WebView, WebViewBuilder};
thread_local! {static NATIVE_MODAL:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
const TRAY: u32 = WM_APP + 1;
const RESTORE: u32 = WM_APP + 2;
const COMMAND: u32 = WM_APP + 3;
static ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/frontend/dist");
struct NativeWindow(HWND);
struct OwnedWindow(HWND);
impl Drop for OwnedWindow {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(self.0) != 0 {
                SetWindowLongPtrW(self.0, GWLP_USERDATA, 0);
                DestroyWindow(self.0);
            }
        }
    }
}
impl HasWindowHandle for NativeWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let value = Win32WindowHandle::new(
            NonZeroIsize::new(self.0 as isize).ok_or(HandleError::Unavailable)?,
        );
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(value)) })
    }
}
struct Desktop {
    bridge: Bridge,
    view: Option<WebView>,
    requests: Receiver<Request>,
    tray: bool,
    auto_hidden: bool,
    last_state: Option<String>,
}
impl Desktop {
    unsafe fn restore(&mut self, window: HWND) {
        self.auto_hidden = true;
        ShowWindow(window, SW_RESTORE);
        SetForegroundWindow(window);
    }
    unsafe fn hide(&mut self, window: HWND) {
        if !self.tray {
            self.tray = notify(window, NIM_ADD);
        }
        if self.tray {
            ShowWindow(window, SW_HIDE);
        } else {
            self.bridge.message = "Notification area unavailable. Keeping Paddock visible.".into();
        }
    }
    fn send(&self, reply: Reply) {
        if let Some(view) = &self.view {
            if let Ok(json) = serde_json::to_string(&reply) {
                let _ = view.evaluate_script(&format!("window.__paddockReply?.({json})"));
            }
        }
    }
    fn push(&mut self) {
        let value = self.bridge.snapshot();
        if let Ok(json) = serde_json::to_string(&value) {
            if self.last_state.as_ref() != Some(&json) {
                if let Some(view) = &self.view {
                    let _ = view.evaluate_script(&format!(
                        "window.dispatchEvent(new CustomEvent('paddock:state',{{detail:{json}}}))"
                    ));
                }
                self.last_state = Some(json);
            }
        }
    }
    unsafe fn process(&mut self, window: HWND, request: Request) {
        let id = request.id;
        if !matches!(request.command.as_str(), "start" | "state")
            && matches!(
                self.bridge.controller.phase,
                Phase::Starting | Phase::Running
            )
        {
            self.auto_hidden = true;
        }
        let result = match request.command.as_str() {
            "app.add" | "app.browse" => {
                if let Err(error) = self.bridge.editable() {
                    Err(error)
                } else {
                    let replace = if request.command == "app.browse" {
                        request
                            .args
                            .get("index")
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|i| usize::try_from(i).ok())
                    } else {
                        None
                    };
                    if request.command == "app.browse"
                        && replace.is_none_or(|index| index >= self.bridge.store.config.apps.len())
                    {
                        Err("Unknown app".into())
                    } else {
                        let current = replace
                            .map(|index| self.bridge.store.config.apps[index].path.as_str())
                            .unwrap_or("");
                        match choose_exe(window, current) {
                            Some(path) => self.bridge.selected_path(path, replace),
                            None => Ok(self.bridge.snapshot()),
                        }
                    }
                }
            }
            "window.hide" => {
                self.hide(window);
                Ok(self.bridge.snapshot())
            }
            "window.activity" => Ok(self.bridge.snapshot()),
            "data.folder" => {
                open(window, &self.bridge.store.root.to_string_lossy());
                Ok(self.bridge.snapshot())
            }
            "schedule.source" => {
                open(window, crate::season_schedule::SOURCE);
                Ok(self.bridge.snapshot())
            }
            "learn.open" => {
                const URLS: [&str; 3] = [
                    "https://www.iracing.com/iracing-101-the-iracing-driving-school/",
                    "https://www.dannylee.co.uk/course",
                    "https://github.com/mitchwarrenburg/input-telemetry-overlay",
                ];
                match request
                    .args
                    .get("index")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|index| URLS.get(index as usize))
                {
                    Some(url) => {
                        open(window, url);
                        Ok(self.bridge.snapshot())
                    }
                    None => Err("Unknown resource".into()),
                }
            }
            "start" => {
                self.auto_hidden = false;
                self.bridge.command(&request.command, &request.args)
            }
            _ => {
                let reply = self.bridge.handle(request);
                self.send(reply);
                self.push();
                return;
            }
        };
        let reply = match result {
            Ok(value) => Reply {
                id,
                result: Some(value),
                error: None,
            },
            Err(error) => {
                self.bridge.message = error.clone();
                Reply {
                    id,
                    result: None,
                    error: Some(error),
                }
            }
        };
        self.send(reply);
        self.push();
    }
}
unsafe fn open(window: HWND, target: &str) {
    ShellExecuteW(
        window,
        wide("open").as_ptr(),
        wide(target).as_ptr(),
        null(),
        null(),
        SW_SHOWNORMAL,
    );
}
unsafe fn notify(window: HWND, command: u32) -> bool {
    let mut icon: NOTIFYICONDATAW = std::mem::zeroed();
    icon.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    icon.hWnd = window;
    icon.uID = 1;
    icon.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    icon.uCallbackMessage = TRAY;
    icon.hIcon = LoadIconW(GetModuleHandleW(null()), 1 as _);
    let label = wide("Paddock · iRacing Companion");
    icon.szTip[..label.len()].copy_from_slice(&label);
    Shell_NotifyIconW(command, &icon) != 0
}
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    parameter: WPARAM,
    data: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        SetWindowLongPtrW(
            window,
            GWLP_USERDATA,
            (*(data as *const CREATESTRUCTW)).lpCreateParams as isize,
        );
    }
    // Native dialogs/menus pump messages recursively. Keep the presentation state
    // exclusively borrowed by the initiating command until the modal returns.
    if NATIVE_MODAL.with(|value| value.get()) {
        return match message {
            WM_TIMER | COMMAND | WM_CLOSE => 0,
            WM_ERASEBKGND => 1,
            _ => DefWindowProcW(window, message, parameter, data),
        };
    }
    let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Desktop;
    if pointer.is_null() {
        return DefWindowProcW(window, message, parameter, data);
    }
    let state = &mut *pointer;
    match message {
        WM_SETFOCUS => {
            if let Some(view) = &state.view {
                let _ = view.focus();
            }
            0
        }
        WM_ACTIVATE
            if parameter as u32 & 65535 != WA_INACTIVE
                && matches!(
                    state.bridge.controller.phase,
                    Phase::Starting | Phase::Running
                ) =>
        {
            state.auto_hidden = true;
            DefWindowProcW(window, message, parameter, data)
        }
        WM_SIZE => {
            if let Some(view) = &state.view {
                let _ = view.set_bounds(wry::Rect {
                    position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                    size: wry::dpi::PhysicalSize::new(
                        (data as u32 & 65535).max(1),
                        (data as u32 >> 16).max(1),
                    )
                    .into(),
                });
            }
            0
        }
        WM_DPICHANGED => {
            let bounds = &*(data as *const RECT);
            SetWindowPos(
                window,
                null_mut(),
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            0
        }
        WM_GETMINMAXINFO => {
            let value = &mut *(data as *mut MINMAXINFO);
            let scale = GetDpiForWindow(window).max(96) as i32;
            value.ptMinTrackSize = POINT {
                x: 360 * scale / 96,
                y: 420 * scale / 96,
            };
            0
        }
        WM_ERASEBKGND => 1,
        COMMAND => {
            while let Ok(request) = state.requests.try_recv() {
                state.process(window, request);
            }
            0
        }
        WM_TIMER => {
            let phase = state.bridge.controller.phase;
            state.bridge.tick();
            if phase == Phase::Stopping && state.bridge.controller.phase == Phase::Idle {
                state.restore(window);
            }
            if state.bridge.controller.phase == Phase::Running
                && state.bridge.store.config.launcher_to_tray
                && !state.auto_hidden
                && IsWindowEnabled(window) != 0
                && state.bridge.controller.start_time.elapsed().as_secs() >= 10
                && state.bridge.store.config.apps.iter().any(|app| {
                    app.enabled
                        && state
                            .bridge
                            .controller
                            .status
                            .get(&app.id)
                            .copied()
                            .unwrap_or(0)
                            > 0
                })
            {
                state.hide(window);
                state.auto_hidden = true;
            }
            state.push();
            0
        }
        RESTORE => {
            state.restore(window);
            0
        }
        WM_SYSCOMMAND if parameter & 0xfff0 == SC_MINIMIZE as usize => {
            state.hide(window);
            0
        }
        TRAY => {
            match data as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => state.restore(window),
                WM_RBUTTONUP => {
                    let menu = CreatePopupMenu();
                    AppendMenuW(menu, MF_STRING, 1, wide("Open Paddock").as_ptr());
                    AppendMenuW(menu, MF_STRING, 2, wide("Launch all").as_ptr());
                    AppendMenuW(menu, MF_STRING, 3, wide("Stop all included apps").as_ptr());
                    AppendMenuW(menu, MF_SEPARATOR, 0, null());
                    AppendMenuW(
                        menu,
                        MF_STRING
                            | if state.bridge.controller.phase == Phase::Idle {
                                0
                            } else {
                                MF_GRAYED
                            },
                        4,
                        wide("Quit Paddock").as_ptr(),
                    );
                    let mut point = POINT::default();
                    GetCursorPos(&mut point);
                    SetForegroundWindow(window);
                    NATIVE_MODAL.with(|value| value.set(true));
                    let chosen = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_RIGHTBUTTON,
                        point.x,
                        point.y,
                        0,
                        window,
                        null(),
                    );
                    NATIVE_MODAL.with(|value| value.set(false));
                    PostMessageW(window, COMMAND, 0, 0);
                    DestroyMenu(menu);
                    PostMessageW(window, WM_NULL, 0, 0);
                    match chosen {
                        1 => state.restore(window),
                        2 | 3 => {
                            let command = if chosen == 2 { "start" } else { "stop" };
                            let _ = state.bridge.command(command, &serde_json::json!({}));
                        }
                        4 if state.bridge.controller.phase == Phase::Idle => {
                            DestroyWindow(window);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            if state.bridge.controller.phase == Phase::Idle {
                DestroyWindow(window);
            } else {
                state.hide(window);
            }
            0
        }
        WM_DESTROY => {
            notify(window, NIM_DELETE);
            KillTimer(window, 1);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(window, message, parameter, data),
    }
}
fn local_url(url: &str) -> bool {
    url.starts_with("http://paddock.localhost/")
        || url.starts_with("https://paddock.localhost/")
        || url.starts_with("paddock://localhost/")
}
pub fn run(root: PathBuf) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
        let mutex = CreateMutexW(null(), 1, wide("Local\\PaddockPersonalLauncher").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mutex);
            let mut window = FindWindowW(wide("PaddockWebWindow").as_ptr(), null());
            if window.is_null() {
                window = FindWindowW(wide("PaddockWindow").as_ptr(), null());
            }
            if !window.is_null() {
                PostMessageW(window, RESTORE, 0, 0);
            }
            return Ok(());
        }
        let result = (|| {
            let instance = GetModuleHandleW(null());
            let class = wide("PaddockWebWindow");
            let description = WNDCLASSW {
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                lpfnWndProc: Some(procedure),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hIcon: LoadIconW(instance, 1 as _),
                ..WNDCLASSW::default()
            };
            RegisterClassW(&description);
            let (tx, rx) = mpsc::channel();
            let bridge = Bridge::open(root.clone())?;
            let mut state = Box::new(Desktop {
                bridge,
                view: None,
                requests: rx,
                tray: false,
                auto_hidden: false,
                last_state: None,
            });
            let screen_width = GetSystemMetrics(SM_CXSCREEN);
            let screen_height = GetSystemMetrics(SM_CYSCREEN);
            let width = 1360.min(screen_width - 64).max(360);
            let height = 960.min(screen_height - 80).max(420);
            let window = CreateWindowExW(
                WS_EX_APPWINDOW,
                class.as_ptr(),
                wide("Paddock · iRacing Companion").as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                (screen_width - width) / 2,
                (screen_height - height) / 2,
                width,
                height,
                null_mut(),
                null_mut(),
                instance,
                &mut *state as *mut Desktop as _,
            );
            if window.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let _owned_window = OwnedWindow(window);
            let dark = 1i32;
            DwmSetWindowAttribute(
                window,
                DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                &dark as *const _ as _,
                4,
            );
            let mut context = WebContext::new(Some(root.join("webview")));
            let native = NativeWindow(window);
            let handle = window as usize;
            let view = WebViewBuilder::new_with_web_context(&mut context)
                .with_url("paddock://localhost/")
                .with_background_color((18, 18, 18, 255))
                .with_devtools(cfg!(debug_assertions))
                .with_hotkeys_zoom(true)
                .with_custom_protocol("paddock".into(), |_, request| {
                    let path = request.uri().path().trim_start_matches('/');
                    let path = if path.is_empty() { "index.html" } else { path };
                    let file = if path.contains("..") || path.contains('\\') {
                        None
                    } else {
                        ASSETS.get_file(path)
                    };
                    let mime = if path.ends_with(".html") {
                        "text/html; charset=utf-8"
                    } else if path.ends_with(".js") {
                        "text/javascript; charset=utf-8"
                    } else if path.ends_with(".css") {
                        "text/css; charset=utf-8"
                    } else if path.ends_with(".woff2") {
                        "font/woff2"
                    } else {
                        "application/octet-stream"
                    };
                    wry::http::Response::builder()
                        .status(if file.is_some() { 200 } else { 404 })
                        .header("Content-Type", mime)
                        .header("X-Content-Type-Options", "nosniff")
                        .body(Cow::Borrowed(
                            file.map(|file| file.contents()).unwrap_or(b"Not found"),
                        ))
                        .unwrap()
                })
                .with_navigation_handler(|url| local_url(&url))
                .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
                .with_ipc_handler(move |request| {
                    if local_url(&request.uri().to_string()) {
                        if let Ok(request) = serde_json::from_str::<Request>(request.body()) {
                            let _ = tx.send(request);
                            PostMessageW(handle as HWND, COMMAND, 0, 0);
                        }
                    }
                })
                .build_as_child(&native)
                .map_err(|error| format!("Unable to start WebView2: {error}"))?;
            state.view = Some(view);
            let mut bounds = RECT::default();
            GetClientRect(window, &mut bounds);
            state
                .view
                .as_ref()
                .unwrap()
                .set_bounds(wry::Rect {
                    position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                    size: wry::dpi::PhysicalSize::new(bounds.right, bounds.bottom).into(),
                })
                .map_err(|error| error.to_string())?;
            state.tray = notify(window, NIM_ADD);
            SetTimer(window, 1, 750, None);
            ShowWindow(window, SW_SHOW);
            UpdateWindow(window);
            app_control::log(
                &root,
                "Paddock React + Astryx renderer ready (local WebView2)",
            );
            let mut message = MSG::default();
            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            state.view.take();
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            Ok(())
        })();
        ReleaseMutex(mutex);
        CloseHandle(mutex);
        CoUninitialize();
        result
    }
}
unsafe fn choose_exe(window: HWND, current: &str) -> Option<String> {
    let mut buffer = vec![0u16; 32768];
    let value = wide(current);
    let count = value.len().min(buffer.len());
    buffer[..count].copy_from_slice(&value[..count]);
    let filter = wide("Executable (*.exe)\0*.exe\0\0");
    let mut dialog: OPENFILENAMEW = std::mem::zeroed();
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = window;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.lpstrFile = buffer.as_mut_ptr();
    dialog.nMaxFile = buffer.len() as u32;
    dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    NATIVE_MODAL.with(|value| value.set(true));
    let chosen = GetOpenFileNameW(&mut dialog);
    NATIVE_MODAL.with(|value| value.set(false));
    PostMessageW(window, COMMAND, 0, 0);
    if chosen == 0 {
        None
    } else {
        Some(String::from_utf16_lossy(
            &buffer[..buffer.iter().position(|&value| value == 0).unwrap()],
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_embedded_app_origin_is_privileged() {
        assert!(local_url("http://paddock.localhost/"));
        assert!(local_url("paddock://localhost/index.html"));
        for value in [
            "https://example.com/",
            "http://paddock.localhost.evil/",
            "file:///C:/test.html",
            "javascript:alert(1)",
        ] {
            assert!(!local_url(value));
        }
    }
}
