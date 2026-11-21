use chrono::{DateTime, Duration, NaiveDate, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{path::Path, ptr::null};
use windows_sys::Win32::Networking::WinHttp::*;
use crate::{race_catalog::{self, Series}, storage::{self, wide, Plan}};

pub const SOURCE: &str = "https://members-assets.iracing.com/public/schedulepdf/SeasonSchedule.pdf";
pub const CARS: [&str; 8] = ["MX-5", "GR86", "F4 Fixed", "Porsche Cup Fixed", "Porsche Cup Open", "F4 America", "F4 Asia Pacific", "F4 Europe"];
const LEGACY_SERIES: [&str; 8] = ["Global Mazda MX-5 Cup", "Toyota GR86 Cup", "FIA Formula 4 Challenge - Fixed", "iRacing Porsche Cup - Fixed", "iRacing Porsche Cup", "FIA F4 Esports Regional Tour - America", "FIA F4 Esports Regional Tour - Asia Pacific", "FIA F4 Esports Regional Tour - Europe"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Week {
    pub number: u8,
    pub start: String,
    pub tracks: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Schedule {
    pub season: String,
    pub source: String,
    pub fetched_at: String,
    pub weeks: Vec<Week>,
    #[serde(default)]
    pub catalog: Vec<Series>,
    #[serde(default)]
    pub catalog_count: usize,
}

impl Schedule {
    pub fn validate(&self) -> Result<(), String> {
        if self.weeks.len() != 12 || self.source != SOURCE { return Err("Invalid schedule source or incomplete weeks in cache.".into()); }
        let mut previous = None;
        for (index, week) in self.weeks.iter().enumerate() {
            let date = NaiveDate::parse_from_str(&week.start, "%Y-%m-%d").map_err(|_| "Invalid cached date")?;
            if week.start.len() != 10 || !week.start.is_ascii() || week.number as usize != index+1 || week.tracks.len() != CARS.len() || week.tracks.iter().any(|track| track.trim().is_empty()) { return Err("Invalid cached week or track".into()); }
            if previous.map(|last| date - last != Duration::days(7)).unwrap_or(false) { return Err("Invalid cached week interval".into()); }
            previous = Some(date);
        }
        if self.catalog_count != self.catalog.len() { return Err("Cached series count does not match.".into()); }
        if !self.catalog.is_empty() {
            for (index, prefix) in LEGACY_SERIES.iter().enumerate() {
                let series = self.catalog.get(index).ok_or("Existing series missing")?;
                if !series.name.starts_with(prefix) || self.weeks.iter().any(|week| series.entry(&week.start).map(|entry| &entry.track) != week.tracks.get(index)) { return Err("Schedule and cached weeks differ. Refresh the official schedule.".into()); }
            }
        }
        race_catalog::validate(&self.catalog)
    }
    pub fn active_week(&self) -> Option<u8> {
        self.active_week_at(Utc::now())
    }
    pub fn active_week_at(&self, now: DateTime<Utc>) -> Option<u8> {
        // Official week dates begin at 00:00 UTC (09:00 in Korea).
        let today = now.date_naive();
        self.weeks.iter().find(|week| {
            NaiveDate::parse_from_str(&week.start, "%Y-%m-%d").ok().and_then(|date| {
                date.checked_add_signed(Duration::days(7)).map(|end| today >= date && today < end)
            }).unwrap_or(false)
        }).map(|week| week.number)
    }
    pub fn is_current(&self) -> bool { self.active_week().is_some() }
    pub fn key(&self, week: u8) -> String { format!("{}:{week}", self.season) }
    pub fn track(&self, number: u8, series: usize) -> Option<&str> {
        let week = self.weeks.iter().find(|week| week.number == number)?;
        if series < CARS.len() { week.tracks.get(series).map(String::as_str) }
        else { self.catalog.get(series)?.entry(&week.start).map(|entry| entry.track.as_str()) }
    }
    pub fn plan_index(&self, plan: &Plan) -> Option<usize> {
        if plan.series_id.is_empty() { (plan.series < CARS.len()).then_some(plan.series) }
        else { self.catalog.iter().position(|series| series.id == plan.series_id) }
    }
    pub fn plan_name<'a>(&'a self, plan: &'a Plan) -> &'a str {
        if !plan.series_name.is_empty() { &plan.series_name }
        else { CARS.get(plan.series).copied().unwrap_or("Saved series") }
    }
    pub fn expand_catalog(&mut self, text: &str) -> Result<(), String> {
        let mut catalog = race_catalog::parse(text)?;
        catalog.sort_by_key(|series| LEGACY_SERIES.iter().position(|prefix| series.name.starts_with(prefix)).unwrap_or(LEGACY_SERIES.len()));
        self.catalog_count = catalog.len();
        self.catalog = catalog;
        self.validate()
    }
}

pub fn parse_text(text: &str) -> Result<Schedule, String> {
    let headings = [
        r"Global Mazda MX-5 Cup(?: by [^\n]*?)?\s*-\s*(\d{4})\s+Season\s+(\d)",
        r"Toyota GR86 Cup(?: by [^\n]*?)?\s*-\s*(\d{4})\s+Season\s+(\d)",
        r"FIA Formula 4 Challenge\s*-\s*Fixed\s*-\s*(\d{4})\s+Season\s+(\d)",
        r"iRacing Porsche Cup\s*-\s*Fixed(?: by [^\n]*?)?\s*-\s*(\d{4})\s+Season\s+(\d)",
        r"iRacing Porsche Cup(?: by [^\n]*?)?\s*-\s*(\d{4})\s+Season\s+(\d)",
        r"FIA F4 Esports Regional Tour\s*-\s*America\s*-\s*(\d{4})\s+Season\s+(\d)",
        r"FIA F4 Esports Regional Tour\s*-\s*Asia Pacific\s*-\s*(\d{4})\s+Season\s+(\d)",
        r"FIA F4 Esports Regional Tour\s*-\s*Europe\s*-\s*(\d{4})\s+Season\s+(\d)",
    ];
    let week_pattern = Regex::new(r"Week\s+(\d{1,2})\s*\((\d{4}-\d{2}-\d{2})\)\s*([^\n\r]+)").unwrap();
    let mut season = String::new();
    let mut columns = vec![];
    for heading in headings {
        let expression = Regex::new(heading).unwrap();
        let mut found = None;
        for capture in expression.captures_iter(text) {
            let after = &text[capture.get(0).unwrap().end()..];
            let entries: Vec<_> = week_pattern.captures_iter(after).take(12).collect();
            if entries.len() != 12 || entries[0].get(0).unwrap().start() > 700 { continue; }
            let label = format!("{}-S{}", &capture[1], &capture[2]);
            if !season.is_empty() && season != label { return Err("Series seasons differ. Previous cache kept.".into()); }
            let mut rows = vec![];
            for (index, entry) in entries.iter().enumerate() {
                let number: u8 = entry[1].parse().map_err(|_| "Invalid week")?;
                if number as usize != index + 1 { return Err("Unable to verify 12 consecutive race weeks.".into()); }
                let date = NaiveDate::parse_from_str(&entry[2], "%Y-%m-%d").map_err(|_| "Invalid date")?;
                let mut track = entry[3].trim().to_string();
                if track.is_empty() || track.starts_with('(') { return Err("Unable to read track.".into()); }
                let mut found_session = false;
                for continuation in after[entry.get(0).unwrap().end()..].lines().map(str::trim).filter(|line| !line.is_empty()).take(4) {
                    if continuation.starts_with('(') { found_session = true; break; }
                    track.push(' '); track.push_str(continuation);
                }
                if !found_session { return Err("Unable to parse wrapped track name.".into()); }
                rows.push((date, track));
            }
            if rows.windows(2).any(|pair| pair[1].0 - pair[0].0 != Duration::days(7)) { return Err("Race weeks are not seven days apart.".into()); }
            season = label;
            found = Some(rows);
            break;
        }
        columns.push(found.ok_or("Unable to verify all 12 weeks in the official PDF.")?);
    }
    let mut weeks = vec![];
    for index in 0..12 {
        if columns.iter().any(|column| column[index].0 != columns[0][index].0) { return Err("Race week start dates differ between series.".into()); }
        weeks.push(Week { number: (index + 1) as u8, start: columns[0][index].0.to_string(), tracks: columns.iter().map(|column| column[index].1.clone()).collect() });
    }
    Ok(Schedule { season, source: SOURCE.into(), fetched_at: Utc::now().to_rfc3339(), weeks, catalog: vec![], catalog_count: 0 })
}

struct HttpHandle(*mut std::ffi::c_void);
impl Drop for HttpHandle { fn drop(&mut self) { unsafe { WinHttpCloseHandle(self.0); } } }
fn checked(handle: *mut std::ffi::c_void) -> Result<HttpHandle, String> {
    if handle.is_null() { Err(std::io::Error::last_os_error().to_string()) } else { Ok(HttpHandle(handle)) }
}

pub fn download() -> Result<Vec<u8>, String> {
    unsafe {
        let session = checked(WinHttpOpen(wide("Paddock/0.1").as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0))?;
        WinHttpSetTimeouts(session.0, 10000, 10000, 15000, 20000);
        let connection = checked(WinHttpConnect(session.0, wide("members-assets.iracing.com").as_ptr(), 443, 0))?;
        let request = checked(WinHttpOpenRequest(connection.0, wide("GET").as_ptr(), wide("/public/schedulepdf/SeasonSchedule.pdf").as_ptr(), null(), null(), null(), WINHTTP_FLAG_SECURE))?;
        if WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0) == 0 || WinHttpReceiveResponse(request.0, std::ptr::null_mut()) == 0 { return Err(std::io::Error::last_os_error().to_string()); }
        let mut status = 0u32;
        let mut size = 4u32;
        if WinHttpQueryHeaders(request.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, null(), &mut status as *mut _ as _, &mut size, std::ptr::null_mut()) == 0 || status != 200 { return Err(format!("Official server response: {status}")); }
        let mut bytes = vec![];
        loop {
            let mut block = [0u8; 16384];
            let mut read = 0;
            if WinHttpReadData(request.0, block.as_mut_ptr() as _, block.len() as u32, &mut read) == 0 { return Err(std::io::Error::last_os_error().to_string()); }
            if read == 0 { break; }
            bytes.extend_from_slice(&block[..read as usize]);
            if bytes.len() > 25_000_000 { return Err("PDF exceeds the expected size.".into()); }
        }
        if !bytes.starts_with(b"%PDF-") { return Err("Official server response is not a PDF.".into()); }
        Ok(bytes)
    }
}

pub fn refresh(root: &Path, previous: Option<&Schedule>) -> Result<Schedule, String> {
    let bytes = download()?;
    import(&bytes, root, previous)
}

pub fn import(bytes: &[u8], root: &Path, previous: Option<&Schedule>) -> Result<Schedule, String> {
    let text = pdf_extract::extract_text_from_mem(bytes).map_err(|error| error.to_string())?;
    storage::atomic_bytes(&root.join("last-extracted.txt"), text.as_bytes())?;
    let mut schedule = parse_text(&text)?;
    schedule.expand_catalog(&text)?;
    if let Some(old) = previous {
        if schedule.weeks[0].start < old.weeks[0].start { return Err(format!("Server PDF is for an older season ({}). Keeping {} cache.", schedule.season, old.season)); }
    }
    storage::atomic_bytes(&root.join(format!("{}.pdf", schedule.season)), bytes)?;
    storage::save(&root.join(format!("{}.json", schedule.season)), &schedule)?;
    storage::save(&root.join("schedule.json"), &schedule)?;
    Ok(schedule)
}

fn expand_legacy_cache(root: &Path, schedule: &Schedule) -> Result<Schedule, String> {
    #[cfg(test)]
    {
        let initial: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).map_err(|error| error.to_string())?;
        if schedule.season == initial.season && schedule.weeks.iter().zip(&initial.weeks).all(|(old, new)| old.number == new.number && old.start == new.start && old.tracks == new.tracks[..3]) {
            return Ok(initial);
        }
    }
    let pdf = root.join(format!("{}.pdf", schedule.season));
    let bytes = std::fs::read(pdf).map_err(|_| "Refresh to load more series. Existing cache and plans kept.")?;
    let text = pdf_extract::extract_text_from_mem(&bytes).map_err(|error| error.to_string())?;
    let expanded = parse_text(&text)?;
    if expanded.season != schedule.season { return Err("Saved PDF is from another season. Refresh the schedule.".into()); }
    Ok(expanded)
}

fn cached_legacy(root: &Path) -> Result<Schedule, String> {
    if root.join("schedule.json").exists() {
        let schedule: Schedule = storage::load(&root.join("schedule.json"))?;
        if schedule.source != SOURCE { return Err("Invalid cached source.".into()); }
        if schedule.weeks.len() == 12 && schedule.weeks.iter().all(|week| week.tracks.len() == 3) {
            let expanded = expand_legacy_cache(root, &schedule)?;
            expanded.validate()?;
            storage::atomic_bytes(&root.join("schedule.3-series.backup.json"), &std::fs::read(root.join("schedule.json")).map_err(|error| error.to_string())?)?;
            storage::save(&root.join("schedule.json"), &expanded)?;
            return Ok(expanded);
        }
        schedule.validate()?; Ok(schedule)
    } else {
        #[cfg(test)]
        {
            let initial: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).map_err(|error| error.to_string())?;
            initial.validate()?;
            storage::save(&root.join("schedule.json"), &initial)?;
            Ok(initial)
        }
        #[cfg(not(test))]
        { Err("Refresh schedule to download the current official season.".into()) }
    }
}

fn cached_catalog_text(root: &Path, schedule: &Schedule) -> Result<String, String> {
    let pdf = root.join(format!("{}.pdf", schedule.season));
    if pdf.exists() {
        return pdf_extract::extract_text_from_mem(&std::fs::read(pdf).map_err(|error| error.to_string())?).map_err(|error| error.to_string());
    }
    #[cfg(test)]
    if schedule.season == "2026-S4" { return Ok(include_str!("../assets/schedule-full-fixture.txt").into()); }
    Err("Refresh to load all series. Saved choices kept.".into())
}

pub fn cached(root: &Path) -> Result<Schedule, String> {
    let mut schedule = cached_legacy(root)?;
    if schedule.catalog.is_empty() {
        schedule.expand_catalog(&cached_catalog_text(root, &schedule)?)?;
    }
    schedule.validate()?;
    Ok(schedule)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_week_changes_at_each_utc_midnight_boundary() {
        let schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
        for (index, week) in schedule.weeks.iter().enumerate() {
            let start = NaiveDate::parse_from_str(&week.start, "%Y-%m-%d").unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc();
            let previous = index.checked_sub(1).map(|previous| schedule.weeks[previous].number);
            assert_eq!(schedule.active_week_at(start - Duration::seconds(1)), previous);
            assert_eq!(schedule.active_week_at(start), Some(week.number));
            assert_eq!(schedule.active_week_at(start + Duration::seconds(1)), Some(week.number));
            let end = start + Duration::days(7);
            let next = schedule.weeks.get(index + 1).map(|next| next.number);
            assert_eq!(schedule.active_week_at(end - Duration::seconds(1)), Some(week.number));
            assert_eq!(schedule.active_week_at(end), next);
            assert_eq!(schedule.active_week_at(end + Duration::seconds(1)), next);
        }
    }
    #[test]
    fn korean_midnight_does_not_advance_the_race_week() {
        let schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
        let midnight_in_korea = DateTime::parse_from_rfc3339("2026-11-10T00:00:00+09:00").unwrap().with_timezone(&Utc);
        assert_eq!(schedule.active_week_at(midnight_in_korea), Some(8));
        assert_eq!(schedule.active_week_at(midnight_in_korea + Duration::hours(9) - Duration::seconds(1)), Some(8));
        assert_eq!(schedule.active_week_at(midnight_in_korea + Duration::hours(9)), Some(9));
    }
    #[test]
    fn active_week_is_absent_outside_the_season() {
        let schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
        for instant in ["2026-09-01T00:00:00Z", "2026-12-15T00:00:00Z"] {
            assert_eq!(schedule.active_week_at(DateTime::parse_from_rfc3339(instant).unwrap().with_timezone(&Utc)), None);
        }
    }
    #[test]
    fn full_catalog_migrates_in_memory_without_touching_existing_eight_series_cache() {
        let root = std::env::temp_dir().join(format!("paddock-full-migration-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let original = include_bytes!("../assets/schedule-2026-s4.json");
        storage::atomic_bytes(&root.join("schedule.json"), original).unwrap();
        let mut expanded = cached(&root).unwrap();
        assert_eq!(expanded.catalog.len(), 154);
        assert_eq!(expanded.catalog_count, 154);
        assert_eq!(std::fs::read(root.join("schedule.json")).unwrap(), original);
        for index in 0..8 { assert_eq!(expanded.track(5, index), Some(expanded.weeks[4].tracks[index].as_str())); }
        expanded.catalog.pop(); assert!(expanded.validate().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn synthetic_eight_series_are_distinct_and_complete() {
        let schedule = parse_text(include_str!("../assets/schedule-fixture.txt")).unwrap();
        assert_eq!(schedule.season, "2026-S4");
        assert_eq!(schedule.weeks.len(), 12);
        assert_eq!(schedule.weeks[4].start, "2026-10-13");
        assert_eq!(schedule.weeks[4].tracks.len(), 8);
        assert_eq!(schedule.weeks[4].tracks[0], "Test Circuit 001 - Layout 5");
        assert_eq!(schedule.weeks[4].tracks[2], "Test Circuit 003 - Layout 5");
        assert_eq!(schedule.weeks[4].tracks[3], "Road America - Full Course");
        assert_eq!(schedule.weeks[4].tracks[6], "Test Circuit 007 - Layout 5");
        assert_eq!(schedule.weeks[11].tracks[7], "Test Circuit 008 - Layout 12");
        schedule.validate().unwrap();
    }
    #[test]
    fn old_cache_expands_without_changing_plan_indices() {
        let root = std::env::temp_dir().join(format!("paddock-migration-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut old: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
        for week in &mut old.weeks { week.tracks.truncate(3); }
        storage::save(&root.join("schedule.json"), &old).unwrap();
        let plan = br#"{"choices":{"2026-S4:5":{"season":"2026-S4","week":5,"date":"2026-10-13","series":2,"track":"Oulton Park"}}}"#;
        storage::atomic_bytes(&root.join("plans.json"), plan).unwrap();
        let expanded = cached(&root).unwrap();
        for (before, after) in old.weeks.iter().zip(&expanded.weeks) { assert_eq!(before.tracks, after.tracks[..3]); }
        assert_eq!(std::fs::read(root.join("plans.json")).unwrap(), plan);
        assert!(root.join("schedule.3-series.backup.json").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn partial_schedule_is_rejected() {
        let fixture = include_str!("../assets/schedule-fixture.txt").replace("Week 12", "Missing 12");
        assert!(parse_text(&fixture).is_err());
    }
    #[test]
    fn replacement_series_and_invalid_cache_are_rejected() {
        let fixture = include_str!("../assets/schedule-fixture.txt").replace("Toyota GR86 Cup by SIMAGIC", "Production Car Challenge by Sim-Lab");
        assert!(parse_text(&fixture).is_err());
        let mut cache = parse_text(include_str!("../assets/schedule-fixture.txt")).unwrap();
        cache.weeks[3].start = "not-a-date".into();
        assert!(cache.validate().is_err());
    }
    #[test]
    fn invalid_pdf_preserves_verified_cache() {
        let root = std::env::temp_dir().join(format!("paddock-cache-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let cache = parse_text(include_str!("../assets/schedule-fixture.txt")).unwrap();
        storage::save(&root.join("schedule.json"), &cache).unwrap();
        let original = std::fs::read(root.join("schedule.json")).unwrap();
        assert!(import(b"Not a PDF", &root, Some(&cache)).is_err());
        assert_eq!(std::fs::read(root.join("schedule.json")).unwrap(), original);
        std::fs::remove_dir_all(root).unwrap();
    }
}
