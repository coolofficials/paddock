#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod storage;
mod discovery;
mod season_schedule;
mod race_catalog;
mod planner;
mod app_control;
mod ui;
mod verification;
mod design;
mod icons;
mod bridge;
mod desktop;

use std::path::PathBuf;

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.iter().any(|argument| argument == "--review-ui-read-only") && (!arguments.iter().any(|argument| argument == "--data-dir") || arguments.iter().any(|argument| matches!(argument.as_str(), "--backup-settings" | "--import-settings-backup" | "--verify-real-apps" | "--verify-full-stop" | "--cleanup-test-residue"))) { std::process::exit(2); }
    if let Some(name) = std::env::current_exe().ok().and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned())).filter(|name| name.starts_with("paddock-fixture-")) {
        ui::fixture(&name, name.contains("stubborn")); return;
    }
    if arguments.iter().any(|argument| matches!(argument.as_str(), "--inspect-settings-editor" | "--backup-settings" | "--import-settings-backup" | "--cleanup-test-residue")) { std::process::exit(2); }
    let root = arguments.iter().position(|argument| argument == "--data-dir").and_then(|index| arguments.get(index + 1)).map(PathBuf::from).unwrap_or_else(|| std::env::current_exe().unwrap().parent().unwrap().join("data"));
    if let Some(index)=arguments.iter().position(|argument|argument=="--export-ui-state") {
        match bridge::Bridge::open(root).and_then(|bridge|storage::save(&PathBuf::from(arguments.get(index+1).ok_or("Output path required")?),&bridge.snapshot())) {Ok(())=>{},Err(_)=>std::process::exit(1)}return;
    }
    if let Some(index) = arguments.iter().position(|argument| argument == "--render-preview") {
        if let Err(error) = ui::render_preview(root.clone(), &PathBuf::from(&arguments[index+1])) { let _ = storage::atomic_bytes(&root.join("preview-error.txt"), error.as_bytes()); std::process::exit(1); } return;
    }
    if let Some(index) = arguments.iter().position(|argument| argument == "--render-planner-preview") {
        if let Err(error) = ui::render_planner_preview(root.clone(), &PathBuf::from(&arguments[index+1])) { let _ = storage::atomic_bytes(&root.join("preview-error.txt"), error.as_bytes()); std::process::exit(1); } return;
    }
    if arguments.iter().any(|argument| argument == "--smoke-test") {
        if verification::smoke(&root).is_err() { std::process::exit(1); } return;
    }
    if let Some(index) = arguments.iter().position(|argument| argument == "--verify-helper-tray") {
        let result = arguments.get(index+1).ok_or("Helper ID required".into()).and_then(|id| verification::helper_tray(&root, id));
        if let Err(error) = result { let _ = storage::atomic_bytes(&root.join("helper-tray-error.txt"), error.as_bytes()); std::process::exit(1); }
        return;
    }
    if arguments.iter().any(|argument| argument == "--verify-real-apps" || argument == "--verify-full-stop") {
        if let Err(error) = verification::real_apps(&root) { let _ = storage::atomic_bytes(&root.join("real-app-verification.txt"), error.as_bytes()); std::process::exit(1); } return;
    }
    if arguments.iter().any(|argument| argument == "--refresh") {
        let previous = season_schedule::cached(&root).ok();
        match season_schedule::refresh(&root, previous.as_ref()) {
            Ok(schedule) => { let _ = storage::atomic_bytes(&root.join("refresh-status.txt"), format!("{}: {} verified weeks", schedule.season, schedule.weeks.len()).as_bytes()); },
            Err(error) => { let _ = storage::atomic_bytes(&root.join("refresh-status.txt"), error.as_bytes()); std::process::exit(1); }
        } return;
    }
    if let Some(index) = arguments.iter().position(|argument| argument == "--fixture-app") {
        ui::fixture(arguments.get(index + 1).map(String::as_str).unwrap_or("Fixture"), arguments.iter().any(|argument| argument == "--ignore-close")); return;
    }
    if arguments.iter().any(|argument| argument == "--inventory") {
        let config = storage::Store::open(root.clone()).unwrap().config;
        let processes = app_control::processes();
        let windows = app_control::windows();
        let output: Vec<_> = config.apps.iter().map(|spec| serde_json::json!({"id":spec.id,"exists":std::path::Path::new(&spec.path).exists(),"path":spec.path,"processes":processes.iter().filter(|process| spec.matches(&process.path)).map(|process| serde_json::json!({"pid":process.pid,"parent_pid":process.parent_pid,"path":process.path})).collect::<Vec<_>>(),"windows":windows.iter().filter(|window| processes.iter().any(|process| process.pid == window.pid && spec.matches(&process.path))).map(|window| serde_json::json!({"handle":window.handle,"title":window.title,"visible":window.visible,"tool":window.tool,"class":window.class_name,"owner":window.owner,"app_window":window.app_window})).collect::<Vec<_>>()})).collect();
        let _ = storage::save(&root.join("inventory.json"), &output); return;
    }
    if let Some(index) = arguments.iter().position(|argument| argument == "--import-pdf") {
        let result = std::fs::read(&arguments[index + 1]).map_err(|error| error.to_string()).and_then(|bytes| season_schedule::import(&bytes, &root, None));
        match result { Ok(schedule) => { let _ = storage::atomic_bytes(&root.join("import-result.txt"), format!("{}: {} weeks verified", schedule.season, schedule.weeks.len()).as_bytes()); }, Err(error) => { let _ = storage::atomic_bytes(&root.join("import-result.txt"), error.as_bytes()); std::process::exit(1); } }
        return;
    }
    let result=if arguments.iter().any(|argument|argument=="--legacy-ui"||argument=="--verify-ui"){ui::run(root)}else{desktop::run(root)};
    if let Err(error) = result {
        if arguments.iter().any(|argument| argument == "--verify-ui") { eprintln!("{error}"); std::process::exit(1); }
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(std::ptr::null_mut(), storage::wide(&error).as_ptr(), storage::wide("Paddock").as_ptr(), windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR); }
    }
}
