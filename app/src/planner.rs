use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use crate::{race_catalog::{Category, License}, season_schedule::Schedule};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sort { #[default] Class, Name, Category }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode { #[default] Week, Season, Catalog }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Legacy single-category selection, used only when categories is empty.
    pub category: Option<Category>,
    pub categories: BTreeSet<Category>,
    /// Empty means every series class, not the driver's eligibility.
    pub classes: BTreeSet<License>,
    pub query: String,
    pub favorites_only: bool,
    pub hidden_series: BTreeSet<String>,
    pub hidden_only: bool,
    pub collapsed_classes: BTreeSet<License>,
    pub sort: Sort,
    pub mode: Mode,
}

impl Default for Preferences {
    fn default() -> Self { Self::beginner() }
}

impl Preferences {
    pub fn beginner() -> Self {
        Self {
            category: None,
            categories: BTreeSet::new(),
            classes: BTreeSet::from([License::Rookie, License::D]),
            query: String::new(),
            favorites_only: false,
            hidden_series: BTreeSet::new(),
            hidden_only: false,
            collapsed_classes: BTreeSet::new(),
            sort: Sort::default(),
            mode: Mode::default(),
        }
    }

    /// Clear filters while keeping the view and the user's hidden/collapsed items.
    pub fn reset(&mut self) {
        let hidden_series = std::mem::take(&mut self.hidden_series);
        let collapsed_classes = std::mem::take(&mut self.collapsed_classes);
        *self = Self { classes: BTreeSet::new(), mode: self.mode, hidden_series, collapsed_classes, ..Self::default() };
    }

    pub fn indices(&self, schedule: &Schedule, _week: u8, favorites: &BTreeSet<String>) -> Vec<usize> {
        let tokens: Vec<_> = self.query.split_whitespace().map(str::to_lowercase).collect();
        let mut matches: Vec<_> = schedule.catalog.iter().enumerate().filter_map(|(index, series)| {
            // The PDF also includes annual/special series from other seasons.
            if !schedule.weeks.iter().any(|week| series.entry(&week.start).is_some()) { return None; }
            let category_matches = if self.categories.is_empty() {
                self.category.is_none_or(|category| category == series.category)
            } else { self.categories.contains(&series.category) };
            if !category_matches
                || (!self.classes.is_empty() && !self.classes.contains(&series.class))
                || (self.favorites_only && !favorites.contains(&series.id))
                || self.hidden_series.contains(&series.id) != self.hidden_only { return None; }
            if !tokens.is_empty() {
                let mut searchable = format!("{} {}", series.name, series.vehicles);
                // A season card remains searchable even when its selected week has no race.
                for entry in &series.entries {
                    searchable.push(' '); searchable.push_str(&entry.track);
                    searchable.push(' '); searchable.push_str(&entry.vehicles);
                }
                let searchable = searchable.to_lowercase();
                if !tokens.iter().all(|token| searchable.contains(token)) { return None; }
            }
            Some((index, series.name.to_lowercase()))
        }).collect();
        matches.sort_by(|(left_index, left_name), (right_index, right_name)| {
            let left = &schedule.catalog[*left_index];
            let right = &schedule.catalog[*right_index];
            let order = match self.sort {
                Sort::Class => left.class.cmp(&right.class).then_with(|| left_name.cmp(right_name)),
                Sort::Name => left_name.cmp(right_name).then_with(|| left.class.cmp(&right.class)),
                Sort::Category => left.category.cmp(&right.category).then_with(|| left.class.cmp(&right.class)).then_with(|| left_name.cmp(right_name)),
            };
            order.then_with(|| left.category.cmp(&right.category))
                .then_with(|| left.id.cmp(&right.id))
                .then_with(|| left_index.cmp(right_index))
        });
        matches.into_iter().map(|(index, _)| index).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{race_catalog::{Entry, Series}, season_schedule::{Week, SOURCE}};

    fn series(id: &str, name: &str, category: Category, class: License) -> Series {
        Series {
            id: id.into(), name: name.into(), category, class,
            minimum: License::Rookie, minimum_sr: 1.0, maximum: License::Pro, maximum_sr: 4.99,
            conditions: String::new(), vehicles: "Global Mazda MX-5 Cup".into(), times: String::new(),
            entries: vec![
                Entry { start: "2026-11-10".into(), track: "Okayama - Short".into(), vehicles: "Global Mazda MX-5 Cup".into() },
                Entry { start: "2026-11-17".into(), track: "Lime Rock Park - Classic".into(), vehicles: "Global Mazda MX-5 Cup".into() },
            ],
        }
    }

    fn schedule(catalog: Vec<Series>) -> Schedule {
        Schedule {
            season: "2026-S4".into(), source: SOURCE.into(), fetched_at: String::new(),
            weeks: vec![
                Week { number: 9, start: "2026-11-10".into(), tracks: vec![] },
                Week { number: 10, start: "2026-11-17".into(), tracks: vec![] },
            ], catalog_count: catalog.len(), catalog,
        }
    }

    #[test]
    fn reset_clears_filters_but_keeps_the_view_hidden_series_and_collapsed_groups() {
        let mut preferences = Preferences::default();
        assert_eq!(preferences.classes, BTreeSet::from([License::Rookie, License::D]));
        assert_eq!(preferences.mode, Mode::Week);
        preferences.mode = Mode::Catalog;
        preferences.category = Some(Category::Formula);
        preferences.categories.insert(Category::Oval);
        preferences.query = "F4".into();
        preferences.favorites_only = true;
        preferences.hidden_series.insert("hidden-series".into());
        preferences.hidden_only = true;
        preferences.collapsed_classes.insert(License::D);
        preferences.sort = Sort::Name;
        preferences.reset();
        assert_eq!(preferences.mode, Mode::Catalog);
        assert_eq!(preferences.category, None);
        assert!(preferences.classes.is_empty() && preferences.categories.is_empty() && preferences.query.is_empty());
        assert!(!preferences.favorites_only && !preferences.hidden_only);
        assert_eq!(preferences.hidden_series, BTreeSet::from(["hidden-series".into()]));
        assert_eq!(preferences.collapsed_classes, BTreeSet::from([License::D]));
        assert_eq!(preferences.sort, Sort::Class);
    }

    #[test]
    fn class_union_combines_with_category_and_favorites_by_stable_id() {
        let schedule = schedule(vec![
            series("rookie", "Rookie Cup", Category::SportsCar, License::Rookie),
            series("d", "D Cup", Category::SportsCar, License::D),
            series("c", "C Cup", Category::SportsCar, License::C),
            series("formula", "Formula Cup", Category::Formula, License::D),
        ]);
        let favorites = BTreeSet::from(["d".into(), "c".into(), "formula".into()]);
        let mut preferences = Preferences::default();
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![0, 1, 3]);
        preferences.category = Some(Category::SportsCar);
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![0, 1]);
        preferences.favorites_only = true;
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![1]);
        preferences.classes.clear();
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![1, 2]);
        assert!(preferences.indices(&schedule, 9, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn search_matches_all_tokens_across_the_full_seasons_tracks_and_rotating_vehicles() {
        let mut rotating = series("rotating", "Touring Challenge", Category::SportsCar, License::D);
        rotating.vehicles = "See race week for cars in use that week.".into();
        rotating.entries[0].track = "Nürburgring - Grand Prix".into();
        rotating.entries[0].vehicles = "BMW M2".into();
        rotating.entries[1].vehicles = "Caterham 420R".into();
        let schedule = schedule(vec![rotating]);
        let favorites = BTreeSet::new();
        let mut preferences = Preferences::default();
        preferences.query = "  TOURING\tNÜRBURGRING\n bMw  ".into();
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![0]);
        assert_eq!(preferences.indices(&schedule, 10, &favorites), vec![0]);
        preferences.query = "Caterham".into();
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![0]);
        assert_eq!(preferences.indices(&schedule, 10, &favorites), vec![0]);
        preferences.query = "BMW lime".into();
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![0]);
        preferences.query = " BMW nowhere ".into();
        assert!(preferences.indices(&schedule, 9, &favorites).is_empty());
        preferences.query = " \t\n ".into();
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![0]);
    }

    #[test]
    fn season_cards_include_series_without_a_race_in_the_selected_week() {
        let mut weekend = series("weekend", "Weekend", Category::SportsCar, License::D);
        weekend.entries = vec![Entry { start: "2026-11-13".into(), track: "Okayama".into(), vehicles: "MX-5".into() }];
        let mut boundary = weekend.clone();
        boundary.id = "boundary".into(); boundary.name = "Boundary".into();
        boundary.entries[0].start = "2026-11-17".into();
        let mut old = weekend.clone();
        old.id = "old".into(); old.entries[0].start = "2026-11-09".into();
        let schedule = schedule(vec![weekend, boundary, old]);
        let preferences = Preferences::default();
        let favorites = BTreeSet::new();
        for week in [0, 9, 10, 11] {
            assert_eq!(preferences.indices(&schedule, week, &favorites), vec![1, 0]);
        }
    }

    #[test]
    fn season_catalog_excludes_nonoverlapping_events_but_keeps_a_later_week_race() {
        let mut past = series("past-qualifier", "Past qualifier", Category::SportsCar, License::D);
        past.entries.truncate(1); past.entries[0].start = "2026-05-13".into();
        let mut later = series("later-race", "Later race", Category::SportsCar, License::D);
        later.entries.truncate(1); later.entries[0].start = "2026-11-20".into();
        let mut after_season = series("after-season", "After season", Category::SportsCar, License::D);
        after_season.entries.truncate(1); after_season.entries[0].start = "2026-11-24".into();
        let schedule = schedule(vec![past, later, after_season]);
        assert!(schedule.catalog[1].entry(&schedule.weeks[0].start).is_none());
        assert!(schedule.catalog[1].entry(&schedule.weeks[1].start).is_some());
        assert_eq!(Preferences::default().indices(&schedule, 9, &BTreeSet::new()), vec![1]);
    }

    #[test]
    fn multi_category_union_overrides_legacy_category_and_combines_with_hidden_and_favorites() {
        let schedule = schedule(vec![
            series("sports", "Sports Cup", Category::SportsCar, License::D),
            series("formula", "Formula Cup", Category::Formula, License::D),
            series("oval", "Oval Cup", Category::Oval, License::D),
            series("dirt", "Dirt Cup", Category::DirtOval, License::D),
        ]);
        let favorites = BTreeSet::from(["formula".into(), "oval".into()]);
        let mut preferences: Preferences = serde_json::from_str(r#"{"category":"SportsCar"}"#).unwrap();
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![0]);
        preferences.categories = BTreeSet::from([Category::Formula, Category::Oval]);
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![1, 2]);
        preferences.hidden_series = BTreeSet::from(["formula".into(), "dirt".into()]);
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![2]);
        preferences.hidden_only = true;
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![1]);
        preferences.favorites_only = true;
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![1]);
        assert!(preferences.indices(&schedule, 9, &BTreeSet::new()).is_empty());
        preferences.hidden_series.remove("formula");
        assert!(preferences.indices(&schedule, 9, &favorites).is_empty());
        preferences.hidden_only = false;
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![1, 2]);
    }

    #[test]
    fn sorting_uses_class_then_name_or_name_then_class_with_deterministic_ties() {
        let schedule = schedule(vec![
            series("z", "Alpha", Category::SportsCar, License::D),
            series("rookie", "Zulu", Category::SportsCar, License::Rookie),
            series("a", "alpha", Category::SportsCar, License::D),
            series("c", "ALPHA", Category::SportsCar, License::C),
        ]);
        let favorites = BTreeSet::new();
        let mut preferences = Preferences::default(); preferences.reset();
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![1, 2, 0, 3]);
        preferences.sort = Sort::Name;
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![2, 0, 3, 1]);
        let mut schedule = schedule;
        schedule.catalog[1].category = Category::Oval;
        schedule.catalog[2].category = Category::Formula;
        preferences.sort = Sort::Category;
        assert_eq!(preferences.indices(&schedule, 9, &favorites), vec![0, 3, 2, 1]);
    }

    #[test]
    fn partial_preferences_default_missing_fields_but_keep_explicit_all_classes() {
        let partial: Preferences = serde_json::from_str(r#"{"mode":"Season"}"#).unwrap();
        assert_eq!(partial.mode, Mode::Season);
        assert_eq!(partial.classes, Preferences::beginner().classes);
        assert!(partial.categories.is_empty() && partial.hidden_series.is_empty() && partial.collapsed_classes.is_empty());
        assert!(!partial.hidden_only);
        let all: Preferences = serde_json::from_str(r#"{"classes":[],"query":"  mx-5  "}"#).unwrap();
        assert!(all.classes.is_empty());
        assert_eq!(serde_json::from_str::<Preferences>(&serde_json::to_string(&all).unwrap()).unwrap(), all);
        let mut saved = all;
        saved.categories = BTreeSet::from([Category::SportsCar, Category::Formula]);
        saved.hidden_series.insert("hidden-series".into());
        saved.hidden_only = true;
        saved.collapsed_classes.insert(License::Rookie);
        saved.sort = Sort::Category;
        assert_eq!(serde_json::from_str::<Preferences>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
    }
}
