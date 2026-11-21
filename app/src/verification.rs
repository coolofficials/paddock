use crate::{app_control::{self, Controller, Phase}, storage::{self, AppSpec, Config}};
use std::{fs, path::Path, process::Command, thread, time::{Duration, Instant}};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, ShowWindow, SW_HIDE, SW_RESTORE, WM_CLOSE};

pub fn smoke(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let spec = |id: &str, suffix: &str| -> Result<AppSpec, String> {
        let path = root.join(format!("paddock-fixture-{suffix}.exe"));
        fs::copy(&executable, &path).map_err(|error| error.to_string())?;
        Ok(AppSpec { id: id.into(), name: suffix.into(), path: path.to_string_lossy().into_owned(), working_dir: root.to_string_lossy().into_owned(), enabled: true, tray: id != "iracing", rehide_after_game: false, extra_paths: vec![] })
    };
    let main = spec("iracing", "main")?;
    let normal = spec("normal", "normal")?;
    let stubborn = spec("stubborn", "stubborn")?;
    let outside = spec("outside", "outside-stubborn")?;
    let mut outsider = Command::new(&outside.path).spawn().map_err(|error| error.to_string())?;
    let mut result = vec![];
    let config = Config { apps: vec![main.clone(), normal.clone(), stubborn.clone()], launcher_to_tray: false, ..Config::default() };
    let matching_windows = |app: &AppSpec| {
        let running = app_control::processes();
        app_control::windows().into_iter().filter(|window| window.class_name == "PaddockWindow" && running.iter().any(|process| process.pid == window.pid && app.matches(&process.path))).collect::<Vec<_>>()
    };
    let visible = |app: &AppSpec| matching_windows(app).iter().any(|window| window.visible);
    Command::new(&normal.path).spawn().map_err(|error| error.to_string())?;
    Command::new(&stubborn.path).spawn().map_err(|error| error.to_string())?;
    let deadline = Instant::now()+Duration::from_secs(15);
    while Instant::now() < deadline && (!visible(&normal) || !visible(&stubborn)) { thread::sleep(Duration::from_millis(200)); }
    let preexisting_ready = visible(&normal) && visible(&stubborn);
    for window in matching_windows(&stubborn) { unsafe { ShowWindow(window.handle as _, SW_HIDE); } }
    let mut controller = Controller::new();
    controller.start(root, &config)?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        controller.tick(root, &config.apps);
        if controller.phase == Phase::Running && config.apps.iter().all(|app| controller.status.get(&app.id) == Some(&1)) && app_control::windows().iter().filter(|window| app_control::processes().iter().any(|process| process.pid == window.pid && config.apps.iter().any(|app| app.matches(&process.path)))).count() >= 3 { break; }
        thread::sleep(Duration::from_millis(300));
    }
    let verify = |condition: bool, label: &str, result: &mut Vec<String>| -> Result<(), String> {
        if !condition { Err(format!("FAIL: {label}")) } else { result.push(format!("PASS: {label}")); Ok(()) }
    };
    let outcome: Result<(), String> = (|| {
        verify(preexisting_ready, "pre-existing visible and native-tray helpers prepared", &mut result)?;
        verify(controller.seen_main, "main UI process detected", &mut result)?;
        app_control::launch(root, &config.apps);
        let processes = app_control::processes();
        verify(config.apps.iter().all(|app| processes.iter().filter(|process| app.matches(&process.path)).count() == 1), "already-running apps reused without duplicates", &mut result)?;
        verify(visible(&main) && !visible(&normal) && !visible(&stubborn), "main UI remains visible; helper windows hidden", &mut result)?;
        verify(app_control::windows().iter().any(|window| window.visible && window.app_window && !window.tool && processes.iter().any(|process| process.pid == window.pid && main.matches(&process.path))), "main UI explicitly registered for taskbar", &mut result)?;
        for window in matching_windows(&normal) { unsafe { ShowWindow(window.handle as _, SW_RESTORE); } }
        for _ in 0..3 { controller.tick(root, &config.apps); thread::sleep(Duration::from_millis(300)); }
        verify(visible(&normal) && !controller.revealed.contains(&normal.id), "externally reopened helper stays visible across timer ticks", &mut result)?;
        for window in matching_windows(&stubborn) { unsafe { ShowWindow(window.handle as _, SW_RESTORE); } }
        for _ in 0..3 { controller.tick(root, &config.apps); thread::sleep(Duration::from_millis(300)); }
        verify(visible(&stubborn) && !controller.revealed.contains(&stubborn.id), "initially hidden native-tray helper stays visible when reopened", &mut result)?;
        controller.hide_helpers(&app_control::processes());
        verify(!visible(&normal) && !visible(&stubborn), "explicit hide hides current helper windows once", &mut result)?;
        for window in matching_windows(&stubborn) { unsafe { ShowWindow(window.handle as _, SW_RESTORE); } }
        controller.tick(root, &config.apps);
        verify(visible(&stubborn), "explicit hide does not re-enable continuous hiding", &mut result)?;
        verify(controller.restore(&normal) == 1, "helper window restored", &mut result)?;
        controller.tick(root, &config.apps);
        verify(visible(&normal), "restored window stays visible", &mut result)?;
        let before = app_control::processes();
        controller.start(root, &config)?;
        let deadline = Instant::now()+Duration::from_secs(15);
        while Instant::now()<deadline && controller.phase!=Phase::Running { controller.tick(root,&config.apps);thread::sleep(Duration::from_millis(200)); }
        let after = app_control::processes();
        verify(config.apps.iter().all(|app| {
            let old:Vec<_>=before.iter().filter(|process|app.matches(&process.path)).map(|process|(process.pid,process.created)).collect();
            let new:Vec<_>=after.iter().filter(|process|app.matches(&process.path)).map(|process|(process.pid,process.created)).collect();old==new && new.len()==1
        }) && visible(&normal) && visible(&stubborn), "repeated Start All preserves process identities and reopened windows", &mut result)?;
        app_control::stop_all(root, &[normal.clone()]);
        controller.start(root, &config)?;
        let deadline=Instant::now()+Duration::from_secs(15);
        while Instant::now()<deadline {
            controller.tick(root,&config.apps);
            if controller.phase==Phase::Running&&!matching_windows(&normal).is_empty()&&!visible(&normal){break;}
            thread::sleep(Duration::from_millis(200));
        }
        let after_relaunch=app_control::processes();
        verify(config.apps.iter().all(|app|after_relaunch.iter().filter(|process|app.matches(&process.path)).count()==1)
            && after_relaunch.iter().any(|process|normal.matches(&process.path)&&!after.iter().any(|old|old.pid==process.pid&&old.created==process.created))
            && [&main,&stubborn].iter().all(|app|after_relaunch.iter().filter(|process|app.matches(&process.path)).all(|process|after.iter().any(|old|old.pid==process.pid&&old.created==process.created)))
            && !visible(&normal)&&visible(&stubborn), "mixed-state Start All launches only the closed app and applies its startup preference", &mut result)?;
        controller.hide_helpers(&app_control::processes());
        let processes = app_control::processes();
        for window in app_control::windows().iter().filter(|window| processes.iter().any(|process| process.pid == window.pid && main.matches(&process.path))) { unsafe { PostMessageW(window.handle as _, WM_CLOSE, 0, 0); } }
        let deadline = Instant::now() + Duration::from_secs(9);
        while Instant::now() < deadline { controller.tick(root, &config.apps); thread::sleep(Duration::from_millis(300)); }
        verify(controller.phase == Phase::Running && app_control::processes().iter().any(|process| normal.matches(&process.path)) && app_control::processes().iter().any(|process| stubborn.matches(&process.path)), "main UI exit preserves all helper apps until explicit Stop All", &mut result)?;
        controller.stop_all(root, &config);
        controller.stop_all(root, &config);
        let deadline = Instant::now() + Duration::from_secs(35);
        while Instant::now() < deadline && controller.phase != Phase::Finished { controller.tick(root, &config.apps); thread::sleep(Duration::from_millis(300)); }
        verify(controller.phase == Phase::Finished, "explicit Stop All completes session cleanup", &mut result)?;
        verify(!app_control::processes().iter().any(|process| config.apps.iter().any(|app| app.matches(&process.path))), "normal and close-resistant helpers fully exited", &mut result)?;
        controller.stop_all(root, &config); controller.tick(root, &config.apps);
        verify(controller.phase==Phase::Finished, "repeated Stop All after completion remains stopped", &mut result)?;
        verify(outsider.try_wait().map_err(|error| error.to_string())?.is_none(), "unrelated executable remains running", &mut result)?;
        controller.reset_finished();
        verify(controller.phase == Phase::Idle && controller.session_specs.is_empty() && controller.hidden.is_empty() && controller.revealed.is_empty(), "cleanup resets launcher for another session", &mut result)?;
        controller.start(root, &config)?;
        controller.stop_all(root, &config);
        controller.stop_all(root, &config);
        let deadline = Instant::now() + Duration::from_secs(40);
        while Instant::now() < deadline && controller.phase != Phase::Finished { controller.tick(root, &config.apps); thread::sleep(Duration::from_millis(300)); }
        verify(controller.phase == Phase::Finished && !app_control::processes().iter().any(|process| config.apps.iter().any(|app| app.matches(&process.path))), "relaunch and full stop during startup leave no managed apps", &mut result)?;
        controller.reset_finished();
        verify(controller.phase == Phase::Idle && outsider.try_wait().map_err(|error| error.to_string())?.is_none(), "full stop keeps launcher reusable and unrelated process alive", &mut result)?;
        controller.start(root, &config)?;
        let deadline = Instant::now()+Duration::from_secs(15);
        while Instant::now() < deadline {
            controller.tick(root, &config.apps);
            if controller.phase == Phase::Running && visible(&main) && !matching_windows(&normal).is_empty() && !matching_windows(&stubborn).is_empty() && !visible(&normal) && !visible(&stubborn) { break; }
            thread::sleep(Duration::from_millis(300));
        }
        verify(controller.phase == Phase::Running && visible(&main) && !matching_windows(&normal).is_empty() && !matching_windows(&stubborn).is_empty() && !visible(&normal) && !visible(&stubborn), "next Start All session arms one-shot hiding again", &mut result)?;
        controller.stop_all(root, &config);
        let deadline = Instant::now()+Duration::from_secs(40);
        while Instant::now() < deadline && controller.phase != Phase::Finished { controller.tick(root, &config.apps); thread::sleep(Duration::from_millis(300)); }
        verify(controller.phase == Phase::Finished && !app_control::processes().iter().any(|process| config.apps.iter().any(|app| app.matches(&process.path))), "final fixture cleanup completed", &mut result)?;
        controller.reset_finished();let mut helpers=config.clone();helpers.apps[0].enabled=false;
        controller.start(root,&helpers)?;
        let deadline=Instant::now()+Duration::from_secs(15);
        while Instant::now()<deadline {
            controller.tick(root,&helpers.apps);
            if controller.phase==Phase::Running&&app_control::processes().iter().any(|process|normal.matches(&process.path)){break;}
            thread::sleep(Duration::from_millis(200));
        }
        let running=app_control::processes();
        verify(controller.phase==Phase::Running&&!running.iter().any(|process|main.matches(&process.path))&&running.iter().any(|process|normal.matches(&process.path)),"profile starts helpers with game launcher excluded",&mut result)?;
        controller.stop_all(root,&helpers);
        let deadline=Instant::now()+Duration::from_secs(40);
        while Instant::now()<deadline&&controller.phase!=Phase::Finished{controller.tick(root,&helpers.apps);thread::sleep(Duration::from_millis(250));}
        verify(controller.phase==Phase::Finished&&!app_control::processes().iter().any(|process|helpers.apps.iter().any(|app|app.enabled&&app.matches(&process.path))),"helper-only profile stops all included apps",&mut result)?;
        controller.reset_finished();
        let mut tray_config=config.clone();for app in &mut tray_config.apps {app.enabled=app.id=="iracing";app.tray=true;}
        controller.start(root,&tray_config)?;
        let deadline=Instant::now()+Duration::from_secs(15);
        while Instant::now()<deadline {
            controller.tick(root,&tray_config.apps);
            if controller.phase==Phase::Running&&!matching_windows(&main).is_empty()&&!visible(&main){break;}
            thread::sleep(Duration::from_millis(200));
        }
        verify(!matching_windows(&main).is_empty()&&!visible(&main),"iRacing Tray preference hides its main window",&mut result)?;
        verify(controller.restore(&main)==1&&visible(&main),"hidden iRacing window can be restored",&mut result)?;
        controller.start(root,&tray_config)?;
        let deadline=Instant::now()+Duration::from_secs(15);
        while Instant::now()<deadline&&controller.phase!=Phase::Running{controller.tick(root,&tray_config.apps);thread::sleep(Duration::from_millis(200));}
        verify(visible(&main),"repeated Start All preserves manually restored iRacing window",&mut result)?;
        controller.stop_all(root,&tray_config);
        let deadline=Instant::now()+Duration::from_secs(35);
        while Instant::now()<deadline&&controller.phase!=Phase::Finished{controller.tick(root,&tray_config.apps);thread::sleep(Duration::from_millis(200));}
        verify(controller.phase==Phase::Finished&&!app_control::processes().iter().any(|process|main.matches(&process.path)),"tray fixture cleanup completed",&mut result)?;
        Ok(())
    })();
    if outcome.is_err() {
        app_control::stop(root, &config.apps);
        let running = app_control::processes();
        for window in app_control::windows().iter().filter(|window| running.iter().any(|process| process.pid == window.pid && main.matches(&process.path))) { unsafe { PostMessageW(window.handle as _, WM_CLOSE, 0, 0); } }
    }
    let _ = outsider.kill(); let _ = outsider.wait();
    if let Err(error) = &outcome { result.push(error.clone()); }
    storage::atomic_bytes(&root.join("smoke-result.txt"), result.join("\n").as_bytes())?;
    outcome
}

fn simulator_running() -> bool {
    app_control::processes().iter().any(|process| Path::new(&process.path).file_name().map(|name| name.to_string_lossy().to_lowercase().starts_with("iracingsim")).unwrap_or(false))
}

/// Inspect a single real helper without launching iRacing or touching other managed apps.
pub fn helper_tray(root: &Path, id: &str) -> Result<(), String> {
    let store = storage::Store::open(root.to_owned())?;
    let spec = store.config.apps.iter().find(|spec| spec.id == id && spec.id != "iracing").cloned().ok_or("Unknown helper")?;
    if app_control::processes().iter().any(|process| spec.matches(&process.path)) { return Err("Helper already running; verification skipped".into()); }
    let mut controller = Controller::new();
    controller.phase = Phase::Running; controller.session_specs = vec![spec.clone()];
    let mut report = app_control::launch(root, &[spec.clone()]);
    let deadline = Instant::now()+Duration::from_secs(15);
    let mut hidden = false;
    while Instant::now() < deadline {
        controller.tick(root, &[spec.clone()]);
        if controller.hide_status(id) == "Startup hidden" { hidden = true; break; }
        thread::sleep(Duration::from_millis(200));
    }
    let running = app_control::processes();
    let matched: Vec<_> = app_control::windows().into_iter().filter(|window| running.iter().any(|process| process.pid == window.pid && spec.matches(&process.path))).collect();
    for window in &matched { report.push(format!("window={:?}; visible={}; class={}",window.title,window.visible,window.class_name)); }
    report.push(format!("Startup main window hidden: {hidden}"));
    report.extend(app_control::stop(root, &[spec]));
    storage::atomic_bytes(&root.join(format!("helper-tray-{id}.txt")), report.join("\n").as_bytes())?;
    if hidden { Ok(()) } else { Err("Helper startup hide was not verified".into()) }
}

pub fn real_apps(root: &Path) -> Result<(), String> {
    let store = storage::Store::open(root.to_path_buf())?;
    if simulator_running() { return Err("실제 시뮬레이터 실행 중: 종료 테스트 생략".into()); }
    let initial = app_control::processes();
    if initial.iter().any(|process| store.config.apps.iter().any(|spec| spec.id == "iracing" && spec.matches(&process.path))) { return Err("기존 iRacing UI 실행 중: 실제 종료 테스트 생략".into()); }
    let mut controller = Controller::new(); controller.start(root, &store.config)?;
    let mut report = vec![];
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline { controller.tick(root, &store.config.apps); thread::sleep(Duration::from_millis(500)); }
    report.extend(controller.messages.iter().cloned());
    let snapshot = |label: &str, report: &mut Vec<String>| {
        let running = app_control::processes(); let windows = app_control::windows();
        report.push(format!("--- {label} ---"));
        for spec in &store.config.apps {
            let matches: Vec<_> = running.iter().filter(|process| spec.matches(&process.path)).collect();
            report.push(format!("{}: {} processes; {}", spec.name, matches.len(), matches.iter().map(|process| format!("{} {}",process.pid,process.path)).collect::<Vec<_>>().join("; ")));
            for window in windows.iter().filter(|window| matches.iter().any(|process| process.pid == window.pid)) { report.push(format!("  window {:?}; visible={}; tool={}; app_window={}; owner={}",window.title,window.visible,window.tool,window.app_window,window.owner)); }
        }
    };
    snapshot("after launch and tray handling", &mut report);
    let running = app_control::processes();
    let taskbar_ready = app_control::windows().iter().any(|window| window.visible && window.app_window && !window.tool && running.iter().any(|process| process.pid == window.pid && store.config.apps.iter().any(|spec| spec.id == "iracing" && spec.matches(&process.path))));
    report.push(format!("iRacing main window visible and taskbar-enabled: {taskbar_ready}"));
    for spec in store.config.apps.iter().filter(|spec| spec.id != "iracing") { report.push(format!("{} restored windows: {}",spec.name,controller.restore(spec))); }
    snapshot("after restore", &mut report);
    controller.hide_helpers(&app_control::processes());
    if simulator_running() {
        controller.restore_all(&store.config.apps);
        report.push("시뮬레이터가 시작되어 종료 검증 생략. 창을 복원했습니다.".into());
        storage::atomic_bytes(&root.join("real-app-verification.txt"), report.join("\n").as_bytes())?;
        return Ok(());
    }
    if !std::env::args().any(|argument| argument == "--verify-full-stop") {
      if let Some(main) = store.config.apps.iter().find(|spec| spec.id == "iracing") {
        let running = app_control::processes();
        for window in app_control::windows().iter().filter(|window| running.iter().any(|process| process.pid == window.pid && main.matches(&process.path))) { unsafe { PostMessageW(window.handle as _, WM_CLOSE, 0, 0); } }
      }
      let deadline = Instant::now()+Duration::from_secs(9);
      while Instant::now()<deadline { controller.tick(root, &store.config.apps); thread::sleep(Duration::from_millis(300)); }
      report.push(format!("UI exit does not initiate cleanup: {}", controller.phase == Phase::Running));
    }
    report.push("Shutdown trigger: explicit Stop All".into());
    controller.stop_all(root, &store.config);
    let deadline = Instant::now() + Duration::from_secs(35);
    while Instant::now() < deadline && controller.phase != Phase::Finished { controller.tick(root, &store.config.apps); thread::sleep(Duration::from_millis(500)); }
    if controller.phase != Phase::Finished { report.push("종료 감지 미완료: 지정 앱 정리 재시도".into()); if controller.shutdown_main { app_control::stop_all(root, &controller.session_specs); } else { app_control::stop(root, &controller.session_specs); } }
    report.extend(controller.messages.iter().cloned());
    snapshot("after UI close and cleanup", &mut report);
    report.push(format!("Controller final phase: {:?}",controller.phase));
    let stopped = controller.phase == Phase::Finished;
    controller.reset_finished();
    report.push(format!("Launcher ready for reuse: {}", controller.phase == Phase::Idle));
    storage::atomic_bytes(&root.join("real-app-verification.txt"), report.join("\n").as_bytes())?;
    if taskbar_ready && stopped { Ok(()) } else { Err("실제 앱 작업 표시줄/종료 상태 검증 실패: 로그를 확인하세요.".into()) }
}
