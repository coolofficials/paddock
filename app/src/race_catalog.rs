use chrono::{Duration, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category { SportsCar, Formula, Oval, DirtOval, DirtRoad, Unranked }

impl Category {
    pub const ALL: [Self; 6] = [Self::SportsCar, Self::Formula, Self::Oval, Self::DirtOval, Self::DirtRoad, Self::Unranked];
    pub fn label(self) -> &'static str { match self { Self::SportsCar => "Sports Car", Self::Formula => "Formula", Self::Oval => "Oval", Self::DirtOval => "Dirt Oval", Self::DirtRoad => "Dirt Road", Self::Unranked => "Unranked" } }
    fn from_pdf(value: &str) -> Result<Self, String> { match value { "SPORTS CAR" => Ok(Self::SportsCar), "FORMULA CAR" => Ok(Self::Formula), "OVAL" => Ok(Self::Oval), "DIRT OVAL" => Ok(Self::DirtOval), "DIRT ROAD" => Ok(Self::DirtRoad), "UNRANKED" => Ok(Self::Unranked), _ => Err(format!("Unknown discipline: {value}")) } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum License { Rookie, D, C, B, A, Pro }

impl License {
    pub const ALL: [Self; 6] = [Self::Rookie, Self::D, Self::C, Self::B, Self::A, Self::Pro];
    pub fn label(self) -> &'static str { match self { Self::Rookie => "R", Self::D => "D", Self::C => "C", Self::B => "B", Self::A => "A", Self::Pro => "Pro/WC" } }
    fn from_pdf(value: &str) -> Result<Self, String> { match value.trim_start_matches("Class ") { "R" | "Rookie" => Ok(Self::Rookie), "D" => Ok(Self::D), "C" => Ok(Self::C), "B" => Ok(Self::B), "A" => Ok(Self::A), "Pro/WC" => Ok(Self::Pro), _ => Err(format!("Unknown license: {value}")) } }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry { pub start: String, pub track: String, pub vehicles: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Series {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub class: License,
    pub minimum: License,
    pub minimum_sr: f32,
    pub maximum: License,
    pub maximum_sr: f32,
    pub conditions: String,
    pub vehicles: String,
    pub times: String,
    pub entries: Vec<Entry>,
}

impl Series {
    pub fn accepts_class(&self, license: Option<License>) -> bool {
        license.map(|license| license >= self.minimum && license <= self.maximum).unwrap_or(true)
    }
    pub fn sr_condition(&self, license: Option<License>) -> Option<String> {
        let license = license?;
        if license == self.minimum && license == self.maximum { Some(format!("SR {:.1}~{:.1}", self.minimum_sr, self.maximum_sr)) }
        else if license == self.minimum { Some(format!("SR {:.1} or above", self.minimum_sr)) }
        else if license == self.maximum { Some(format!("SR {:.1} or below", self.maximum_sr)) }
        else { None }
    }
    pub fn entry(&self, week_start: &str) -> Option<&Entry> {
        let start = NaiveDate::parse_from_str(week_start, "%Y-%m-%d").ok()?;
        self.entries.iter().find(|entry| NaiveDate::parse_from_str(&entry.start, "%Y-%m-%d").map(|date| date >= start && date < start+Duration::days(7)).unwrap_or(false))
    }
    pub fn minimum_label(&self) -> String { format!("From {} {:.1}", self.minimum.label(), self.minimum_sr) }
}

pub type Licenses = BTreeMap<Category, License>;

fn normalized(value: &str) -> String { value.split_whitespace().collect::<Vec<_>>().join(" ") }

pub fn parse(text: &str) -> Result<Vec<Series>, String> {
    let lines: Vec<_> = text.lines().map(normalized).collect();
    let section = Regex::new(r"^([RDCBA]) Class Series \(([^)]+)\)$").unwrap();
    let license = Regex::new(r"^(Rookie|Class [ABCD]|Pro/WC) (\d\.\d+) --> (Rookie|Class [ABCD]|Pro/WC) (\d\.\d+)(.*)$").unwrap();
    let week = Regex::new(r"^Week (\d+) \((\d{4}-\d{2}-\d{2})\) (.+)$").unwrap();
    let season = Regex::new(r"\s*-?\s*\d{4}\s*-?\s*Season(?:\s+[1-4])?\s*-?\s*").unwrap();
    let mut category = None;
    let mut titles = BTreeMap::new();
    for line in &lines {
        let Some((title, _)) = line.split_once(" . . .") else { continue; };
        if let Some(capture) = section.captures(title) { category = Some((Category::from_pdf(&capture[2])?, License::from_pdf(&capture[1])?)); }
        else if Category::from_pdf(title).is_err() {
            let group = category.ok_or("Unable to read disciplines from the schedule.")?;
            if titles.insert(title.to_string(), group).is_some() { return Err(format!("Duplicate series in table of contents: {title}")); }
        }
    }
    if titles.is_empty() { return Err("Unable to read the full series list.".into()); }
    let headers: Vec<_> = lines.iter().enumerate().filter(|(_, line)| titles.contains_key(*line)).collect();
    if headers.len() != titles.len() || headers.iter().map(|(_, title)| *title).collect::<HashSet<_>>().len() != titles.len() {
        return Err("Series list and schedule body do not match.".into());
    }
    let mut catalog = vec![];
    for (position, (start, title)) in headers.iter().enumerate() {
        let end = headers.get(position+1).map(|(index, _)| *index).unwrap_or(lines.len());
        let body = &lines[start+1..end];
        let bounds: Vec<_> = body.iter().enumerate().filter_map(|(index, line)| license.captures(line).map(|capture| (index, capture))).collect();
        if bounds.len() != 1 { return Err(format!("Unable to read entry requirements: {title}")); }
        let (bound_index, capture) = &bounds[0];
        let vehicles = body[..*bound_index].join(" ");
        let rotating = vehicles == "See race week for cars in use that week.";
        let mut entries = vec![];
        let mut first_week = None;
        for (index, line) in body.iter().enumerate() {
            let Some(capture) = week.captures(line) else {
                if line.starts_with("Week ") { return Err(format!("Invalid week format: {title}")); }
                continue;
            };
            first_week.get_or_insert(index);
            let number: usize = capture[1].parse().map_err(|_| "Invalid week")?;
            if number != entries.len()+1 { return Err(format!("Missing or duplicate week: {title}")); }
            let mut continuation = vec![];
            let mut session = false;
            for line in body[index+1..].iter().take(16) {
                if line.starts_with('(') { session = true; break; }
                if line.is_empty() || (line.len() <= 3 && line.chars().all(|character| character.is_ascii_digit())) || section.is_match(line) || Category::from_pdf(line).is_ok() { continue; }
                continuation.push(line.as_str());
            }
            if !session { return Err(format!("Unable to identify track and session boundaries: {title}")); }
            let mut track = capture[3].to_string();
            if !rotating && !continuation.is_empty() { track.push(' '); track.push_str(&continuation.join(" ")); }
            entries.push(Entry { start: capture[2].into(), track, vehicles: if rotating { continuation.join(" ") } else { vehicles.clone() } });
        }
        let (category, class) = titles[*title];
        let name = season.replace_all(title, " ").trim().trim_matches('-').trim().to_string();
        let name = name.strip_suffix(" Fixed").filter(|prefix| prefix.ends_with("Fixed")).unwrap_or(&name).to_string();
        catalog.push(Series {
            id: format!("{category:?}:{}", normalized(&name).to_lowercase()), name, category, class,
            minimum: License::from_pdf(&capture[1])?, minimum_sr: capture[2].parse().map_err(|_| "Invalid minimum SR")?,
            maximum: License::from_pdf(&capture[3])?, maximum_sr: capture[4].parse().map_err(|_| "Invalid maximum SR")?,
            conditions: capture[5].trim_start_matches(", ").into(), vehicles,
            times: body[bound_index+1..first_week.ok_or(format!("Missing schedule: {title}"))?].join(" "), entries,
        });
    }
    let license_lines = lines.iter().filter(|line| line.contains(" --> ")).count();
    if catalog.len() != license_lines { return Err("Entry requirements and series count do not match.".into()); }
    validate(&catalog)?;
    Ok(catalog)
}

pub fn validate(catalog: &[Series]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for series in catalog {
        if series.id.is_empty() || !ids.insert(&series.id) || series.name.is_empty() || series.vehicles.is_empty() || series.entries.is_empty() || series.minimum > series.maximum || (series.minimum == series.maximum && series.minimum_sr > series.maximum_sr) || !series.minimum_sr.is_finite() || !series.maximum_sr.is_finite() || !(0.0..=4.99).contains(&series.minimum_sr) || !(0.0..=4.99).contains(&series.maximum_sr) { return Err("Invalid series format".into()); }
        let mut previous = None;
        for entry in &series.entries {
            let date = NaiveDate::parse_from_str(&entry.start, "%Y-%m-%d").map_err(|_| "Invalid series date")?;
            if entry.start.len() != 10 || entry.track.is_empty() || entry.vehicles.is_empty() || previous.map(|last| date <= last).unwrap_or(false) { return Err(format!("Invalid series schedule: {}", series.name)); }
            previous = Some(date);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog() -> Vec<Series> { parse(include_str!("../assets/schedule-full-fixture.txt")).unwrap() }
    #[test]
    fn synthetic_catalog_matches_toc_and_covers_all_disciplines() {
        let catalog = catalog();
        assert_eq!(catalog.len(), 154);
        assert_eq!(catalog.iter().map(|series| series.entries.len()).sum::<usize>(), 154*12);
        for category in Category::ALL { assert!(catalog.iter().any(|series| series.category == category)); }
    }
    #[test]
    fn lower_classes_and_conditional_sr_are_not_confused_with_promotions() {
        for series in catalog() {
            for license in License::ALL {
                assert_eq!(series.accepts_class(Some(license)), license >= series.minimum && license <= series.maximum);
                if license > series.minimum && license < series.maximum { assert_eq!(series.sr_condition(Some(license)), None); }
            }
        }
        let catalog = catalog(); let porsche = catalog.iter().find(|series| series.name.starts_with("iRacing Porsche Cup - Fixed")).unwrap();
        assert!(!porsche.accepts_class(Some(License::Rookie))); assert!(porsche.accepts_class(Some(License::D)));
        assert_eq!(porsche.sr_condition(Some(License::D)).as_deref(), Some("SR 4.0 or above"));
        assert_eq!(porsche.sr_condition(Some(License::C)), None);
        let mut restricted = porsche.clone(); restricted.maximum = License::C;
        assert!(!restricted.accepts_class(Some(License::B))); assert!(restricted.accepts_class(None));
    }
    #[test]
    fn irregular_dates_and_rotating_cars_are_preserved() {
        let fixture = include_str!("../assets/schedule-full-fixture.txt")
            .replacen("Global Mazda MX-5 Cup\nRookie", "See race week for cars in use that week.\nRookie", 1)
            .replacen("(Test session)", "Synthetic Rotating Car\n(Test session)", 12)
            .replacen("Week 1 (2026-09-15)", "Week 1 (2026-09-18)", 1);
        let catalog = parse(&fixture).unwrap();
        let series = &catalog[0];
        assert_eq!(series.entry("2026-09-15").unwrap().start, "2026-09-18");
        assert_eq!(series.entries[0].vehicles, "Synthetic Rotating Car");
        assert_eq!(series.entries[0].track, "Test Circuit 001 - Layout 1");
        let mut gap = series.clone(); gap.entries.remove(4);
        assert!(gap.entry("2026-10-13").is_none());
    }
    #[test]
    fn missing_body_week_or_unknown_requirement_is_rejected() {
        let fixture = include_str!("../assets/schedule-full-fixture.txt");
        for broken in [fixture.replacen("Rookie 1.0 -->", "Unknown 1.0 -->", 1), fixture.replacen("Week 12 (", "Week 14 (", 1), fixture.replacen("Global Mazda MX-5 Cup - 2026 Season 4\n", "Missing body heading\n", 1)] { assert!(parse(&broken).is_err()); }
    }
}
