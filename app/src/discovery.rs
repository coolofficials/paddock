//! Small, bounded scan of known default install locations. Never executes candidates.
use crate::storage::AppSpec;
use std::path::{Path, PathBuf};

pub fn known_apps() -> Vec<AppSpec> {
    let drive=std::env::var("SystemDrive").unwrap_or_else(|_|"C:".into());
    let mut programs=Vec::new();
    for key in ["ProgramW6432","ProgramFiles","ProgramFiles(x86)"] {
        if let Some(path)=std::env::var_os(key) {let path=PathBuf::from(path);if !programs.contains(&path){programs.push(path);}}
    }
    for suffix in ["Program Files","Program Files (x86)"] {let path=PathBuf::from(format!(r"{drive}\{suffix}"));if !programs.contains(&path){programs.push(path);}}
    let profile=std::env::var_os("USERPROFILE").map(PathBuf::from);
    let local=std::env::var_os("LOCALAPPDATA").map(PathBuf::from).or_else(||profile.as_ref().map(|p|p.join("AppData/Local")));
    let roaming=std::env::var_os("APPDATA").map(PathBuf::from).or_else(||profile.as_ref().map(|p|p.join("AppData/Roaming")));
    catalog(&programs,local.as_deref(),roaming.as_deref())
}

fn catalog(programs:&[PathBuf],local:Option<&Path>,roaming:Option<&Path>)->Vec<AppSpec> {
    let mut result=Vec::new();
    let mut add=|id:&str,name:&str,paths:Vec<PathBuf>| {
        if let Some(path)=paths.iter().find(|p|p.is_file()).or_else(||paths.first()) {
            result.push(AppSpec{id:id.into(),name:name.into(),path:path.to_string_lossy().into_owned(),working_dir:path.parent().unwrap().to_string_lossy().into_owned(),enabled:true,tray:false,rehide_after_game:false,extra_paths:vec![]});
        }
    };
    for (id,name,relatives) in [
        ("iracing","iRacing UI",vec!["iRacing/ui/iRacingUI.exe"]),
        ("simhub","SimHub",vec!["SimHub/SimHubWPF.exe"]),
        ("edge","Edge Overlays",vec!["Edge Overlays/Edge Overlays.exe"]),
        ("paints","Trading Paints",vec!["Rhinode LLC/Trading Paints/Trading Paints.exe"]),
        ("crew","Crew Chief",vec!["Britton IT Ltd/CrewChiefV4/CrewChiefV4.exe"]),
        ("moza","MOZA Pit House",vec!["MOZA Pit House/MOZA Pit House.exe","MOZA Pit House/bin/MOZA Pit House.exe"]),
    ] {add(id,name,programs.iter().flat_map(|root|relatives.iter().map(move |p|root.join(p))).collect());}
    if let Some(root)=local {add("gofast","GO Fast",vec![root.join("GOFast/current/GOFast.App.exe"),root.join("Programs/GO Fast/GOFast.App.exe")]);}
    if let Some(root)=roaming {add("garage61","Garage 61",vec![root.join("garage61-install/garage61-launcher.exe")]);}
    result
}

pub fn scan()->Vec<AppSpec> {known_apps().into_iter().filter(|app|Path::new(&app.path).is_file()).collect()}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scan_resolves_alternate_program_roots_and_omits_missing_apps() {
        let root=std::env::temp_dir().join(format!("paddock-discovery-{}",std::process::id()));
        let programs=vec![root.join("64"),root.join("32")];let local=root.join("local");let roaming=root.join("roaming");
        for file in [programs[1].join("SimHub/SimHubWPF.exe"),local.join("GOFast/current/GOFast.App.exe"),roaming.join("garage61-install/garage61-launcher.exe")] {std::fs::create_dir_all(file.parent().unwrap()).unwrap();std::fs::write(file,b"fixture").unwrap();}
        let found:Vec<_>=catalog(&programs,Some(&local),Some(&roaming)).into_iter().filter(|app|Path::new(&app.path).is_file()).collect();
        assert_eq!(found.len(),3);assert_eq!(found.iter().find(|a|a.id=="simhub").unwrap().path,programs[1].join("SimHub/SimHubWPF.exe").to_string_lossy());
        assert!(found.iter().all(|app|Path::new(&app.working_dir).is_dir()));
    }
}
