use crate::storage::{self, AppSpec, Config};
use chrono::Utc;
use std::{collections::{HashMap, HashSet}, fs, io::Write, path::Path, sync::mpsc::{self, Receiver}, thread, time::{Duration, Instant}};
use windows_sys::Win32::{Foundation::*, System::{Com::*, Diagnostics::ToolHelp::*, Threading::*}, UI::{Shell::*, WindowsAndMessaging::*}};

#[derive(Clone, Debug)]
pub struct Process {
    pub pid: u32,
    pub parent_pid: u32,
    pub path: String,
    pub created: u64,
}

pub fn process_identity(pid: u32) -> Option<Process> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() { return None; }
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        let mut created: FILETIME = std::mem::zeroed();
        let mut exit: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        let success = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) != 0 && GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) != 0;
        CloseHandle(handle);
        if !success { return None; }
        Some(Process { pid, parent_pid: 0, path: String::from_utf16_lossy(&buffer[..length as usize]), created: (created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64 })
    }
}

pub fn processes() -> Vec<Process> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE { return vec![]; }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut result = vec![];
        let mut next = Process32FirstW(snapshot, &mut entry);
        while next != 0 {
            if let Some(mut process) = process_identity(entry.th32ProcessID) { process.parent_pid = entry.th32ParentProcessID; result.push(process); }
            next = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        result
    }
}

#[derive(Clone, Debug)]
pub struct AppWindow { pub handle: usize, pub pid: u32, pub title: String, pub visible: bool, pub tool: bool, pub class_name: String, pub owner: usize, pub app_window: bool }
pub fn windows() -> Vec<AppWindow> {
    unsafe extern "system" fn collect(window: HWND, parameter: LPARAM) -> i32 {
        let result = &mut *(parameter as *mut Vec<AppWindow>);
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        let length = GetWindowTextLengthW(window);
        if GetWindowLongPtrW(window, GWL_STYLE) as u32 & WS_CHILD == 0 {
            let mut title = vec![0u16; length as usize + 1];
            let read = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
            let mut class_name = [0u16; 256]; let class_length = GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32);
            let extended = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
            result.push(AppWindow { handle: window as usize, pid, title: String::from_utf16_lossy(&title[..read as usize]), visible: IsWindowVisible(window) != 0, tool: extended & WS_EX_TOOLWINDOW != 0, class_name: String::from_utf16_lossy(&class_name[..class_length as usize]), owner: GetWindow(window, GW_OWNER) as usize, app_window: extended & WS_EX_APPWINDOW != 0 });
        }
        1
    }
    let mut result: Vec<AppWindow> = vec![];
    unsafe { EnumWindows(Some(collect), &mut result as *mut _ as _); }
    result
}

pub fn log(root: &Path, message: &str) {
    let directory = root.parent().unwrap_or(root).join("logs");
    let _ = fs::create_dir_all(&directory);
    if let Ok(mut file) = fs::OpenOptions::new().append(true).create(true).open(directory.join("launcher.log")) {
        let _ = writeln!(file, "{} {message}", Utc::now().to_rfc3339());
    }
}

fn main_window(spec: &AppSpec, window: &AppWindow) -> bool {
    if spec.id == "iracing" { return iracing_window(window); }
    if window.tool && !matches!(spec.id.as_str(), "moza" | "gofast" | "crew" | "paints" | "garage61") { return false; }
    if window.class_name == "IME" || window.class_name == "MSCTFIME UI" { return false; }
    let title = window.title.to_lowercase();
    if title.starts_with("gdi+") || title.starts_with(".net-") || title == "cspthreadtask window" || title == "systemresourcenotifywindow" || title == "mediacontextnotificationwindow" { return false; }
    match spec.id.as_str() {
        "iracing" => title == "iracing" || title.starts_with("iracing ui"),
        "edge" => title == "edge overlays" || title.starts_with("edge overlays -") || title == "free overlays",
        "gofast" => (title.contains("go fast") || title.contains("gofast")) && !title.contains("overlay") && !title.contains("hud"),
        "moza" => title.contains("moza") || title.contains("pit house"),
        "crew" => title.contains("crew chief") || title.contains("crewchief"),
        "paints" => title == "trading paints" || title.starts_with("trading paints "),
        "garage61" => title == "garage 61 telemetry agent" || title.starts_with("garage 61 telemetry agent -"),
        _ => !title.is_empty() && !title.contains("overlay") && !title.contains("hud"),
    }
}

fn iracing_window(window: &AppWindow) -> bool {
    let title = window.title.to_lowercase();
    (window.class_name == "Chrome_WidgetWin_1" || window.class_name == "PaddockWindow") && (title == "iracing" || title.starts_with("iracing ui"))
}

fn launch_iracing(spec: &AppSpec) -> Result<u32, String> {
    let executable = fs::canonicalize(&spec.path).map_err(|error| error.to_string())?;
    let working_dir = fs::canonicalize(&spec.working_dir).map_err(|error| error.to_string())?;
    unsafe {
        let initialized = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32 | COINIT_DISABLE_OLE1DDE as u32);
        let file = storage::wide(executable.to_string_lossy().trim_start_matches(r"\\?\")); let directory = storage::wide(working_dir.to_string_lossy().trim_start_matches(r"\\?\")); let verb = storage::wide("open");
        let mut execution: SHELLEXECUTEINFOW = std::mem::zeroed();
        execution.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        execution.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
        execution.lpVerb = verb.as_ptr(); execution.lpFile = file.as_ptr(); execution.lpDirectory = directory.as_ptr(); execution.nShow = SW_SHOWNORMAL;
        let result = if ShellExecuteExW(&mut execution) == 0 { Err(std::io::Error::last_os_error().to_string()) } else {
            let pid = if execution.hProcess.is_null() { 0 } else { let pid = GetProcessId(execution.hProcess); CloseHandle(execution.hProcess); pid };
            Ok(pid)
        };
        if initialized >= 0 { CoUninitialize(); }
        result
    }
}

fn launch_helper(spec: &AppSpec) -> Result<u32, String> {
    // Request a minimized, non-activating native startup before discovering the real main window.
    // SW_HIDE here can prevent some applications from creating their tray icon at all.
    unsafe {
        let file = storage::wide(&spec.path);
        let mut command = storage::wide(&format!("\"{}\"", spec.path));
        let directory = storage::wide(&spec.working_dir);
        let mut startup: STARTUPINFOW = std::mem::zeroed();
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        startup.dwFlags = STARTF_USESHOWWINDOW;
        startup.wShowWindow = if spec.tray { SW_SHOWMINNOACTIVE as u16 } else { SW_SHOWNORMAL as u16 };
        let mut info: PROCESS_INFORMATION = std::mem::zeroed();
        if CreateProcessW(file.as_ptr(), command.as_mut_ptr(), std::ptr::null(), std::ptr::null(), 0, CREATE_NO_WINDOW, std::ptr::null(), directory.as_ptr(), &startup, &mut info) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        CloseHandle(info.hThread); CloseHandle(info.hProcess);
        Ok(info.dwProcessId)
    }
}

fn ensure_iracing_taskbar(root: &Path, spec: &AppSpec, running: &[Process]) {
    for window in windows().into_iter().filter(|window| window.visible && iracing_window(window) && (!window.app_window || window.tool)) {
        if !running.iter().any(|process| process.pid == window.pid && spec.matches(&process.path)) { continue; }
        unsafe {
            let handle = window.handle as HWND;
            let previous = GetWindowLongPtrW(handle, GWL_EXSTYLE);
            let display = if IsIconic(handle) != 0 { SW_SHOWMINNOACTIVE } else { SW_SHOWNA };
            ShowWindow(handle, SW_HIDE);
            SetLastError(0);
            let updated = SetWindowLongPtrW(handle, GWL_EXSTYLE, (previous | WS_EX_APPWINDOW as isize) & !(WS_EX_TOOLWINDOW as isize));
            let error = GetLastError();
            SetWindowPos(handle, std::ptr::null_mut(), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
            ShowWindow(handle, display);
            if updated == 0 && error != 0 { log(root, &format!("iRacing taskbar update failed: {error}")); }
            else { log(root, "iRacing taskbar registration checked"); }
        }
    }
}

pub fn launch(root: &Path, specs: &[AppSpec]) -> Vec<String> {
    let mut messages = vec![];
    for spec in specs.iter().filter(|spec| spec.enabled) {
        if processes().iter().any(|process| spec.matches(&process.path)) {
            messages.push(format!("{}: using existing process", spec.name)); continue;
        }
        if !Path::new(&spec.path).is_file() { messages.push(format!("{}: unable to find executable", spec.name)); continue; }
        if !Path::new(&spec.working_dir).is_dir() { messages.push(format!("{}: unable to find working directory", spec.name)); continue; }
        let launched = if spec.id == "iracing" { launch_iracing(spec) } else { launch_helper(spec) };
        match launched {
            Ok(pid) => { messages.push(format!("{}: started PID {}", spec.name, pid)); }
            Err(error) => messages.push(format!("{}: launch failed ({error})", spec.name)),
        }
    }
    for message in &messages { log(root, message); }
    messages
}

fn terminate_exact(process: &Process, spec: &AppSpec) -> Result<(), String> {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, process.pid);
        if handle.is_null() { return Err(std::io::Error::last_os_error().to_string()); }
        let mut created: FILETIME = std::mem::zeroed();
        let mut exit: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        let mut buffer = vec![0u16; 32768]; let mut size = buffer.len() as u32;
        let valid = GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) != 0 && QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) != 0 && spec.matches(&String::from_utf16_lossy(&buffer[..size as usize])) && process.created == (created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64;
        let result = if valid && TerminateProcess(handle, 0) != 0 { WaitForSingleObject(handle, 2000); Ok(()) } else { Err(std::io::Error::last_os_error().to_string()) };
        CloseHandle(handle);
        result
    }
}

pub fn stop(root: &Path, specs: &[AppSpec]) -> Vec<String> {
    stop_scoped(root, specs, false)
}

pub fn stop_all(root: &Path, specs: &[AppSpec]) -> Vec<String> {
    stop_scoped(root, specs, true)
}

fn stop_scoped(root: &Path, specs: &[AppSpec], include_main: bool) -> Vec<String> {
    let mut specs=specs.to_vec();for spec in &mut specs{spec.add_known_processes();}
    let targets: Vec<_> = specs.iter().filter(|spec| spec.enabled && (include_main || spec.id != "iracing")).collect();
    let running = processes();
    let ids: HashSet<_> = running.iter().filter(|process| targets.iter().any(|spec| spec.matches(&process.path))).map(|process| process.pid).collect();
    for window in windows().into_iter().filter(|window| ids.contains(&window.pid) && targets.iter().any(|spec| running.iter().any(|process| process.pid == window.pid && spec.matches(&process.path)) && main_window(spec, window))) {
        unsafe { PostMessageW(window.handle as HWND, WM_CLOSE, 0, 0); }
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if !processes().iter().any(|process| targets.iter().any(|spec| spec.matches(&process.path))) { break; }
        thread::sleep(Duration::from_millis(250));
    }
    let mut messages = vec![];
    let force_deadline = Instant::now() + Duration::from_secs(20);
    let mut quiet_since = None;
    while Instant::now() < force_deadline {
        let current = processes();
        let mut remaining = false;
        for spec in &targets {
            for process in current.iter().filter(|process| spec.matches(&process.path)) {
                remaining = true;
                if let Some(identity) = process_identity(process.pid) {
                    if identity.created != process.created { continue; }
                    match terminate_exact(process, spec) {
                        Ok(()) => messages.push(format!("{}: stopped PID {} after timeout", spec.name, process.pid)),
                        Err(error) => if process_identity(process.pid).is_some() { messages.push(format!("{}: stop failed PID {} ({error})", spec.name, process.pid)); },
                    }
                }
            }
        }
        if remaining { quiet_since = None; } else {
            let since = quiet_since.get_or_insert(Instant::now());
            if since.elapsed() > Duration::from_secs(2) { break; }
        }
        thread::sleep(Duration::from_millis(250));
    }
    for spec in targets {
        let left: Vec<_> = processes().into_iter().filter(|process| spec.matches(&process.path)).collect();
        messages.push(format!("{}: {}", spec.name, if left.is_empty() { "stopped" } else { "processes remain" }));
    }
    for message in &messages { log(root, message); }
    messages
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase { Idle, Starting, Running, Stopping, CleanupFailed, Finished }
pub enum Event { Launched(Vec<String>), Stopped(Vec<String>) }

pub struct Controller {
    pub phase: Phase,
    pub messages: Vec<String>,
    pub status: HashMap<String, usize>,
    pub hidden: HashMap<usize, (Process, String)>,
    pub revealed: HashSet<String>,
    pub seen_main: bool,
    pub last_main: Instant,
    pub start_time: Instant,
    pub receiver: Option<Receiver<Event>>,
    pub session_specs: Vec<AppSpec>,
    pub shutdown_main: bool,
    stop_after_launch: Option<bool>,
    startup_tray_handled: HashSet<String>,
    hide_attempts: HashMap<String, u8>,
    sim_was_running: bool,
    hidden_during_game: HashSet<String>,
    game_return_deadlines: HashMap<String,Instant>,
    paints_window_ready: Option<(usize, Instant)>,
}

impl Controller {
    pub fn new() -> Self { Self { phase: Phase::Idle, messages: vec![], status: HashMap::new(), hidden: HashMap::new(), revealed: HashSet::new(), seen_main: false, last_main: Instant::now(), start_time: Instant::now(), receiver: None, session_specs: vec![], shutdown_main: false, stop_after_launch: None, startup_tray_handled: HashSet::new(), hide_attempts: HashMap::new(), sim_was_running: false, hidden_during_game: HashSet::new(), game_return_deadlines: HashMap::new(), paints_window_ready: None } }
    pub fn reset_finished(&mut self) {
        if self.phase == Phase::Finished { *self = Self::new(); }
    }
    pub fn start(&mut self, root: &Path, config: &Config) -> Result<(), String> {
        if !matches!(self.phase, Phase::Idle | Phase::Running) { return Err("Wait for the current operation to finish.".into()); }
        if !config.apps.iter().any(|spec|spec.enabled) { return Err("Include at least one app in this profile.".into()); }
        let continuing = self.phase == Phase::Running;
        self.phase = Phase::Starting; self.start_time = Instant::now(); self.last_main = Instant::now(); self.seen_main = false;
        self.session_specs = config.apps.clone();
        for spec in &mut self.session_specs {
            spec.add_known_processes();
            if let Ok(resolved) = fs::canonicalize(&spec.path) { spec.extra_paths.push(resolved.to_string_lossy().trim_start_matches(r"\\?\").to_string()); }
        }
        let running = processes();
        self.prepare_startup_state(continuing, &running);
        self.mark_existing_hidden(&running, &windows());
        let specs = self.session_specs.clone(); let root = root.to_owned();
        let (sender, receiver) = mpsc::channel(); self.receiver = Some(receiver);
        thread::spawn(move || { let messages = launch(&root, &specs); let _ = sender.send(Event::Launched(messages)); });
        Ok(())
    }
    pub fn stop_all(&mut self, root: &Path, config: &Config) {
        if self.session_specs.is_empty() { self.session_specs = config.apps.clone(); }
        self.begin_stop(root, true);
    }
    pub fn retry_stop(&mut self, root: &Path) { self.begin_stop(root, self.shutdown_main); }
    fn begin_stop(&mut self, root: &Path, include_main: bool) {
        if self.phase == Phase::Starting { self.stop_after_launch = Some(self.stop_after_launch.unwrap_or(false) || include_main); return; }
        if self.phase == Phase::Stopping || self.phase == Phase::Finished { return; }
        self.phase = Phase::Stopping;
        self.shutdown_main = include_main;
        let root = root.to_owned(); let specs = self.session_specs.clone();
        let (sender, receiver) = mpsc::channel(); self.receiver = Some(receiver);
        thread::spawn(move || { let messages = if include_main { stop_all(&root, &specs) } else { stop(&root, &specs) }; let _ = sender.send(Event::Stopped(messages)); });
    }
    pub fn tick(&mut self, root: &Path, specs: &[AppSpec]) {
        let events = self.receiver.as_ref().map(|receiver| receiver.try_iter().collect::<Vec<_>>()).unwrap_or_default();
        for event in events {
            match event {
                Event::Launched(messages) => { self.messages.extend(messages); self.phase = Phase::Running; if let Some(include_main) = self.stop_after_launch.take() { self.begin_stop(root, include_main); } }
                Event::Stopped(messages) => { self.phase = if messages.iter().any(|message| message.contains("processes remain")) { Phase::CleanupFailed } else { Phase::Finished }; self.messages.extend(messages); }
            }
        }
        let running = processes();
        self.status = specs.iter().map(|spec| (spec.id.clone(), running.iter().filter(|process| spec.matches(&process.path)).count())).collect();
        if self.phase != Phase::Running && self.phase != Phase::Starting { return; }
        let main_alive = self.session_specs.iter().find(|spec| spec.id == "iracing").map(|spec| running.iter().any(|process| spec.matches(&process.path))).unwrap_or(false);
        if main_alive { self.seen_main = true; self.last_main = Instant::now(); }
        if let Some(main) = self.session_specs.iter().find(|spec| spec.id == "iracing") { ensure_iracing_taskbar(root, main, &running); }
        // iRacing UI and simulator lifetime never initiate cleanup. Only an explicit Stop does.
        self.handle_sim_return(root, &running);
        self.hide_helper_windows(&running, true);
    }

    fn game_return_targets(&mut self, sim_running: bool, running: &[Process], available: &[AppWindow]) -> Vec<(AppWindow, Process, String)> {
        let ended=self.sim_was_running && !sim_running;
        self.sim_was_running=sim_running;
        let mut targets=vec![];
        for spec in self.session_specs.iter().filter(|spec|spec.enabled&&spec.tray&&spec.rehide_after_game) {
            let id=&spec.id;
            if self.revealed.contains(id) {self.game_return_deadlines.remove(id);continue;}
            let matched:Vec<_>=available.iter().filter(|window|main_window(spec,window)).filter_map(|window|running.iter().find(|process|process.pid==window.pid&&spec.matches(&process.path)).map(|process|(window,process))).collect();
            if sim_running {
                self.game_return_deadlines.remove(id);
                if !matched.is_empty()&&matched.iter().all(|(window,_)|!window.visible){self.hidden_during_game.insert(id.clone());}else{self.hidden_during_game.remove(id);}
                continue;
            }
            if ended&&self.hidden_during_game.remove(id) {self.game_return_deadlines.insert(id.clone(),Instant::now()+Duration::from_secs(15));}
            let Some(deadline)=self.game_return_deadlines.get(id)else{continue};
            if Instant::now()>*deadline{self.game_return_deadlines.remove(id);continue;}
            let visible:Vec<_>=matched.into_iter().filter(|(window,_)|window.visible).collect();
            if !visible.is_empty(){self.game_return_deadlines.remove(id);}
            for(window,process)in visible{targets.push((window.clone(),process.clone(),id.clone()));}
        }
        targets
    }
    fn handle_sim_return(&mut self, root: &Path, running: &[Process]) {
        let sim_running=running.iter().any(|process|Path::new(&process.path).file_name().is_some_and(|name|name.to_string_lossy().to_ascii_lowercase().starts_with("iracingsim")));
        for(window,process,id)in self.game_return_targets(sim_running,running,&windows()){
            unsafe{ShowWindow(window.handle as HWND,SW_HIDE);}
            if unsafe{IsWindowVisible(window.handle as HWND)}==0 {
                self.hidden.insert(window.handle,(process,id.clone()));
                log(root,&format!("{id}: applied optional game-exit window hide"));
            }
        }
    }
    pub fn hide_helpers(&mut self, running: &[Process]) {
        self.hide_helper_windows(running, false);
    }
    fn prepare_startup_state(&mut self, continuing: bool, running: &[Process]) {
        // Repeated Start must not re-hide windows the user has already reopened.
        // Closed apps lose their one-shot state so a new process gets its startup preference.
        let alive: HashSet<_> = self.session_specs.iter().filter(|spec| continuing && spec.enabled && running.iter().any(|process| spec.matches(&process.path))).map(|spec| spec.id.clone()).collect();
        self.startup_tray_handled.retain(|id| alive.contains(id));
        self.revealed.retain(|id| alive.contains(id));
        self.hide_attempts.retain(|id, _| alive.contains(id));
        self.hidden_during_game.retain(|id| alive.contains(id));
        self.game_return_deadlines.retain(|id, _| alive.contains(id));
        if !continuing { self.sim_was_running = false; }
        self.hidden.retain(|_, (saved, id)| alive.contains(id) && running.iter().any(|process| process.pid == saved.pid && process.created == saved.created && process.path.eq_ignore_ascii_case(&saved.path)));
        self.paints_window_ready = None;
    }
    fn mark_existing_hidden(&mut self, running: &[Process], available: &[AppWindow]) {
        for spec in self.session_specs.iter().filter(|spec| spec.enabled && spec.tray) {
            let matched: Vec<_> = available.iter().filter(|window| main_window(spec, window) && running.iter().any(|process| process.pid == window.pid && spec.matches(&process.path))).collect();
            if !matched.is_empty() && matched.iter().all(|window| !window.visible) { self.startup_tray_handled.insert(spec.id.clone()); }
        }
    }
    pub fn hide_status(&self, id: &str) -> &str {
        if self.revealed.contains(id) { "Window opened manually" }
        else if self.hide_attempts.get(id).copied().unwrap_or(0) >= 3 { "Hide failed · Check permissions" }
        else if self.startup_tray_handled.contains(id) { "Startup hidden" }
        else if self.start_time.elapsed() >= Duration::from_secs(120) { "Startup wait ended · Manual hide available" }
        else { "Waiting for main window" }
    }
    pub fn hide_one(&mut self, spec: &AppSpec) {
        let previous = self.session_specs.clone();
        let mut target = spec.clone(); target.enabled = true; target.tray = true;
        self.session_specs = vec![target]; self.hide_attempts.remove(&spec.id);
        self.hide_helper_windows(&processes(), false); self.session_specs = previous;
    }
    fn helper_windows_to_hide(&mut self, running: &[Process], available: &[AppWindow], startup: bool) -> Vec<(AppWindow, Process, String)> {
        if startup && self.start_time.elapsed() >= Duration::from_secs(120) { return vec![]; }
        let mut targets = vec![];
        for spec in self.session_specs.iter().filter(|spec| spec.enabled && spec.tray) {
            if startup && (self.startup_tray_handled.contains(&spec.id) || self.revealed.contains(&spec.id)) { continue; }
            let matched: Vec<_> = available.iter().filter(|window| main_window(spec, window)).filter_map(|window| {
                running.iter().find(|process| process.pid == window.pid && spec.matches(&process.path)).map(|process| (window, process))
            }).collect();
            if startup && !matched.iter().any(|(window, _)| window.visible) { continue; }
            // WinForms may show the downloader again during Load, after its first HWND appears.
            // Let that initialization finish before consuming the one-shot startup hide.
            if startup && spec.id == "paints" {
                if let Some((window, _)) = matched.iter().find(|(window, _)| window.visible) {
                    if self.paints_window_ready.map(|(handle, _)| handle) != Some(window.handle) {
                        self.paints_window_ready = Some((window.handle, Instant::now()));
                    }
                    if self.paints_window_ready.unwrap().1.elapsed() < Duration::from_secs(2) { continue; }
                }
            }
            self.startup_tray_handled.insert(spec.id.clone());
            for (window, process) in matched.into_iter().filter(|(window, _)| window.visible) { targets.push((window.clone(), process.clone(), spec.id.clone())); }
        }
        targets
    }
    fn hide_helper_windows(&mut self, running: &[Process], startup: bool) {
        let available = windows();
        for (window, process, id) in self.helper_windows_to_hide(running, &available, startup) {
            unsafe {
                if id == "paints" { ShowWindow(window.handle as HWND, SW_SHOWMINNOACTIVE); }
                ShowWindow(window.handle as HWND, SW_HIDE);
                if IsWindowVisible(window.handle as HWND) == 0 {
                    self.hidden.insert(window.handle, (process, id.clone()));
                } else {
                    let attempts = self.hide_attempts.entry(id.clone()).or_default(); *attempts += 1;
                    if *attempts < 3 { self.startup_tray_handled.remove(&id); }
                    let message = format!("{id}: hide failed. Check permissions and try again.");
                    if !self.messages.contains(&message) { self.messages.push(message); }
                }
            }
        }
    }
    pub fn restore(&mut self, spec: &AppSpec) -> usize {
        self.revealed.insert(spec.id.clone());
        let running = processes();
        let mut count = 0;
        let mut candidates: Vec<_> = windows().into_iter().filter(|window| main_window(spec, window) && running.iter().any(|process| process.pid == window.pid && spec.matches(&process.path))).collect();
        if spec.id == "moza" {
            candidates.sort_by_key(|window| std::cmp::Reverse(window.title.to_lowercase().contains("release")));
            candidates.truncate(1);
        }
        for window in candidates {
            if running.iter().any(|process| process.pid == window.pid && spec.matches(&process.path)) {
                unsafe { ShowWindow(window.handle as HWND, SW_RESTORE); SetForegroundWindow(window.handle as HWND); }
                self.hidden.remove(&window.handle); count += 1;
            }
        }
        count
    }
    pub fn restore_all(&mut self, specs: &[AppSpec]) { for spec in specs { self.restore(spec); } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iracing_tray_is_opt_in_and_respects_reopened_windows() {
        assert!(!Config::default().apps[0].tray);
        let (mut controller, process, mut window) = tray_fixture(true);
        controller.session_specs[0].id = "iracing".into();
        window.title = "iRacing UI".into(); window.class_name = "Chrome_WidgetWin_1".into();
        controller.session_specs[0].tray = false;
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        controller.session_specs[0].tray = true;
        assert_eq!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).len(), 1);
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        controller.startup_tray_handled.clear(); controller.revealed.insert("iracing".into());
        assert!(controller.helper_windows_to_hide(&[process], &[window], true).is_empty());
    }

    #[test]
    fn repeated_start_preserves_live_window_state_but_rearms_closed_apps() {
        let (mut controller, process, window) = tray_fixture(true);
        controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true);
        controller.revealed.insert("fixture".into());
        controller.hidden.insert(window.handle, (process.clone(), "fixture".into()));
        controller.prepare_startup_state(true, &[process.clone()]);
        assert!(controller.startup_tray_handled.contains("fixture"));
        assert!(controller.revealed.contains("fixture"));
        assert!(controller.hidden.contains_key(&window.handle));
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        controller.prepare_startup_state(true, &[]);
        assert!(controller.startup_tray_handled.is_empty() && controller.revealed.is_empty() && controller.hidden.is_empty());
        assert_eq!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).len(), 1);
        controller.prepare_startup_state(false, &[process]);
        assert!(controller.startup_tray_handled.is_empty());
    }

    #[test]
    fn ui_exit_and_missing_ui_never_start_automatic_cleanup() {
        for seen in [false, true] {
            let mut controller = Controller::new();
            controller.phase = Phase::Running;
            controller.seen_main = seen;
            controller.start_time = Instant::now()-Duration::from_secs(180);
            controller.last_main = Instant::now()-Duration::from_secs(180);
            controller.tick(Path::new("."), &[]);
            assert_eq!(controller.phase, Phase::Running);
            assert!(controller.receiver.is_none());
        }
    }

    #[test]
    fn paints_waits_for_the_final_window_then_hides_once() {
        let (mut controller, process, mut window) = tray_fixture(true);
        controller.session_specs[0].id = "paints".into(); window.title = "Trading Paints".into();
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        window.handle += 1;
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        controller.paints_window_ready = Some((window.handle, Instant::now()-Duration::from_secs(3)));
        assert_eq!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).len(), 1);
        assert!(controller.helper_windows_to_hide(&[process], &[window], true).is_empty());
    }

    #[test]
    fn opted_in_game_return_hides_only_main_window_once_and_respects_manual_restore() {
        let (mut controller, process, mut main) = tray_fixture(false);
        controller.session_specs[0].id = "edge".into();
        controller.session_specs[0].rehide_after_game = true;
        main.title = "Edge Overlays".into();
        assert!(controller.game_return_targets(true, &[process.clone()], &[main.clone()]).is_empty());
        assert!(controller.game_return_targets(false, &[process.clone()], &[main.clone()]).is_empty());
        main.visible = true;
        let mut overlay = main.clone(); overlay.title = "Overlays".into(); overlay.handle += 1;
        let mut unrelated = process.clone(); unrelated.path = r"C:\Other\Edge Overlays.exe".into();
        assert!(controller.game_return_targets(false, &[unrelated], &[main.clone()]).is_empty());
        let targets = controller.game_return_targets(false, &[process.clone()], &[main.clone(), overlay]);
        assert_eq!(targets.len(), 1); assert_eq!(targets[0].0.handle, main.handle);
        assert!(controller.game_return_targets(false, &[process.clone()], &[main.clone()]).is_empty());
        main.visible = false;
        controller.game_return_targets(true, &[process.clone()], &[main.clone()]);
        controller.revealed.insert("edge".into()); main.visible = true;
        assert!(controller.game_return_targets(false, &[process], &[main]).is_empty());
    }

    #[test]
    fn game_return_is_disabled_by_default_and_generic_when_enabled() {
        let (mut controller, process, mut window) = tray_fixture(false);
        controller.game_return_targets(true, &[process.clone()], &[window.clone()]);
        window.visible = true;
        assert!(controller.game_return_targets(false, &[process.clone()], &[window.clone()]).is_empty());
        controller.session_specs[0].rehide_after_game = true;
        window.visible = false;
        controller.game_return_targets(true, &[process.clone()], &[window.clone()]);
        window.visible = true;
        assert_eq!(controller.game_return_targets(false, &[process.clone()], &[window.clone()]).len(), 1);
        assert!(controller.game_return_targets(false, &[process], &[window]).is_empty());
    }

    #[test]
    fn game_return_expires_and_fresh_start_discards_pending_work() {
        let (mut controller, process, mut window) = tray_fixture(false);
        controller.session_specs[0].rehide_after_game = true;
        controller.game_return_targets(true, &[process.clone()], &[window.clone()]);
        controller.game_return_targets(false, &[process.clone()], &[window.clone()]);
        controller.game_return_deadlines.insert("fixture".into(), Instant::now()-Duration::from_secs(1));
        window.visible = true;
        assert!(controller.game_return_targets(false, &[process.clone()], &[window]).is_empty());
        controller.hidden_during_game.insert("fixture".into());
        controller.game_return_deadlines.insert("fixture".into(), Instant::now()+Duration::from_secs(15));
        controller.sim_was_running = true;
        controller.prepare_startup_state(false, &[process]);
        assert!(controller.hidden_during_game.is_empty() && controller.game_return_deadlines.is_empty());
        assert!(!controller.sim_was_running);
    }

    #[test]
    fn manual_restore_cancels_paints_startup_wait() {
        let (mut controller, process, mut window) = tray_fixture(true);
        controller.session_specs[0].id = "paints".into(); window.title = "Trading Paints".into();
        controller.paints_window_ready = Some((window.handle, Instant::now()-Duration::from_secs(3)));
        controller.revealed.insert("paints".into());
        assert!(controller.helper_windows_to_hide(&[process], &[window], true).is_empty());
    }

    fn tray_fixture(visible: bool) -> (Controller, Process, AppWindow) {
        let spec = AppSpec { id: "fixture".into(), name: "Fixture".into(), path: r"C:\Paddock-test\helper.exe".into(), working_dir: r"C:\Paddock-test".into(), enabled: true, tray: true, rehide_after_game: false, extra_paths: vec![] };
        let process = Process { pid: 123, parent_pid: 0, path: spec.path.clone(), created: 1 };
        let window = AppWindow { handle: 1234, pid: process.pid, title: "Fixture".into(), visible, tool: false, class_name: "Fixture".into(), owner: 0, app_window: false };
        let mut controller = Controller::new(); controller.session_specs = vec![spec];
        (controller, process, window)
    }

    #[test]
    fn startup_hides_once_even_if_reopened_recreated_or_restarted() {
        let (mut controller, mut process, mut window) = tray_fixture(true);
        assert_eq!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).len(), 1);
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        window.handle += 1;
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        process.pid += 1; process.created += 1; window.pid = process.pid;
        assert!(controller.helper_windows_to_hide(&[process], &[window], true).is_empty());
    }

    #[test]
    fn initially_hidden_native_tray_windows_are_already_handled() {
        let (mut controller, process, mut window) = tray_fixture(false);
        controller.mark_existing_hidden(&[process.clone()], &[window.clone()]);
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        window.visible = true;
        assert!(controller.helper_windows_to_hide(&[process], &[window], true).is_empty());
    }

    #[test]
    fn startup_waits_for_first_window_but_never_enforces_tray_indefinitely() {
        let (mut controller, process, window) = tray_fixture(true);
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[], true).is_empty());
        assert_eq!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).len(), 1);
        let (mut controller, _, _) = tray_fixture(true);
        controller.start_time = Instant::now()-Duration::from_secs(121);
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        assert_eq!(controller.helper_windows_to_hide(&[process], &[window], false).len(), 1);
    }

    #[test]
    fn explicit_hide_is_one_pass_and_launcher_restore_cancels_pending_hide() {
        let (mut controller, process, window) = tray_fixture(true);
        controller.revealed.insert("fixture".into());
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        assert_eq!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], false).len(), 1);
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        assert_eq!(controller.helper_windows_to_hide(&[process], &[window], false).len(), 1);
    }

    #[test]
    fn tray_targets_preserve_unmanaged_windows_and_opt_outs() {
        let (mut controller, process, window) = tray_fixture(true);
        let mut unrelated = process.clone(); unrelated.path = r"C:\Elsewhere\helper.exe".into();
        assert!(controller.helper_windows_to_hide(&[unrelated], &[window.clone()], true).is_empty());
        for (enabled, tray, id) in [(false, true, "fixture"), (true, false, "fixture")] {
            let spec = &mut controller.session_specs[0]; spec.enabled = enabled; spec.tray = tray; spec.id = id.into();
            assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], false).is_empty());
        }
        let (mut controller, process, mut window) = tray_fixture(true); window.title = "Fixture Overlay".into();
        assert!(controller.helper_windows_to_hide(&[process], &[window], true).is_empty());
    }

    #[test]
    fn finished_session_clears_one_shot_state() {
        let (mut controller, process, window) = tray_fixture(true);
        controller.helper_windows_to_hide(&[process], &[window], true);
        assert!(!controller.startup_tray_handled.is_empty());
        controller.phase = Phase::Finished; controller.reset_finished();
        assert!(controller.startup_tray_handled.is_empty());
    }
    #[test]
    fn newly_created_hidden_window_does_not_consume_initial_hide() {
        let (mut controller, process, mut window) = tray_fixture(false);
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        assert!(!controller.startup_tray_handled.contains("fixture"));
        window.visible = true;
        assert_eq!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).len(), 1);
        assert!(controller.helper_windows_to_hide(&[process], &[window], true).is_empty());
    }
    #[test]
    fn slow_start_and_unready_moza_window_wait_for_real_main_window() {
        let (mut controller, process, mut window) = tray_fixture(true);
        controller.start_time = Instant::now()-Duration::from_secs(45);
        controller.session_specs[0].id = "moza".into();
        window.title.clear(); window.class_name = "Qt6QWindowIcon".into(); window.app_window = true;
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        window.title = "MOZA Pit House 1.4 release".into();
        assert_eq!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).len(), 1);
        assert!(controller.helper_windows_to_hide(&[process.clone()], &[window.clone()], true).is_empty());
        assert_eq!(controller.helper_windows_to_hide(&[process], &[window], false).len(), 1);
    }
    #[test]
    fn overlays_are_preserved() {
        let spec = Config::default().apps.remove(3);
        let window = |title: &str| AppWindow { handle: 1, pid: 1, title: title.into(), visible: true, tool: false, class_name: "Fixture".into(), owner: 0, app_window: false };
        assert!(main_window(&spec, &window("Edge Overlays")));
        assert!(!main_window(&spec, &window("Relative Overlay")));
        let mut spec = Config::default().apps.remove(6);
        assert!(!main_window(&spec, &window("GDI+ Window (CrewChiefV4.exe)")));
        assert!(main_window(&spec, &window("Crew Chief V4")));
        spec.id = "garage61".into();
        assert!(main_window(&spec, &window("Garage 61 Telemetry Agent - Settings")));
    }
}
