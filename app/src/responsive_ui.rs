use super::*;

pub(super) struct Item { pub text: String, pub action: Option<Action>, pub selected: bool }
impl Item {
    pub fn text(value: impl Into<String>) -> Self { Self { text: value.into(), action: None, selected: false } }
    pub fn action(value: impl Into<String>, action: Action, selected: bool) -> Self { Self { text: value.into(), action: Some(action), selected } }
}

impl State {
    pub(super) unsafe fn compact_menu(&mut self, window: HWND, menu_action: Action) {
        let mut options: Vec<(String, Action)> = match menu_action {
            Action::ScheduleTools if self.schedule_view.weekly => vec![

                ("Previous week".into(), Action::PlannerWeek(-1)), ("Next week".into(), Action::PlannerWeek(1)), ("Today".into(), Action::PlannerToday),
                ("Search series, cars or tracks".into(), Action::PlannerSearch), ("Filters & sort".into(), Action::PlannerFilters),
                ("Refresh schedule".into(), Action::Refresh), ("Official PDF".into(), Action::Source)
            ],
            Action::ScheduleTools => vec![
                ("Season planner".into(), Action::PlannerMode(crate::planner::Mode::Week)),
                (if self.schedule_view.favorites_only { "Series library" } else { "Season" }.into(), Action::FavoritesOnly(!self.schedule_view.favorites_only)),
                ("This week".into(), Action::Week(self.schedule.as_ref().and_then(|schedule| schedule.active_week()).unwrap_or(1))),
                ("Find a series".into(), Action::SeriesJump), ("Refresh schedule".into(), Action::Refresh), ("Official PDF".into(), Action::Source)
            ],
            _ => vec![("Theme".into(), Action::ThemeMenu),("Game profile".into(),Action::ProfileMenu),("Add app".into(),Action::AppAdd)]
        };
        options.extend([ ("Launch all".into(), Action::Start), ("Stop all".into(), Action::Exit), ("Hide Paddock".into(), Action::Tray)]);
        options.retain(|(_, action)| self.enabled(action));
        let labels: Vec<_> = options.iter().map(|(label, _)| label.clone()).collect();
        if let Some(index) = self.schedule_menu(window, &menu_action, &labels, None) { self.action(window, options[index].1.clone()); }
    }

    fn reflow_items(&self) -> Vec<Item> {
        let mut items = vec![];
        if !self.message.is_empty() { items.push(Item::text(self.message.clone())); }
        if self.page==PAGE_LEARN { for(index,(title,description,_))in profiles_ui::LESSONS.iter().enumerate(){items.push(Item::action(format!("{title}\n{description}\nOpen resource ↗"),Action::LearnOpen(index),false));}return items;}
        if self.page == PAGE_APPS {
            items.push(Item::action(format!("Profile: {}",self.store.config.profile_name()),Action::ProfileMenu,false));
            items.push(Item::action("Add app",Action::AppAdd,false));
            items.push(Item::action(if self.store.config.launcher_to_tray { "Hide Paddock after launch: on" } else { "Hide Paddock after launch: off" }, Action::LauncherTray, self.store.config.launcher_to_tray));
            items.push(Item::action("Hide app windows", Action::HideAll, false));
            items.push(Item::action(format!("Theme: {}", self.store.config.appearance.theme.label()), Action::ThemeMenu, false));
            items.push(Item::action("Data folder", Action::Data, false));
            for (index, spec) in self.store.config.apps.iter().enumerate() {
                items.push(Item::text(format!("{}\n{}\n{}", spec.name, spec.path, if self.controller.phase == Phase::Idle { "Ready" } else { self.controller.hide_status(&spec.id) })));
                items.push(Item::action("Open", Action::Restore(index), false));
                items.push(Item::action(if spec.enabled {"Included in this profile"}else{"Excluded from this profile"},Action::Enable(index),spec.enabled));
                if spec.id != "iracing" {
                    items.push(Item::action("Hide", Action::Hide(index), false));
                    items.push(Item::action(if spec.tray { "Start in tray" } else { "Show on launch" }, Action::TrayMode(index), spec.tray));
                }
                items.push(Item::action("Change executable", Action::Browse(index), false));
            }
            return items;
        }

        let Some(schedule) = &self.schedule else { items.push(Item::text("Schedule unavailable. Refresh to try again.")); return items; };
        if self.schedule_view.weekly { items.extend(self.planner_items()); return items; }

        let indices = self.schedule_view.indices(schedule, &self.store);
        if self.schedule_view.favorites_only {

            if indices.is_empty() { items.push(Item::text("Add series from the library to compare them.")); }
            else { for week in &schedule.weeks {
                let key = schedule.key(week.number); let count = self.store.plans.week(&key).len();
                items.push(Item::action(format!("Week {} · {} · {} planned", week.number, week.start, count), Action::SavedInfo(week.number), count > 0));
                if count > 0 { items.push(Item::action("Clear this week", Action::Remove(key.clone()), false)); }
                if self.store.plans.can_undo(&key) { items.push(Item::action("Undo this week", Action::UndoPlan(key), false)); }
                for index in &indices {
                    if let Some(track) = schedule.track(week.number, *index) {
                        let action = Action::Select(week.number, *index); let selected = self.selected(&action);
                        items.push(Item::action(format!("{}\n{}", schedule_ui::series_title(&schedule.catalog[*index].name), track), action, selected));
                    }
                }
            } }
        } else {
            items.push(Item::action(self.schedule_view.filter.label(), Action::CategoryMenu, false));
            items.push(Item::action("Licenses by discipline", Action::LicenseMenu, false));
            items.push(Item::action(if self.schedule_view.conditional { "Show conditional eligibility: on" } else { "Show conditional eligibility: off" }, Action::ConditionalSeries, self.schedule_view.conditional));
            items.push(Item::text("Promotion requires SR 4.0 + MPR for D–B, or SR 3.0 + MPR for Rookie. Update your license after promotion. Content ownership is separate."));
            let favorites = schedule_ui::favorite_ids(&self.store, schedule);
            for index in indices {
                let series = &schedule.catalog[index]; let added = favorites.contains(&series.id);
                items.push(Item::action(format!("{}\n{}", schedule_ui::catalog_label(series), self.series_condition(series)), Action::SeriesInfo(index), false));
                items.push(Item::action(if added { "✓ Added · remove" } else { "+ Add to comparison" }, Action::Favorite(index), added));
            }
        }
        items
    }
    pub(super) unsafe fn draw_reflow(&mut self, dc: HDC, width: i32, height: i32) {
        design::set(self.store.config.appearance); READ_TEXT.with(|reading| reading.borrow_mut().clear());
        self.hits.clear(); self.input.reflow_targets.clear(); self.input.scrollbar = None; self.schedule_view.bar = None;
        fill(dc, rect(0, 0, width, height), BG);
        let right = width-20; let top = if height < 240 { 52 } else { 96 }; self.input.reflow_top = top; let available = (height-top-8).max(1);
        if height >= 240 {
            let tabs = PAGE_TITLES;
            for (index, label) in tabs.into_iter().enumerate() {
                self.button(dc, label, rect(8+index as i32*(width-16)/3, 8, 8+(index as i32+1)*(width-16)/3-4, 44), Action::Page(index as u8), self.page == index as u8);
            }
            let (label, action, menu) = match self.page {
                PAGE_SCHEDULE => (if width < 240 { "Week" } else { "Choose week" }.to_string(), Action::WeekMenu, Action::ScheduleTools),
                _ => ("Data folder".into(), Action::Data, Action::ToolsMenu)
            };
            self.button(dc, &label, rect(8, 52, width/2-4, 88), action, false);
            self.button(dc, if width < 240 { "Menu" } else if self.page == PAGE_SCHEDULE { "Planner menu" } else { "Apps menu" }, rect(width/2+4, 52, width-8, 88), menu, false);
        } else {
            self.button(dc, "Profile", rect(8, 8, width/2-4, 44), Action::ProfileMenu, false);
            self.button(dc, "Menu", rect(width/2+4, 8, width-8, 44), if self.page == PAGE_SCHEDULE { Action::ScheduleTools } else { Action::ToolsMenu }, false);
        }
        let mut items = self.reflow_items();
        if height < 240 {
            let mut navigation = vec![Item::action("Launch all", Action::Start, false), Item::action("Stop all", Action::Exit, false), Item::action("Hide Paddock", Action::Tray, false)];
            for (index, label) in PAGE_TITLES.into_iter().enumerate() { navigation.push(Item::action(label, Action::Page(index as u8), self.page == index as u8)); }
            navigation.append(&mut items); items = navigation;
        } let font = ui_font(FONT_BODY, false); let previous = SelectObject(dc, font);
        let measured: Vec<_> = items.into_iter().map(|item| { let count = wrapped_lines(dc, &item.text, (right-24).max(1)).len() as i32; let size = (count*27+24).max(44)+if matches!(item.action, Some(Action::SeriesInfo(_))) && !self.schedule_view.weekly { 112 } else if matches!(item.action, Some(Action::Select(_, _))) && !self.schedule_view.weekly { 48 } else { 0 }; (item, size) }).collect();
        SelectObject(dc, previous); DeleteObject(font);
        if let Some(target) = self.input.locate_after_draw.take() {
            let mut position = 0;
            for (item, size) in &measured {
                if item.action.as_ref() == Some(&target) { self.scroll = position; if let Some(action) = &item.action { self.input.focused = Some(action.clone()); } break; }
                position += size+8;
            }
        }
        self.input.reflow_height = measured.iter().map(|(_, size)| size+8).sum();
        self.scroll = self.scroll.clamp(0, (self.input.reflow_height-available).max(0));
        let saved = SaveDC(dc); IntersectClipRect(dc, 8, top, width-8, height-8);
        let mut position = 0;
        for (item, size) in measured {
            let area = rect(8, top+position-self.scroll, right, top+position+size-self.scroll);
            if let Some(action) = &item.action { self.input.reflow_targets.push((action.clone(), position, size)); }
            if area.bottom > top && area.top < height-8 {
                if let Some(action) = item.action {
                    if self.schedule_view.weekly && matches!(action, Action::Select(_, _)) {
                        let (foreground, _) = self.surface(dc, area, &action, item.selected);
                        let label = format!("{} {}", if item.selected { "✓" } else { "□" }, item.text);
                        text_format(dc, &label, rect(20, area.top+12, right-12, area.bottom-12), FONT_BODY, foreground, false, DT_WORDBREAK);
                        self.hits.push(Hit { rect: area, action, label });
                    } else if let Action::Select(number, _) = action {
                        rounded(dc, area, if item.selected { SELECTED } else { CARD }, if item.selected { ACCENT } else { LINE });
                        text_format(dc, &item.text, rect(20, area.top+8, right-12, area.bottom-52), FONT_BODY, TEXT, false, DT_WORDBREAK);
                        self.button(dc, if item.selected { "Planned" } else { "Plan race" }, if width < 240 { rect(16, area.bottom-44, right-8, area.bottom-8) } else { rect(right-112, area.bottom-44, right-8, area.bottom-8) }, action, item.selected);
                        if let Some(hit) = self.hits.last_mut() { hit.label = format!("Week {} · {} · {}", number, item.text, if item.selected { "Planned · remove" } else { "Plan race" }); }
                    } else {
                        let (foreground, _) = self.surface(dc, area, &action, item.selected);
                        let mut text_top = area.top+12;
                        if let Action::SeriesInfo(index) = action {
                            if let Some(series) = self.schedule.as_ref().and_then(|schedule| schedule.catalog.get(index)).filter(|_| !self.schedule_view.weekly) { self.car_art(dc, series, rect(20, text_top, right-12, text_top+100)); text_top += 112; }
                        }
                        text_format(dc, &item.text, rect(20, text_top, right-12, area.bottom-12), FONT_BODY, foreground, false, DT_WORDBREAK);
                        self.hits.push(Hit { rect: area, action, label: item.text });
                    }
                } else { text_format(dc, &item.text, rect(20, area.top+12, right-12, area.bottom-12), FONT_BODY, TEXT, false, DT_WORDBREAK); }
            }
            position += size+8;
        }
        for hit in &mut self.hits {
            if hit.rect.top >= top || hit.rect.bottom > top { hit.rect.top = hit.rect.top.max(top); hit.rect.bottom = hit.rect.bottom.min(height-8); }
        }
        self.hits.retain(|hit| hit.rect.bottom > hit.rect.top && hit.rect.right > hit.rect.left);
        RestoreDC(dc, saved);
        if self.input.reflow_height > available {
            let track = rect(width-16, top, width-2, height-8); let thumb_height = (available*available/self.input.reflow_height).max(24);
            let maximum = self.input.reflow_height-available;
            let offset = self.scroll*(available-thumb_height)/maximum;
            let thumb = rect(track.left, top+offset, track.right, top+offset+thumb_height);
            fill(dc, track, LINE); fill(dc, thumb, MUTED); self.input.scrollbar = Some((track, thumb, maximum));
        }
        self.input.reading = READ_TEXT.with(|reading| reading.borrow().join("\n"));
    }
}
