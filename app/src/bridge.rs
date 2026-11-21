//! Presentation adapter. Persistence, scheduling and process behavior remain in the existing core.
use crate::{
    app_control::{self, Controller, Phase},
    race_catalog::{Category, License},
    season_schedule::{self, Schedule},
    storage::{self, AppProfile, AppSpec, Plan, Store},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
};

#[derive(Deserialize)]
pub struct Request {
    pub id: u64,
    pub command: String,
    #[serde(default)]
    pub args: Value,
}
#[derive(Serialize)]
pub struct Reply {
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<String>,
}
pub struct Bridge {
    pub store: Store,
    pub discovered_apps: Vec<AppSpec>,
    pub schedule: Option<Schedule>,
    pub controller: Controller,
    pub message: String,
    pub refresh: Option<Receiver<Result<Schedule, String>>>,
}
impl Bridge {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        let store = Store::open(root.clone())?;
        let schedule = season_schedule::cached(&root).ok();
        let message = store.warnings.join("\n");
        let mut bridge=Self {
            discovered_apps: crate::discovery::scan(),
            store,
            schedule,
            controller: Controller::new(),
            message,
            refresh: None,
        };
        // Reconcile again on open if a prior refresh saved its cache before a crash.
        if let Some(schedule)=bridge.schedule.clone() {
            if let Err(error)=bridge.prune_season_plans(&schedule) {
                bridge.message=format!("Previous-season plans could not be cleared: {error}");
            }
        }
        Ok(bridge)
    }
    pub fn editable(&self) -> Result<(), String> {
        if self.controller.phase == Phase::Idle {
            Ok(())
        } else {
            Err("Stop the current session before editing apps or switching profiles.".into())
        }
    }
    pub fn tick(&mut self) {
        self.controller
            .tick(&self.store.root, &self.store.config.apps);
        if self.controller.phase == Phase::Finished {
            self.controller.reset_finished();
            self.message = "All included apps stopped".into();
        }
        let done = self
            .refresh
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some(result) = done {
            self.refresh = None;
            match result {
                Ok(schedule) => {
                    let cleanup=self.prune_season_plans(&schedule);
                    self.schedule = Some(schedule);
                    self.message = match cleanup {
                        Ok(true)=>"Schedule updated. Previous-season plans cleared; favorites kept.".into(),
                        Ok(false)=>"Schedule updated".into(),
                        Err(error)=>format!("Schedule updated, but previous-season plans were kept: {error}"),
                    };
                }
                Err(error) => {
                    self.message = format!("Refresh failed. Previous schedule kept. {error}")
                }
            }
        }
    }
    fn save_config(&mut self, previous: storage::Config) -> Result<(), String> {
        if let Err(error) = self.store.save_config() {
            self.store.config = previous;
            return Err(error);
        }
        Ok(())
    }
    fn prune_season_plans(&mut self, schedule: &Schedule) -> Result<bool,String> {
        let mut plans=self.store.plans.clone();
        if !plans.retain_season(&schedule.season) {return Ok(false);}
        // Legacy favorites were inferred from plans; materialize them before pruning.
        if self.store.config.favorite_series.is_none() {
            let source=self.schedule.as_ref().unwrap_or(schedule);
            let favorites=self.store.plans.choices.values().chain(self.store.plans.selections.values().flatten()).filter_map(|plan| {
                if !plan.series_id.is_empty() {Some(plan.series_id.clone())}
                else {source.plan_index(plan).and_then(|index|source.catalog.get(index)).map(|series|series.id.clone())}
            }).collect();
            let previous=self.store.config.clone();
            self.store.config.favorite_series=Some(favorites);
            self.save_config(previous)?;
        }
        let previous=self.store.plans.clone();self.store.plans=plans;
        self.save_plans(previous)?;
        Ok(true)
    }
    fn save_plans(&mut self, previous: storage::Plans) -> Result<(), String> {
        if let Err(error) = self.store.save_plans() {
            self.store.plans = previous;
            return Err(error);
        }
        Ok(())
    }
    pub fn snapshot(&self) -> Value {
        let config = &self.store.config;
        let mut preferences = config.planner.clone();
        // Hidden is retired in the current UI. Old saved entries must not hide series.
        preferences.hidden_series.clear();
        preferences.hidden_only = false;
        let favorites = config.favorite_series.clone().unwrap_or_else(|| {
            self.store
                .plans
                .choices
                .values()
                .chain(self.store.plans.selections.values().flatten())
                .filter_map(|plan| {
                    self.schedule
                        .as_ref()?
                        .plan_index(plan)
                        .and_then(|index| self.schedule.as_ref()?.catalog.get(index))
                        .map(|series| series.id.clone())
                })
                .collect()
        });
        let schedule=self.schedule.as_ref().map(|schedule|{
            let mut indices=preferences.indices(schedule,schedule.active_week().unwrap_or(1),&favorites);
            // Keep each selected sort order within favorite and non-favorite groups.
            indices.sort_by_key(|&index| !favorites.contains(&schedule.catalog[index].id));
            // Resolve each saved plan once, rather than once per series/week cell.
            let selected_by_week: BTreeMap<_, BTreeSet<_>> = schedule.weeks.iter().map(|week| {
                let selected = self.store.plans.week(&schedule.key(week.number)).iter().filter_map(|plan| {
                    schedule.plan_index(plan).filter(|&index| Some(plan.track.as_str()) == schedule.track(week.number,index))
                }).collect();
                (week.number, selected)
            }).collect();
            // Current-week plans are independent of catalog search and filters.
            let lineup: Vec<_> = schedule.active_week().and_then(|week| {
                Some((week, selected_by_week.get(&week)?, schedule.weeks.iter().find(|item|item.number==week)?))
            }).map(|(_,selected,week)| selected.iter().filter_map(|&index| {
                let series=schedule.catalog.get(index)?;
                let entry=series.entry(&week.start)?;
                Some(json!({"id":series.id,"name":series.name,"class":series.class,"category":series.category,"track":entry.track,"vehicles":entry.vehicles}))
            }).collect()).unwrap_or_default();
            let series:Vec<_>=indices.iter().map(|&index|{
                let series=&schedule.catalog[index];
                let weeks:Vec<_>=schedule.weeks.iter().map(|week|{
                    let entry=series.entry(&week.start);
                    let selected=selected_by_week.get(&week.number).is_some_and(|selected|selected.contains(&index));
                    json!({"number":week.number,"start":week.start,"track":entry.map(|e|&e.track),"vehicles":entry.map(|e|&e.vehicles),"selected":selected})
                }).collect();
                json!({"id":series.id,"name":series.name,"class":series.class,"category":series.category,"vehicles":series.vehicles,"conditions":series.conditions,"times":series.times,"weeks":weeks,"favorite":favorites.contains(&series.id)})
            }).collect();
            json!({"season":schedule.season,"fetched_at":schedule.fetched_at,"weeks":schedule.weeks,"current_week":schedule.active_week(),"series":series,"lineup":lineup,"total_series":schedule.catalog.len(),"planned":schedule.weeks.iter().map(|week|self.store.plans.week(&schedule.key(week.number)).len()).sum::<usize>(),"favorite_count":favorites.len(),"can_undo":schedule.weeks.iter().filter(|week|self.store.plans.can_undo(&schedule.key(week.number))).map(|week|week.number).collect::<Vec<_>>()})
        });
        let apps:Vec<_>=config.apps.iter().enumerate().map(|(index,app)|json!({"index":index,"id":app.id,"name":app.name,"path":app.path,"enabled":app.enabled,"tray":app.tray,"rehide_after_game":app.rehide_after_game,"exists":Path::new(&app.path).is_file(),"running":self.controller.status.get(&app.id).copied().unwrap_or(0),"hide_status":self.controller.hide_status(&app.id)})).collect();
        json!({"version":env!("CARGO_PKG_VERSION"),"phase":format!("{:?}",self.controller.phase),"message":self.message,"logs":self.controller.messages.iter().rev().take(30).collect::<Vec<_>>(),"refreshing":self.refresh.is_some(),"apps":apps,"detected_apps":self.discovered_apps.iter().map(|app|json!({"id":app.id,"name":app.name,"path":app.path,"added":config.apps.iter().any(|existing|existing.id==app.id||existing.matches(&app.path))})).collect::<Vec<_>>(),"profiles":config.profiles.iter().map(|profile|json!({"id":profile.id,"name":profile.name,"app_count":profile.apps.len()})).collect::<Vec<_>>(),"active_profile":config.active_profile,"profile_name":config.profile_name(),"auto_hide":config.launcher_to_tray,"theme":config.appearance.theme,"preferences":preferences,"schedule":schedule,"categories":Category::ALL.map(|value|json!({"value":value,"label":value.label()})),"classes":License::ALL.map(|value|json!({"value":value,"label":if value==License::Pro{"Pro"}else{value.label()}}))})
    }
    pub fn handle(&mut self, request: Request) -> Reply {
        let result = self.command(&request.command, &request.args);
        match result {
            Ok(value) => Reply {
                id: request.id,
                result: Some(value),
                error: None,
            },
            Err(error) => {
                self.message = error.clone();
                Reply {
                    id: request.id,
                    result: None,
                    error: Some(error),
                }
            }
        }
    }
    pub fn command(&mut self, command: &str, args: &Value) -> Result<Value, String> {
        if command != "state" {
            self.message.clear();
        }
        match command {
            "state" => return Ok(self.snapshot()),
            "start" => {
                self.controller
                    .start(&self.store.root, &self.store.config)?;
                self.message = "Launching included apps…".into();
            }
            "stop" => {
                if app_control::processes().iter().any(|process| {
                    Path::new(&process.path).file_name().is_some_and(|name| {
                        name.to_string_lossy()
                            .to_ascii_lowercase()
                            .starts_with("iracingsim")
                    })
                }) {
                    return Err("Finish driving before stopping all apps.".into());
                }
                if matches!(
                    self.controller.phase,
                    Phase::Starting | Phase::Running | Phase::Stopping | Phase::CleanupFailed
                ) || app_control::processes().iter().any(|process| {
                    self.store
                        .config
                        .apps
                        .iter()
                        .any(|app| app.enabled && app.matches(&process.path))
                }) {
                    self.controller
                        .stop_all(&self.store.root, &self.store.config);
                    self.message = "Stopping included apps…".into();
                } else {
                    self.controller = Controller::new();
                    self.message = "No included apps are running".into();
                }
            }
            "retry_stop" => {
                self.controller.retry_stop(&self.store.root);
            }
            "hide_helpers" => {
                for app in self.store.config.apps.iter().filter(|app| app.enabled) {
                    self.controller.hide_one(app);
                }
                self.message = "App windows hidden".into();
            }
            "show_windows" => {
                let included: Vec<_> = self.store.config.apps.iter().filter(|app| app.enabled).cloned().collect();
                self.controller.restore_all(&included);
                self.message = "Included app windows restored".into();
            }
            "app.restore" | "app.hide" => {
                let index = index(args, self.store.config.apps.len())?;
                let app = &self.store.config.apps[index];
                if command == "app.restore" {
                    if self.controller.restore(app) == 0 {
                        self.message = format!("Open {} from its tray icon.", app.name);
                    }
                } else {
                    self.controller.hide_one(app);
                }
            }
            "app.update" => {
                self.editable()?;
                let index = index(args, self.store.config.apps.len())?;
                let previous = self.store.config.clone();
                let app = &mut self.store.config.apps[index];
                if let Some(name) = args.get("name") {
                    app.name = clean_name(name)?;
                }
                if let Some(enabled) = args.get("enabled").and_then(Value::as_bool) {
                    app.enabled = enabled;
                }
                if let Some(tray) = args.get("tray").and_then(Value::as_bool) {
                    app.tray = tray;
                }
                if let Some(enabled) = args.get("rehide_after_game").and_then(Value::as_bool) {app.rehide_after_game=enabled;}
                self.save_config(previous)?;
            }
            "app.scan" => {
                self.discovered_apps=crate::discovery::scan();
                self.message=format!("{} apps found at default locations",self.discovered_apps.len());
            }
            "app.add_detected" => {
                self.editable()?;
                let id=string(args,"id")?;
                let app=self.discovered_apps.iter().find(|app|app.id==id).ok_or("Detected app not found. Scan again.")?.clone();
                if self.store.config.apps.iter().any(|existing|existing.id==app.id||existing.matches(&app.path)) {return Err("This app is already in the profile.".into());}
                return self.selected_path(app.path,None);
            }
            "app.remove" => {
                self.editable()?;
                let index = index(args, self.store.config.apps.len())?;
                let previous = self.store.config.clone();
                self.store.config.apps.remove(index);
                self.save_config(previous)?;
            }
            "profile.select" => {
                self.editable()?;
                let id = string(args, "id")?;
                let index = self
                    .store
                    .config
                    .profiles
                    .iter()
                    .position(|profile| profile.id == id)
                    .ok_or("Unknown profile")?;
                let previous = self.store.config.clone();
                self.store.config.select_profile(index);
                self.save_config(previous)?;
                self.controller.status.clear();
            }
            "profile.create" => {
                self.editable()?;
                let name = clean_name(args.get("name").ok_or("Name required")?)?;
                let previous = self.store.config.clone();
                self.store.config.remember_profile();
                let apps = if args
                    .get("duplicate")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    self.store.config.apps.clone()
                } else {
                    vec![]
                };
                self.store.config.profiles.push(AppProfile {
                    id: format!("profile-{}", chrono::Utc::now().timestamp_micros()),
                    name,
                    apps,
                    launcher_to_tray: self.store.config.launcher_to_tray,
                });
                let last = self.store.config.profiles.len() - 1;
                self.store.config.select_profile(last);
                self.save_config(previous)?;
                self.controller.status.clear();
            }
            "profile.delete" => {
                self.editable()?;
                let id=string(args,"id")?;
                let index=self.store.config.profiles.iter().position(|profile|profile.id==id).ok_or("Unknown profile")?;
                if self.store.config.profiles.len()<=1 {return Err("Keep at least one profile. Create another before deleting this one.".into());}
                let previous=self.store.config.clone();
                self.store.config.remember_profile();
                let active=self.store.config.active_profile==id;
                self.store.config.profiles.remove(index);
                if active {self.store.config.select_profile(0);}
                self.save_config(previous)?;
                self.controller.status.clear();
                self.message="Profile deleted".into();
            }
            "profile.rename" => {
                self.editable()?;
                let name = clean_name(args.get("name").ok_or("Name required")?)?;
                let previous = self.store.config.clone();
                let id = self.store.config.active_profile.clone();
                self.store
                    .config
                    .profiles
                    .iter_mut()
                    .find(|profile| profile.id == id)
                    .ok_or("Unknown profile")?
                    .name = name;
                self.save_config(previous)?;
            }
            "auto_hide" => {
                let previous = self.store.config.clone();
                self.store.config.launcher_to_tray = args
                    .get("value")
                    .and_then(Value::as_bool)
                    .ok_or("Boolean required")?;
                self.save_config(previous)?;
            }
            "theme" => {
                let theme =
                    serde_json::from_value(args.get("value").cloned().ok_or("Theme required")?)
                        .map_err(|_| "Invalid theme")?;
                let previous = self.store.config.clone();
                self.store.config.appearance.theme = theme;
                self.save_config(previous)?;
            }
            "planner.preferences" => {
                let mut preferences: crate::planner::Preferences = serde_json::from_value(args.clone())
                    .map_err(|error| format!("Invalid planner preferences: {error}"))?;
                let previous = self.store.config.clone();
                preferences.hidden_series.clear();
                preferences.hidden_only = false;
                self.store.config.planner = preferences;
                self.save_config(previous)?;
            }
            "planner.reset" => {
                let previous = self.store.config.clone();
                self.store.config.planner.reset();
                self.save_config(previous)?;
            }
            "planner.favorite" => {
                let id = string(args, "id")?;
                self.find_series(id)?;
                let previous = self.store.config.clone();
                if self.store.config.favorite_series.is_none() {
                    let schedule = self.schedule.as_ref().unwrap();
                    self.store.config.favorite_series = Some(
                        self.store
                            .plans
                            .choices
                            .values()
                            .chain(self.store.plans.selections.values().flatten())
                            .filter_map(|plan| {
                                schedule
                                    .plan_index(plan)
                                    .map(|index| schedule.catalog[index].id.clone())
                            })
                            .collect::<BTreeSet<_>>(),
                    );
                }
                let favorites = self.store.config.favorite_series.as_mut().unwrap();
                if !favorites.remove(id) {
                    favorites.insert(id.into());
                }
                self.save_config(previous)?;
            }
            "planner.select" => {
                let id = string(args, "id")?;
                let series = self.find_series(id)?;
                let week = week(args)?;
                let schedule = self.schedule.as_ref().ok_or("Schedule unavailable")?;
                let date = schedule
                    .weeks
                    .iter()
                    .find(|item| item.number == week)
                    .ok_or("Unknown week")?
                    .start
                    .clone();
                let track = schedule
                    .track(week, series)
                    .ok_or("No race in this week")?
                    .to_string();
                let key = schedule.key(week);
                let previous = self.store.plans.clone();
                let mut plans: Vec<_> = self.store.plans.week(&key).into_iter().cloned().collect();
                self.store.plans.remember_week(&key);
                if let Some(index) = plans
                    .iter()
                    .position(|plan| schedule.plan_index(plan) == Some(series))
                {
                    if plans[index].track == track {
                        plans.remove(index);
                    } else {
                        plans[index].track = track;
                    }
                } else {
                    plans.push(Plan {
                        season: schedule.season.clone(),
                        week,
                        date,
                        series,
                        series_id: id.into(),
                        series_name: schedule.catalog[series].name.clone(),
                        track,
                    });
                }
                self.store.plans.set_week(&key, plans);
                self.save_plans(previous)?;
            }
            "planner.undo" => {
                let week = week(args)?;
                let schedule = self.schedule.as_ref().ok_or("Schedule unavailable")?;
                if !schedule.weeks.iter().any(|item| item.number == week) {
                    return Err("Unknown week".into());
                }
                let key = schedule.key(week);
                let previous = self.store.plans.clone();
                self.store.plans.undo(&key);
                self.save_plans(previous)?;
            }
            "refresh" => {
                if self.refresh.is_none() {
                    let root = self.store.root.clone();
                    let previous = self.schedule.clone();
                    let (tx, rx) = mpsc::channel();
                    self.refresh = Some(rx);
                    thread::spawn(move || {
                        let result = season_schedule::refresh(&root, previous.as_ref());
                        let _ = tx.send(result);
                    });
                    self.message = "Refreshing official schedule…".into();
                }
            }
            _ => return Err(format!("Unknown command: {command}")),
        }
        Ok(self.snapshot())
    }
    fn find_series(&self, id: &str) -> Result<usize, String> {
        self.schedule
            .as_ref()
            .and_then(|schedule| schedule.catalog.iter().position(|series| series.id == id))
            .ok_or("Unknown series".into())
    }
    pub fn selected_path(&mut self, path: String, replace: Option<usize>) -> Result<Value, String> {
        self.editable()?;
        if !Path::new(&path).is_file() || !path.to_ascii_lowercase().ends_with(".exe") {
            return Err("Choose an existing executable.".into());
        }
        if let Some(index) = replace {
            if index >= self.store.config.apps.len() {
                return Err("Unknown app".into());
            }
        }
        if self
            .store
            .config
            .apps
            .iter()
            .enumerate()
            .any(|(index, app)| Some(index) != replace && app.matches(&path))
        {
            return Err("This executable is already in the profile.".into());
        }
        let previous = self.store.config.clone();
        let working_dir = Path::new(&path)
            .parent()
            .ok_or("Invalid path")?
            .to_string_lossy()
            .into_owned();
        if let Some(index) = replace {
            let app = &mut self.store.config.apps[index];
            app.path = path;
            app.working_dir = working_dir;
            app.extra_paths.clear();
            app.add_known_processes();
        } else {
            let filename = Path::new(&path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_lowercase();
            let known = crate::discovery::known_apps().into_iter().find(|app| {
                Path::new(&app.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_lowercase()
                    == filename
            });
            let name = known
                .as_ref()
                .map(|app| app.name.clone())
                .unwrap_or_else(|| {
                    Path::new(&path)
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                });
            let id = known
                .map(|app| app.id)
                .filter(|id| !self.store.config.apps.iter().any(|app| &app.id == id))
                .unwrap_or_else(|| format!("app-{}", chrono::Utc::now().timestamp_micros()));
            let mut app = AppSpec {
                id,
                name,
                path,
                working_dir,
                enabled: true,
                tray: false,
                rehide_after_game: false,
                extra_paths: vec![],
            };
            app.add_known_processes();
            self.store.config.apps.push(app);
        }
        self.save_config(previous)?;
        Ok(self.snapshot())
    }
}
fn index(args: &Value, len: usize) -> Result<usize, String> {
    args.get("index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|&value| value < len)
        .ok_or("Unknown app".into())
}
fn week(args: &Value) -> Result<u8, String> {
    args.get("week")
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or("Invalid week".into())
}
fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or(format!("{key} is required"))
}
fn clean_name(value: &Value) -> Result<String, String> {
    let name = value.as_str().ok_or("Name required")?.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err("Use a name between 1 and 100 characters.".into());
    }
    Ok(name.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn season_refresh_clears_previous_plans_and_undo_but_keeps_favorites_and_apps() {
        let mut bridge=bridge("season-transition");
        let old=bridge.schedule.clone().unwrap();let id=old.catalog[0].id.clone();
        bridge.command("planner.select",&json!({"id":id,"week":1})).unwrap();
        let key=old.key(1);let plan=bridge.store.plans.week(&key)[0].clone();
        bridge.store.plans.history.insert(key.clone(),vec![Some(plan.clone())]);bridge.store.plans.removed.insert(key.clone(),plan);
        bridge.store.config.favorite_series=None;bridge.store.save_config().unwrap();bridge.store.save_plans().unwrap();
        let apps=serde_json::to_value(&bridge.store.config.apps).unwrap();let profiles=bridge.snapshot()["profiles"].clone();
        let mut next=old.clone();next.season="2027-S1".into();
        // The downloader has committed its validated schedule when the result arrives.
        storage::save(&bridge.store.root.join("schedule.json"),&next).unwrap();
        let(tx,rx)=mpsc::channel();bridge.refresh=Some(rx);tx.send(Ok(next.clone())).unwrap();bridge.tick();
        assert_eq!(bridge.schedule.as_ref().unwrap().season,"2027-S1");
        assert_eq!(serde_json::to_value(&bridge.store.plans).unwrap(),serde_json::to_value(storage::Plans::default()).unwrap());
        assert!(bridge.store.config.favorite_series.as_ref().unwrap().contains(&id));
        assert_eq!(serde_json::to_value(&bridge.store.config.apps).unwrap(),apps);assert_eq!(bridge.snapshot()["profiles"],profiles);
        assert!(!bridge.store.plans.can_undo(&key));
        let reopened=Bridge::open(bridge.store.root.clone()).unwrap();assert!(reopened.store.plans.choices.is_empty());assert!(reopened.store.config.favorite_series.as_ref().unwrap().contains(&id));
        assert!(storage::load::<storage::Plans>(&bridge.store.root.join("plans.backup.json")).unwrap().choices.contains_key(&key));
        // Repeated refreshes within the current season retain all its selections.
        bridge.command("planner.select",&json!({"id":id,"week":1})).unwrap();let current=serde_json::to_value(&bridge.store.plans).unwrap();
        let(tx,rx)=mpsc::channel();bridge.refresh=Some(rx);tx.send(Ok(next)).unwrap();bridge.tick();assert_eq!(serde_json::to_value(&bridge.store.plans).unwrap(),current);
        let(tx,rx)=mpsc::channel();bridge.refresh=Some(rx);tx.send(Err("invalid PDF".into())).unwrap();bridge.tick();assert_eq!(serde_json::to_value(&bridge.store.plans).unwrap(),current);
    }
    #[test]
    fn restart_finishes_season_cleanup_and_failed_saves_keep_plans_in_memory() {
        let mut bridge=bridge("season-recovery");let old=bridge.schedule.clone().unwrap();let id=old.catalog[0].id.clone();
        bridge.command("planner.select",&json!({"id":id,"week":1})).unwrap();
        bridge.store.config.favorite_series=Some(BTreeSet::from([id.clone()]));bridge.store.save_config().unwrap();
        let mut next=old;next.season="2027-S2".into();
        let plans=serde_json::to_value(&bridge.store.plans).unwrap();bridge.store.warnings.push("fixture storage error".into());
        assert!(bridge.prune_season_plans(&next).is_err());assert_eq!(serde_json::to_value(&bridge.store.plans).unwrap(),plans);
        storage::save(&bridge.store.root.join("schedule.json"),&next).unwrap();
        let reopened=Bridge::open(bridge.store.root.clone()).unwrap();assert!(reopened.store.plans.choices.is_empty());assert_eq!(reopened.store.config.favorite_series,Some(BTreeSet::from([id])));
    }
    #[test]
    fn deleting_profiles_preserves_remaining_settings_and_protects_last_and_live_profiles() {
        let mut bridge=bridge("delete-profiles");
        let original=bridge.snapshot()["apps"].clone();
        let plans=serde_json::to_value(&bridge.store.plans).unwrap();
        bridge.command("profile.create",&json!({"name":"Delete me"})).unwrap();
        let id=bridge.store.config.active_profile.clone();
        bridge.command("profile.delete",&json!({"id":id})).unwrap();
        assert_eq!(bridge.store.config.active_profile,"iracing");assert_eq!(bridge.snapshot()["apps"],original);
        assert_eq!(serde_json::to_value(&bridge.store.plans).unwrap(),plans);
        let reloaded=Bridge::open(bridge.store.root.clone()).unwrap();assert_eq!(reloaded.store.config.profiles.len(),1);
        let saved=std::fs::read(bridge.store.root.join("config.json")).unwrap();
        assert!(bridge.command("profile.delete",&json!({"id":"iracing"})).is_err());
        assert_eq!(std::fs::read(bridge.store.root.join("config.json")).unwrap(),saved);
        bridge.command("profile.create",&json!({"name":"Keep me"})).unwrap();
        let survivor=bridge.store.config.active_profile.clone();
        bridge.controller.phase=Phase::Running;
        assert!(bridge.command("profile.delete",&json!({"id":"iracing"})).is_err());
        bridge.controller.phase=Phase::Idle;
        bridge.command("profile.delete",&json!({"id":"iracing"})).unwrap();
        assert_eq!(bridge.store.config.active_profile,survivor);assert!(bridge.store.config.apps.is_empty());
        assert_eq!(Bridge::open(bridge.store.root.clone()).unwrap().store.config.active_profile,survivor);
    }
    #[test]
    fn detected_apps_are_opt_in_deduplicated_validated_and_persisted() {
        let mut bridge=bridge("discovered-apps");bridge.store.config.apps.clear();
        let path=bridge.store.root.join("SimHubWPF.exe");std::fs::write(&path,b"fixture").unwrap();
        bridge.discovered_apps=vec![AppSpec{id:"simhub".into(),name:"SimHub".into(),path:path.to_string_lossy().into_owned(),working_dir:bridge.store.root.to_string_lossy().into_owned(),enabled:true,tray:false,rehide_after_game:false,extra_paths:vec![]}];
        assert_eq!(bridge.snapshot()["detected_apps"][0]["added"],false);assert!(bridge.store.config.apps.is_empty());
        bridge.command("app.add_detected",&json!({"id":"simhub"})).unwrap();
        assert_eq!(bridge.snapshot()["detected_apps"][0]["added"],true);assert_eq!(bridge.store.config.apps.len(),1);
        assert!(bridge.command("app.add_detected",&json!({"id":"simhub"})).is_err());
        assert!(bridge.command("app.add_detected",&json!({"id":"missing"})).is_err());
        assert_eq!(Bridge::open(bridge.store.root.clone()).unwrap().store.config.apps[0].path,path.to_string_lossy());
        bridge.store.config.apps.clear();bridge.discovered_apps[0].path=bridge.store.root.join("missing.exe").to_string_lossy().into_owned();
        assert!(bridge.command("app.add_detected",&json!({"id":"simhub"})).is_err());assert!(bridge.store.config.apps.is_empty());
        let custom=bridge.store.root.join("portable.exe");std::fs::write(&custom,b"fixture").unwrap();
        bridge.selected_path(custom.to_string_lossy().into_owned(),None).unwrap();assert_eq!(bridge.store.config.apps[0].name,"portable");
        assert!(bridge.selected_path(custom.to_string_lossy().into_owned(),None).is_err());
    }
    fn bridge(label: &str) -> Bridge {
        let root = std::env::temp_dir().join(format!("paddock-web-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        storage::save(&root.join("config.json"), &storage::Config::default()).unwrap();
        Bridge::open(root).unwrap()
    }
    #[test]
    fn presentation_commands_preserve_profile_isolation_and_block_live_edits() {
        let mut bridge = bridge("profiles");
        bridge
            .command(
                "app.update",
                &json!({"index":0,"enabled":false,"tray":true}),
            )
            .unwrap();
        assert!(!bridge.store.config.apps[0].enabled && bridge.store.config.apps[0].tray);
        bridge
            .command("profile.create", &json!({"name":"Other game"}))
            .unwrap();
        assert!(bridge.store.config.apps.is_empty());
        bridge
            .command("profile.select", &json!({"id":"iracing"}))
            .unwrap();
        assert!(!bridge.store.config.apps[0].enabled);
        assert!(bridge.store.config.apps[0].tray);
        assert!(Store::open(bridge.store.root.clone()).unwrap().config.apps[0].tray);
        bridge.controller.phase = Phase::Running;
        assert!(bridge
            .command("profile.create", &json!({"name":"Blocked"}))
            .is_err());
        assert!(bridge.command("app.remove", &json!({"index":0})).is_err());
    }
    #[test]
    fn retired_hidden_filters_do_not_suppress_series_or_change_favorites_and_plans() {
        let mut bridge = bridge("retired-hidden");
        bridge.store.config.planner.classes.clear();
        let baseline = bridge.snapshot()["schedule"]["series"].clone();
        let ids: Vec<_> = baseline.as_array().unwrap().iter().map(|row| row["id"].as_str().unwrap().to_owned()).collect();
        assert!(!ids.is_empty());
        bridge.store.config.planner.hidden_series = ids.iter().cloned().collect();
        bridge.store.config.planner.hidden_only = true;
        let plans = serde_json::to_value(&bridge.store.plans).unwrap();
        assert_eq!(bridge.snapshot()["schedule"]["series"], baseline);
        bridge.store.config.favorite_series = Some(BTreeSet::from([ids[0].clone()]));
        bridge.store.config.planner.favorites_only = true;
        let favorite = bridge.snapshot();
        assert_eq!(favorite["schedule"]["series"].as_array().unwrap().len(), 1);
        assert_eq!(favorite["schedule"]["series"][0]["id"], ids[0]);
        assert_eq!(serde_json::to_value(&bridge.store.plans).unwrap(), plans);
        assert!(bridge.command("planner.hide", &json!({"id":ids[0]})).is_err());
    }
    #[test]
    fn weekly_lineup_tracks_current_plans_independently_of_filters_and_survives_reload() {
        let mut bridge=bridge("weekly-lineup");
        bridge.store.config.planner.classes.clear();
        bridge.store.config.favorite_series=Some(BTreeSet::new());
        // Shift the cached schedule so week 1 contains today, independent of test date.
        let schedule=bridge.schedule.as_mut().unwrap();
        let today=chrono::Utc::now().date_naive();
        let first=chrono::NaiveDate::parse_from_str(&schedule.weeks[0].start,"%Y-%m-%d").unwrap();
        let shift=today-first;
        for week in &mut schedule.weeks {let date=chrono::NaiveDate::parse_from_str(&week.start,"%Y-%m-%d").unwrap();week.start=(date+shift).to_string();}
        for series in &mut schedule.catalog {for entry in &mut series.entries {let date=chrono::NaiveDate::parse_from_str(&entry.start,"%Y-%m-%d").unwrap();entry.start=(date+shift).to_string();}}
        storage::save(&bridge.store.root.join("schedule.json"),schedule).unwrap();
        let first_id=schedule.catalog[0].id.clone();let second_id=schedule.catalog[1].id.clone();
        let track=schedule.catalog[0].entry(&schedule.weeks[0].start).unwrap().track.clone();
        bridge.command("planner.select",&json!({"id":first_id,"week":1})).unwrap();
        bridge.command("planner.select",&json!({"id":second_id,"week":2})).unwrap();
        let current=bridge.snapshot();assert_eq!(current["schedule"]["lineup"].as_array().unwrap().len(),1);
        assert_eq!(current["schedule"]["lineup"][0]["id"],first_id);assert_eq!(current["schedule"]["lineup"][0]["track"],track);
        bridge.store.config.planner.query="no-series-matches-this-query".into();
        bridge.store.config.planner.favorites_only=true;
        let filtered=bridge.snapshot();assert!(filtered["schedule"]["series"].as_array().unwrap().is_empty());assert_eq!(filtered["schedule"]["lineup"],current["schedule"]["lineup"]);
        let reloaded=Bridge::open(bridge.store.root.clone()).unwrap();assert_eq!(reloaded.snapshot()["schedule"]["lineup"],current["schedule"]["lineup"]);
        bridge.command("planner.select",&json!({"id":first_id,"week":1})).unwrap();
        assert!(bridge.snapshot()["schedule"]["lineup"].as_array().unwrap().is_empty());
        assert_eq!(bridge.store.plans.week(&bridge.schedule.as_ref().unwrap().key(2)).len(),1);
    }
    #[test]
    fn favorites_float_above_the_catalog_without_changing_the_sort_within_groups() {
        let mut bridge=bridge("favorite-order");bridge.store.config.planner.classes.clear();bridge.store.config.planner.sort=crate::planner::Sort::Name;
        bridge.store.config.favorite_series=Some(BTreeSet::new());
        let rows=bridge.snapshot()["schedule"]["series"].as_array().unwrap().clone();
        let ids:Vec<_>=rows.iter().map(|row|row["id"].as_str().unwrap().to_owned()).collect();assert!(ids.len()>3);
        let favorites=BTreeSet::from([ids[ids.len()/2].clone(),ids.last().unwrap().clone()]);bridge.store.config.favorite_series=Some(favorites.clone());
        let actual:Vec<_>=bridge.snapshot()["schedule"]["series"].as_array().unwrap().iter().map(|row|row["id"].as_str().unwrap().to_owned()).collect();
        let expected:Vec<_>=ids.iter().filter(|id|favorites.contains(*id)).chain(ids.iter().filter(|id|!favorites.contains(*id))).cloned().collect();assert_eq!(actual,expected);
        bridge.store.config.planner.favorites_only=true;let selected=bridge.snapshot();assert_eq!(selected["schedule"]["series"].as_array().unwrap().len(),2);assert!(selected["schedule"]["series"].as_array().unwrap().iter().all(|row|row["favorite"]==true));
    }
    #[test]
    fn invalid_requests_do_not_modify_files_and_failed_saves_roll_back() {
        let mut bridge = bridge("rollback");
        let path = bridge.store.root.join("config.json");
        let bytes = std::fs::read(&path).unwrap();
        assert!(bridge
            .command("app.update", &json!({"index":999,"enabled":false}))
            .is_err());
        assert!(bridge.command("arbitrary.execute", &json!({})).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let enabled = bridge.store.config.apps[0].enabled;
        assert!(bridge
            .command("app.update", &json!({"index":0,"enabled":!enabled}))
            .is_err());
        assert_eq!(bridge.store.config.apps[0].enabled, enabled);
        std::fs::remove_dir(path).unwrap();
    }
    #[test]
    fn repeated_stop_keeps_cleanup_in_progress_even_after_processes_exit() {
        let mut bridge = bridge("repeat-stop");
        bridge.store.config.apps.clear();
        bridge.controller.phase = Phase::Stopping;
        let (sender, receiver) = std::sync::mpsc::channel();
        bridge.controller.receiver = Some(receiver);
        bridge.command("stop", &json!({})).unwrap();
        assert_eq!(bridge.controller.phase, Phase::Stopping);
        sender.send(app_control::Event::Stopped(vec![])).unwrap();
        bridge.controller.tick(&bridge.store.root, &[]);
        assert_eq!(bridge.controller.phase, Phase::Finished);
    }
    #[test]
    fn web_planner_toggles_independently_persists_and_undo_restores_the_whole_week() {
        let mut bridge = bridge("planning");
        let schedule = bridge.schedule.as_ref().unwrap();
        let first = schedule.catalog[0].id.clone();
        let second = schedule.catalog[1].id.clone();
        bridge
            .command("planner.select", &json!({"id":first,"week":9}))
            .unwrap();
        bridge
            .command("planner.select", &json!({"id":second,"week":9}))
            .unwrap();
        assert_eq!(
            Store::open(bridge.store.root.clone())
                .unwrap()
                .plans
                .week("2026-S4:9")
                .len(),
            2
        );
        bridge
            .command("planner.select", &json!({"id":first,"week":9}))
            .unwrap();
        assert_eq!(bridge.store.plans.week("2026-S4:9")[0].series_id, second);
        bridge.command("planner.undo", &json!({"week":9})).unwrap();
        assert_eq!(bridge.store.plans.week("2026-S4:9").len(), 2);
        let before = std::fs::read(bridge.store.root.join("plans.json")).unwrap();
        assert!(bridge
            .command("planner.select", &json!({"id":first,"week":255}))
            .is_err());
        assert_eq!(
            std::fs::read(bridge.store.root.join("plans.json")).unwrap(),
            before
        );
    }
    #[test]
    fn optional_game_exit_policy_round_trips_through_bridge() {
        let mut bridge=bridge("game-exit-policy");
        assert_eq!(bridge.snapshot()["apps"][0]["rehide_after_game"],false);
        bridge.command("app.update",&json!({"index":0,"rehide_after_game":true})).unwrap();
        assert_eq!(Bridge::open(bridge.store.root.clone()).unwrap().snapshot()["apps"][0]["rehide_after_game"],true);
    }
}
