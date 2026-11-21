use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};
use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

pub fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }

pub fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|error| error.to_string())?; }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::File::create(&temporary).map_err(|error| error.to_string())?;
    use std::io::Write;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|error| error.to_string())?;
    drop(file);
    let source = wide(&temporary.to_string_lossy());
    let destination = wide(&path.to_string_lossy());
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    atomic_bytes(path, &serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?)
}

pub fn load<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?).map_err(|error| format!("{}: {error}", path.display()))
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AppSpec {
    pub id: String,
    pub name: String,
    pub path: String,
    pub working_dir: String,
    pub enabled: bool,
    pub tray: bool,
    #[serde(default)]
    pub rehide_after_game: bool,
    #[serde(default)]
    pub extra_paths: Vec<String>,
}

impl AppSpec {
    pub fn add_known_processes(&mut self) {
        let directory = Path::new(&self.path).parent().unwrap_or(Path::new("."));
        let relatives: &[&str] = match self.id.as_str() {
            "moza" => &["bin/MOZA Pit House.exe", "bin/ProcessMonitor.exe", "bin/crashpad_handler.exe"],
            "gofast" => &["WebView2/msedgewebview2.exe"],
            "garage61" => &["garage61-agent.exe"],
            _ => &[],
        };
        for relative in relatives {
            let path = directory.join(relative);
            if path.is_file() {
                let value = path.to_string_lossy().replace('/', "\\");
                if !self.extra_paths.iter().any(|candidate| candidate.eq_ignore_ascii_case(&value)) { self.extra_paths.push(value); }
            }
        }
        if self.id=="garage61" {
            let agent_dir=if directory.file_name().is_some_and(|name|name.to_string_lossy().eq_ignore_ascii_case("garage61-install")){directory.parent().unwrap_or(directory).join("garage61")}else{directory.to_owned()};
            if let Ok(entries)=fs::read_dir(agent_dir){for entry in entries.flatten(){
                let name=entry.file_name().to_string_lossy().to_ascii_lowercase();
                if entry.path().is_file() && name.starts_with("garage61-agent-") && (name.ends_with(".exe")||name.ends_with(".exe.update")) {
                    let value=entry.path().to_string_lossy().into_owned();
                    if !self.extra_paths.iter().any(|path|path.eq_ignore_ascii_case(&value)){self.extra_paths.push(value);}
                }
            }}
        }
    }
    pub fn matches(&self, path: &str) -> bool {
        std::iter::once(&self.path).chain(self.extra_paths.iter()).any(|candidate| {
            if candidate.eq_ignore_ascii_case(path) { return true; }
            if Path::new(candidate).file_name().map(|name| name.to_string_lossy().to_lowercase()) != Path::new(path).file_name().map(|name| name.to_string_lossy().to_lowercase()) { return false; }
            let normalized = |value: &str| std::fs::canonicalize(value).ok().map(|path| path.to_string_lossy().trim_start_matches(r"\\?\").to_lowercase());
            match (normalized(candidate), normalized(path)) { (Some(left), Some(right)) => left == right, _ => false }
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AppProfile {
    pub id: String,
    pub name: String,
    pub apps: Vec<AppSpec>,
    pub launcher_to_tray: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub profiles: Vec<AppProfile>,
    #[serde(default)]
    pub active_profile: String,
    #[serde(default = "crate::design::Preferences::legacy")]
    pub appearance: crate::design::Preferences,
    pub apps: Vec<AppSpec>,
    pub launcher_to_tray: bool,
    #[serde(default = "default_comparison")]
    pub comparison: [usize; 3],
    #[serde(default)]
    pub licenses: crate::race_catalog::Licenses,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite_series: Option<std::collections::BTreeSet<String>>,
    #[serde(default)]
    pub planner: crate::planner::Preferences,
}

fn default_comparison() -> [usize; 3] { [0, 1, 2] }

impl Default for Config {
    fn default() -> Self {
        let profile = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
        let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| format!(r"{profile}\AppData\Local"));
        let roaming = std::env::var("APPDATA").unwrap_or_else(|_| format!(r"{profile}\AppData\Roaming"));
        let definitions = [
            ("iracing", "iRacing UI", r"C:\Program Files (x86)\iRacing\ui\iRacingUI.exe".to_string()),
            ("simhub", "SimHub", [r"C:\Program Files (x86)\SimHub\SimHubWPF.exe", r"C:\Program Files\SimHub\SimHubWPF.exe"].into_iter().find(|path| Path::new(path).is_file()).unwrap_or(r"C:\Program Files (x86)\SimHub\SimHubWPF.exe").into()),
            ("gofast", "GO Fast", format!(r"{local}\GOFast\current\GOFast.App.exe")),
            ("edge", "Edge Overlays", r"C:\Program Files\Edge Overlays\Edge Overlays.exe".into()),
            ("garage61", "Garage 61", format!(r"{roaming}\garage61-install\garage61-launcher.exe")),
            ("paints", "Trading Paints", r"C:\Program Files (x86)\Rhinode LLC\Trading Paints\Trading Paints.exe".into()),
            ("crew", "Crew Chief", r"C:\Program Files (x86)\Britton IT Ltd\CrewChiefV4\CrewChiefV4.exe".into()),
        ];
        let apps = definitions.into_iter().map(|(id, name, path)| {
            let working_dir = Path::new(&path).parent().unwrap().to_string_lossy().into_owned();
            let extra_paths = if id == "garage61" { vec![format!(r"{working_dir}\garage61-agent.exe")] } else { vec![] };
            AppSpec { id: id.into(), name: name.into(), path, working_dir, enabled: true, tray: id != "iracing", rehide_after_game: false, extra_paths }
        }).collect();
        Self { profiles: vec![], active_profile: String::new(), appearance: Default::default(), apps, launcher_to_tray: true, comparison: default_comparison(), licenses: Default::default(), favorite_series: None, planner: Default::default() }
    }
}

impl Config {
    pub fn initialize_profiles(&mut self) -> bool {
        if !self.profiles.is_empty() { return false; }
        self.active_profile = "iracing".into();
        self.profiles.push(AppProfile { id:self.active_profile.clone(),name:"iRacing".into(),apps:self.apps.clone(),launcher_to_tray:self.launcher_to_tray }); true
    }
    pub fn remember_profile(&mut self) {
        if let Some(profile)=self.profiles.iter_mut().find(|profile|profile.id==self.active_profile) {
            profile.apps=self.apps.clone();profile.launcher_to_tray=self.launcher_to_tray;
        }
    }
    pub fn select_profile(&mut self,index:usize) -> bool {
        if index>=self.profiles.len(){return false;}
        self.remember_profile();
        let profile=&self.profiles[index];self.active_profile=profile.id.clone();self.apps=profile.apps.clone();self.launcher_to_tray=profile.launcher_to_tray;
        for app in &mut self.apps {app.add_known_processes();}
        true
    }
    pub fn profile_name(&self)->&str {self.profiles.iter().find(|profile|profile.id==self.active_profile).map(|profile|profile.name.as_str()).unwrap_or("iRacing")}
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Plan {
    pub season: String,
    pub week: u8,
    pub date: String,
    pub series: usize,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub series_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub series_name: String,
    pub track: String,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Plans {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub selections: BTreeMap<String, Vec<Plan>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub selection_history: BTreeMap<String, Vec<Vec<Plan>>>,
    pub choices: BTreeMap<String, Plan>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub history: BTreeMap<String, Vec<Option<Plan>>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub removed: BTreeMap<String, Plan>,
}

pub struct Store {
    pub root: PathBuf,
    pub config: Config,
    pub plans: Plans,
    pub warnings: Vec<String>,
}

impl Plans {
    /// Discard obsolete seasonal selections and every route that could undo them.
    pub fn retain_season(&mut self, season: &str) -> bool {
        let prefix=format!("{season}:");
        let mut changed=false;
        macro_rules! keep {($field:ident)=>{{let before=self.$field.len();self.$field.retain(|key,_|key.starts_with(&prefix));changed|=before!=self.$field.len();}};}
        keep!(selections);keep!(selection_history);keep!(choices);keep!(history);keep!(removed);
        changed
    }
    pub fn week(&self, key: &str) -> Vec<&Plan> { self.selections.get(key).map(|plans| plans.iter().collect()).unwrap_or_else(|| self.choices.get(key).into_iter().collect()) }
    pub fn set_week(&mut self, key: &str, plans: Vec<Plan>) {
        if let Some(plan) = plans.last() { self.choices.insert(key.into(), plan.clone()); } else { self.choices.remove(key); }
        self.selections.insert(key.into(), plans);
    }
    pub fn remember_week(&mut self, key: &str) {
        let previous = self.week(key).into_iter().cloned().collect();
        let history = self.selection_history.entry(key.into()).or_default(); history.push(previous);
        if history.len() > 32 { history.remove(0); }
    }
    pub fn can_undo(&self, key: &str) -> bool { self.selection_history.get(key).map(|history| !history.is_empty()).unwrap_or(false) || self.history.get(key).map(|history| !history.is_empty()).unwrap_or(false) || (self.week(key).is_empty() && self.removed.contains_key(key)) }
    pub fn undo(&mut self, key: &str) {
        if let Some(previous) = self.selection_history.get_mut(key).and_then(Vec::pop) { self.set_week(key, previous); return; }
        self.selections.remove(key);
        if let Some(previous) = self.history.get_mut(key).and_then(Vec::pop) {
            self.removed.remove(key);
            if let Some(plan) = previous { self.choices.insert(key.into(), plan); } else { self.choices.remove(key); }
        } else if !self.choices.contains_key(key) {
            if let Some(plan) = self.removed.remove(key) { self.choices.insert(key.into(), plan); }
        }
    }
}

impl Store {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let mut warnings = vec![];
        let mut config: Config = if root.join("config.json").exists() { load(&root.join("config.json"))? } else {
            let mut defaults = Config::default(); defaults.apps.clear(); save(&root.join("config.json"), &defaults)?; defaults
        };
        for spec in &mut config.apps { spec.add_known_processes(); }
        if config.initialize_profiles() {
            let backup=root.join("config.pre-0.19.0.json");
            if !backup.exists(){atomic_bytes(&backup,&fs::read(root.join("config.json")).map_err(|error|error.to_string())?)?;}
            save(&root.join("config.json"),&config)?;
        }
        if config.comparison.iter().any(|series| *series >= crate::season_schedule::CARS.len()) || config.comparison.iter().collect::<std::collections::HashSet<_>>().len() != 3 { config.comparison = default_comparison(); }
        let plans = if root.join("plans.json").exists() {
            match load(&root.join("plans.json")) {
                Ok(value) => value,
                Err(error) => { warnings.push(format!("Plan read failed. Original file kept: {error}")); Plans::default() }
            }
        } else { Plans::default() };
        Ok(Self { root, config, plans, warnings })
    }
    pub fn save_config(&self) -> Result<(), String> { let mut config=self.config.clone();config.remember_profile();save(&self.root.join("config.json"), &config) }
    pub fn save_plans(&self) -> Result<(), String> {
        if !self.warnings.is_empty() { return Err("Repair the damaged plan file first. Original file kept.".into()); }
        if self.root.join("plans.json").exists() {
            fs::copy(self.root.join("plans.json"), self.root.join("plans.backup.json")).map_err(|error| error.to_string())?;
        }
        save(&self.root.join("plans.json"), &self.plans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_app_settings_default_to_no_game_exit_hide() {
        let mut value=serde_json::to_value(&Config::default().apps[0]).unwrap();
        value.as_object_mut().unwrap().remove("rehide_after_game");
        let app:AppSpec=serde_json::from_value(value).unwrap();
        assert!(!app.rehide_after_game);
    }
    #[test]
    fn new_install_starts_with_an_empty_opt_in_app_profile() {
        let root=std::env::temp_dir().join(format!("paddock-new-install-{}",chrono::Utc::now().timestamp_micros()));
        let store=Store::open(root.clone()).unwrap();
        assert!(store.config.apps.is_empty());assert_eq!(store.config.profiles.len(),1);
        assert_eq!(store.config.active_profile,"iracing");
        let reopened=Store::open(root).unwrap();assert!(reopened.config.apps.is_empty());
    }
    #[test]
    fn profiles_isolate_apps_and_preserve_changes_across_reopening() {
        let root=std::env::temp_dir().join(format!("paddock-profiles-{}",std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let mut config=Config::default();config.initialize_profiles();
        config.apps[0].enabled=false;config.apps[0].tray=true;config.apps[1].path=r"D:\Tools\SimHubWPF.exe".into();
        config.profiles.push(AppProfile{id:"other".into(),name:"Other game".into(),apps:vec![],launcher_to_tray:false});
        assert!(config.select_profile(1));assert!(config.apps.is_empty());assert!(!config.launcher_to_tray);
        save(&root.join("config.json"),&config).unwrap();
        let mut store=Store::open(root.clone()).unwrap();assert!(store.config.apps.is_empty());
        assert!(store.config.select_profile(0));assert!(!store.config.apps[0].enabled);assert_eq!(store.config.apps[1].path,r"D:\Tools\SimHubWPF.exe");
        store.config.apps[2].enabled=false;store.save_config().unwrap();
        let mut reopened=Store::open(root).unwrap();reopened.config.select_profile(1);reopened.config.select_profile(0);
        assert!(!reopened.config.apps[2].enabled);assert!(reopened.config.apps[0].tray);
    }
    #[test]
    fn garage_agent_discovery_includes_versioned_runtime_but_not_unrelated_programs() {
        let root=std::env::temp_dir().join(format!("paddock-garage-scope-{}",std::process::id()));
        let install=root.join("garage61-install");let runtime=root.join("garage61");fs::create_dir_all(&install).unwrap();fs::create_dir_all(&runtime).unwrap();
        let agent=runtime.join("garage61-agent-20261008125121-34ed9ae.exe");fs::write(&agent,b"fixture").unwrap();
        let outsider=runtime.join("unrelated.exe");fs::write(&outsider,b"fixture").unwrap();
        let mut spec=Config::default().apps.into_iter().find(|app|app.id=="garage61").unwrap();spec.path=install.join("garage61-launcher.exe").to_string_lossy().into_owned();spec.extra_paths.clear();spec.add_known_processes();
        assert!(spec.matches(&agent.to_string_lossy()));assert!(!spec.matches(&outsider.to_string_lossy()));
        assert!(!spec.matches(&root.join("unrelated").join(agent.file_name().unwrap()).to_string_lossy()));
        let newer=runtime.join("garage61-agent-20261120123456-deadbeef.exe");fs::write(&newer,b"fixture").unwrap();spec.add_known_processes();assert!(spec.matches(&newer.to_string_lossy()));
    }
    #[test]
    fn legacy_config_receives_planner_defaults_without_changing_saved_preferences() {
        let legacy = serde_json::json!({
            "apps": [{ "id": "custom", "name": "Custom launcher", "path": "C:\\Custom\\app.exe", "working_dir": "C:\\Custom", "enabled": false, "tray": false, "extra_paths": [] }],
            "launcher_to_tray": false,
            "comparison": [7, 6, 5],
            "licenses": { "SportsCar": "D" },
            "favorite_series": ["SportsCar:my-series"]
        });
        let config: Config = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(config.planner, crate::planner::Preferences::beginner());
        let mut saved = serde_json::to_value(&config).unwrap();
        assert_eq!(saved["apps"][0]["rehide_after_game"], false);
        saved["apps"][0].as_object_mut().unwrap().remove("rehide_after_game");
        for (key, value) in legacy.as_object().unwrap() { assert_eq!(&saved[key], value, "legacy field changed: {key}"); }
        let reopened: Config = serde_json::from_value(saved).unwrap();
        assert_eq!(reopened.planner, config.planner);
        assert_eq!(reopened.favorite_series, config.favorite_series);
    }
    #[test]
    fn planner_config_round_trip_preserves_explicit_all_classes_and_view() {
        let mut config = Config::default();
        config.planner.reset();
        config.planner.category = Some(crate::race_catalog::Category::Formula);
        config.planner.query = "  Formula\tOkayama ".into();
        config.planner.favorites_only = true;
        config.planner.sort = crate::planner::Sort::Name;
        config.planner.mode = crate::planner::Mode::Season;
        config.favorite_series = Some(std::collections::BTreeSet::from(["Formula:f4".into()]));
        let reopened: Config = serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(reopened.planner, config.planner);
        assert!(reopened.planner.classes.is_empty());
        assert_eq!(reopened.favorite_series, config.favorite_series);
    }
    #[test]
    fn scope_is_exact_executable_path() {
        let spec = Config::default().apps.remove(1);
        assert!(spec.matches(&spec.path.to_uppercase()));
        assert!(!spec.matches(&format!("{}-service", spec.path)));
        assert!(!spec.matches(r"C:\Other\MOZA Pit House.exe"));
    }
    #[test]
    fn plans_survive_config_and_binary_independent_storage() {
        let root = std::env::temp_dir().join(format!("paddock-storage-{}", std::process::id()));
        let mut store = Store::open(root.clone()).unwrap();
        store.plans.choices.insert("2026-S4:5".into(), Plan { season: "2026-S4".into(), week: 5, date: "2026-10-13".into(), series: 1, series_id: String::new(), series_name: String::new(), track: "Oulton Park".into() });
        store.save_plans().unwrap();
        store.save_config().unwrap();
        assert_eq!(Store::open(root.clone()).unwrap().plans.choices["2026-S4:5"].track, "Oulton Park");
        fs::remove_dir_all(root).unwrap();
    }
}
#[cfg(test)]
mod multi_plan_tests {
    use super::*;
    fn plan(index: usize) -> Plan { Plan { season: "2026-S4".into(), week: 5, date: "2026-10-13".into(), series: index, series_id: format!("series-{index}"), series_name: format!("Series {index}"), track: "Original layout".into() } }
    #[test]
    fn old_single_choice_migrates_only_on_write_and_undo_restores_the_entire_set() {
        let old = r#"{"choices":{"2026-S4:5":{"season":"2026-S4","week":5,"date":"2026-10-13","series":3,"track":"Old layout"}}}"#;
        let mut plans: Plans = serde_json::from_str(old).unwrap();
        assert!(plans.selections.is_empty()); assert_eq!(plans.week("2026-S4:5")[0].track, "Old layout");
        plans.remember_week("2026-S4:5");
        let mut selected = plans.week("2026-S4:5").into_iter().cloned().collect::<Vec<_>>(); selected.push(plan(2)); plans.set_week("2026-S4:5", selected);
        let mut reopened: Plans = serde_json::from_slice(&serde_json::to_vec(&plans).unwrap()).unwrap();
        assert_eq!(reopened.week("2026-S4:5").len(), 2);
        reopened.undo("2026-S4:5"); assert_eq!(reopened.week("2026-S4:5").len(), 1); assert_eq!(reopened.week("2026-S4:5")[0].track, "Old layout");
    }
    #[test]
    fn empty_multi_selection_and_bounded_history_survive_round_trip() {
        let mut plans = Plans::default();
        for index in 0..40 { plans.remember_week("2026-S4:5"); plans.set_week("2026-S4:5", vec![plan(index)]); }
        assert_eq!(plans.selection_history["2026-S4:5"].len(), 32);
        plans.remember_week("2026-S4:5"); plans.set_week("2026-S4:5", vec![]);
        let mut reopened: Plans = serde_json::from_slice(&serde_json::to_vec(&plans).unwrap()).unwrap();
        assert!(reopened.week("2026-S4:5").is_empty()); assert!(!reopened.choices.contains_key("2026-S4:5"));
        reopened.undo("2026-S4:5"); assert_eq!(reopened.week("2026-S4:5")[0].series, 39);
    }
}

