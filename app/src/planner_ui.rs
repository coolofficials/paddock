use super::*;
use crate::planner::{Mode, Preferences, Sort};
use crate::race_catalog::{Category, License, Series};

const CARD_HEADER: i32 = 88;
const TABLE_HEADER: i32 = 24;
const TABLE_HEIGHT: i32 = 256;
const TABLE_ROW: i32 = 32;
const CARD_HEIGHT: i32 = CARD_HEADER + TABLE_HEADER + TABLE_HEIGHT + 40;
const CARD_GAP: i32 = 16;
const GROUP_HEADER: i32 = 40;

pub(super) fn search_area(layout: &Layout) -> RECT { rect(layout.left, 320, layout.right-44, 358) }
fn planner_accent() -> u32 { design::ACCENT }
fn class_color(class: License) -> u32 {
    let (red, green, blue) = if design::dark() { match class {
        License::Rookie => (248,113,113), License::D => (251,146,60), License::C => (250,204,21),
        License::B => (74,222,128), License::A => (96,165,250), License::Pro => (192,132,252),
    }} else { match class {
        License::Rookie => (153,27,27), License::D => (154,52,18), License::C => (133,77,14),
        License::B => (22,101,52), License::A => (30,64,175), License::Pro => (107,33,168),
    }};
    red | green << 8 | blue << 16
}
fn tint(base: u32, accent: u32, percent: u32) -> u32 {
    let base = design::resolve(base); let accent = design::resolve(accent);
    let channel = |shift: u32| ((((base >> shift) & 255u32)*(100-percent)+((accent >> shift) & 255u32)*percent)/100) << shift;
    channel(0) | channel(8) | channel(16)
}
fn clipped(area: RECT, bounds: RECT) -> Option<RECT> {
    let area = rect(area.left.max(bounds.left), area.top.max(bounds.top), area.right.min(bounds.right), area.bottom.min(bounds.bottom));
    (area.left < area.right && area.top < area.bottom).then_some(area)
}
fn sort_label(sort: Sort) -> &'static str { match sort { Sort::Class => "License", Sort::Name => "Name", Sort::Category => "Discipline" } }
fn class_label(class: License) -> &'static str { if class == License::Pro { "Pro" } else { class.label() } }
fn category_selected(preferences: &Preferences, category: Category) -> bool {
    preferences.categories.contains(&category) || (preferences.categories.is_empty() && preferences.category == Some(category))
}
fn frequency(series: &Series) -> &str {
    let value = series.times.trim();
    if !(value.starts_with("Every ") || value.starts_with("Races every ") || value.starts_with("Race every ")) { return "Official schedule · details"; }
    [" Min entries", " No incident", " Incident", " DQ at", " Qualifying", " Race length"]
        .iter().filter_map(|separator| value.find(separator)).min().map(|end| &value[..end]).unwrap_or(value)
}
fn series_details(series: &Series) -> String {
    format!("{}\n{} · License {}\n\nCars\n{}\n\n{}\n{}\n\nEligibility: {} {:.1} – {} {:.1}", series.name, series.category.label(), series.class.label(), series.vehicles, series.times, series.conditions, series.minimum.label(), series.minimum_sr, series.maximum.label(), series.maximum_sr)
}

impl State {
    pub(super) fn apply_planner_mode(&mut self) {
        self.schedule_view.weekly = self.store.config.planner.mode == Mode::Week;
        self.schedule_view.favorites_only = self.store.config.planner.mode == Mode::Season;
        self.schedule_view.invalidate_measurement();
        self.schedule_view.planner_card_regions.clear();
        self.scroll = 0; self.input.locate_after_draw = None;
    }
    fn save_planner(&mut self, previous: Preferences) {
        let result = self.store.save_config();
        if result.is_err() { self.store.config.planner = previous; self.apply_planner_mode(); }
        self.scroll = 0; self.input.locate_after_draw = None;
        self.report(result, "View saved");
    }
    pub(super) unsafe fn planner_action(&mut self, window: HWND, action: Action) {
        let previous = self.store.config.planner.clone();
        let preserve_scroll = matches!(action, Action::PlannerCollapse(_)); let old_scroll = self.scroll;
        match action {
            Action::PlannerMode(mode) => { self.store.config.planner.mode = mode; self.apply_planner_mode(); }
            Action::PlannerClass(class) => {
                if let Some(class) = class {
                    if !self.store.config.planner.classes.remove(&class) { self.store.config.planner.classes.insert(class); }
                } else { self.store.config.planner.classes.clear(); }
            }
            Action::PlannerBeginner => self.store.config.planner.classes = Preferences::beginner().classes,
            Action::PlannerReset => self.store.config.planner.reset(),
            Action::PlannerFavorites => self.store.config.planner.favorites_only = !self.store.config.planner.favorites_only,
            Action::PlannerHidden => self.store.config.planner.hidden_only = !self.store.config.planner.hidden_only,
            Action::PlannerHide(index) => {
                let Some(series) = self.schedule.as_ref().and_then(|schedule| schedule.catalog.get(index)) else { return; };
                let id = series.id.clone();
                if !self.store.config.planner.hidden_series.remove(&id) { self.store.config.planner.hidden_series.insert(id); }
            }
            Action::PlannerCollapse(class) => {
                if !self.store.config.planner.collapsed_classes.remove(&class) { self.store.config.planner.collapsed_classes.insert(class); }
            }
            Action::PlannerQueryClear => self.store.config.planner.query.clear(),
            Action::PlannerSort => {
                let options = vec!["License".into(), "Name".into(), "Discipline".into()]; let sorts = [Sort::Class, Sort::Name, Sort::Category];
                let Some(index) = self.schedule_menu(window, &Action::PlannerSort, &options, sorts.iter().position(|sort| *sort == self.store.config.planner.sort)) else { return; };
                self.store.config.planner.sort = sorts[index];
            }
            Action::PlannerCategoryToggle(category) => {
                let preferences = &mut self.store.config.planner;
                if preferences.categories.is_empty() { if let Some(legacy) = preferences.category.take() { preferences.categories.insert(legacy); } }
                preferences.category = None;
                if !preferences.categories.remove(&category) { preferences.categories.insert(category); }
            }
            Action::PlannerCategory => {
                let options: Vec<_> = std::iter::once("All disciplines".to_string()).chain(Category::ALL.iter().map(|category| format!("{} {}", if category_selected(&self.store.config.planner, *category) { "✓" } else { "  " }, category.label()))).collect();
                let Some(index) = self.schedule_menu(window, &Action::PlannerCategory, &options, None) else { return; };
                if let Some(index) = index.checked_sub(1) { self.planner_action(window, Action::PlannerCategoryToggle(Category::ALL[index])); return; }
                self.store.config.planner.categories.clear(); self.store.config.planner.category = None;
            }
            Action::PlannerSearch => {
                if window.is_null() { return; }
                if !self.input.reflow { planner_search::focus(window); return; }
                let Some(query) = query_dialog::show(window, &self.store.config.planner.query) else { return; };
                self.store.config.planner.query = query.trim().to_string();
            }
            Action::PlannerWeek(delta) => {
                if let Some(schedule) = &self.schedule {
                    let number = self.schedule_view.week_number(schedule) as i16+delta as i16;
                    if schedule.weeks.iter().any(|week| week.number as i16 == number) {
                        self.schedule_view.week = Some(number as u8); self.schedule_view.planner_card_scroll.clear(); self.scroll = 0;
                    }
                }
                return;
            }
            Action::PlannerToday => { self.schedule_view.week = None; self.schedule_view.planner_card_scroll.clear(); self.scroll = 0; return; }
            Action::PlannerFilters => {
                let mut options = vec![("Disciplines · select multiple".into(),Action::PlannerCategory),("Search query".into(),Action::PlannerSearch),("All licenses".into(),Action::PlannerClass(None)),("Rookie & D".into(),Action::PlannerBeginner)];
                for class in License::ALL { options.push((format!("{} License {}",if self.store.config.planner.classes.contains(&class){"✓"}else{"  "},class.label()),Action::PlannerClass(Some(class)))); }
                options.push((format!("Favorites only: {}",if self.store.config.planner.favorites_only{"On"}else{"Off"}),Action::PlannerFavorites));
                options.push((format!("Hidden series: {}",if self.store.config.planner.hidden_only{"Shown"}else{"Hidden"}),Action::PlannerHidden));
                options.push((format!("Sort: {}",sort_label(self.store.config.planner.sort)),Action::PlannerSort));
                options.push(("Reset filters".into(),Action::PlannerReset));
                if let Some(index)=self.schedule_menu(window,&Action::PlannerFilters,&options.iter().map(|(label,_)|label.clone()).collect::<Vec<_>>(),None) { self.planner_action(window,options[index].1.clone()); }
                return;
            }
            _ => return,
        }
        self.save_planner(previous);
        if preserve_scroll { self.scroll = old_scroll; }
    }
    pub(super) fn planner_indices(&self) -> Vec<usize> {
        self.schedule.as_ref().map(|schedule| self.store.config.planner.indices(schedule, self.schedule_view.week_number(schedule), &schedule_ui::favorite_ids(&self.store, schedule))).unwrap_or_default()
    }
    fn planner_groups(&self) -> Vec<(Option<License>, Vec<usize>)> {
        let indices = self.planner_indices();
        if self.store.config.planner.sort != Sort::Class { return vec![(None, indices)]; }
        let Some(schedule) = &self.schedule else { return vec![]; };
        License::ALL.into_iter().filter_map(|class| {
            let matching: Vec<_> = indices.iter().copied().filter(|index| schedule.catalog[*index].class == class).collect();
            (!matching.is_empty()).then_some((Some(class), matching))
        }).collect()
    }
    pub(super) unsafe fn measure_planner(&mut self, _dc: HDC, layout: &Layout) {
        self.schedule_view.columns = if layout.right-layout.left >= 1168 { 3 } else { 2 };
        self.schedule_view.body_top = 380; self.schedule_view.bar = None;
        let columns = self.schedule_view.columns;
        self.schedule_view.planner_height = self.planner_groups().iter().map(|(class, indices)| {
            let header = if class.is_some() { GROUP_HEADER } else { 0 };
            let collapsed = class.is_some_and(|class| self.store.config.planner.collapsed_classes.contains(&class));
            header + if collapsed { 0 } else { indices.len().div_ceil(columns) as i32*(CARD_HEIGHT+CARD_GAP) }
        }).sum::<i32>().max(140);
    }
    /// Fixed control heights keep dense table buttons inside their own rows.
    unsafe fn planner_control(&mut self, dc: HDC, label: &str, area: RECT, action: Action, selected: bool, accent: u32) {
        let enabled = self.enabled(&action);
        let hovered = enabled && self.input.hovered.as_ref() == Some(&action);
        let pressed = enabled && self.input.pressed.as_ref() == Some(&action) && (hovered || self.input.keyboard_key.is_some());
        let tab = matches!(action, Action::PlannerMode(_));
        let ghost = matches!(action,Action::PlannerReset | Action::PlannerQueryClear | Action::Favorite(_) | Action::PlannerHide(_) | Action::SeriesInfo(_));
        let base = if matches!(action,Action::Favorite(_) | Action::PlannerHide(_) | Action::SeriesInfo(_)) { CARD } else if tab { SIDE } else { BG };
        let background = if pressed { PRESSED } else if hovered || selected && tab { HOVER } else if selected && !ghost { CARD } else { base };
        let border = if tab || ghost { background } else if selected { tint(LINE,TEXT,24) } else { LINE };
        let foreground = if !enabled { MUTED } else if selected && matches!(action,Action::PlannerClass(_) | Action::Favorite(_)) { accent } else if selected || tab { TEXT } else { MUTED };
        let offset = i32::from(pressed);
        if matches!(action,Action::Select(_, _)) {
            let left=(area.left+area.right-18)/2;let top=(area.top+area.bottom-18)/2;
            let check=rect(left,top,left+18,top+18);
            rounded_radius(dc,check,if selected { design::ACCENT } else { background },if selected { design::ACCENT } else { design::CONTROL_BORDER },4);
            if selected { crate::icons::draw(dc,"check",left+2,top+2,14,design::ON_ACCENT); }
        } else {
            rounded(dc,area,background,border);
            if matches!(action,Action::Favorite(_)) {
                crate::icons::draw(dc,"star",(area.left+area.right-16)/2+offset,(area.top+area.bottom-16)/2+offset,16,foreground);
            } else if matches!(action,Action::PlannerClass(_)) {
                let dot=rect(area.left+8,(area.top+area.bottom-4)/2,area.left+12,(area.top+area.bottom+4)/2);
                rounded(dc,dot,accent,accent);
                text_format(dc,label,rect(area.left+14+offset,area.top+offset,area.right-3+offset,area.bottom+offset),FONT_CAPTION,foreground,selected,DT_CENTER|DT_SINGLELINE|DT_VCENTER);
            } else {
                text_format(dc, label, rect(area.left+5+offset,area.top+offset,area.right-5+offset,area.bottom+offset),FONT_CAPTION,foreground,selected,DT_CENTER|DT_SINGLELINE|DT_VCENTER|DT_END_ELLIPSIS);
            }
        }
        if enabled && self.input.focus_visible && self.input.focused.as_ref() == Some(&action) {
            let pen=CreatePen(PS_SOLID,2,design::resolve(design::FOCUS));let previous_pen=SelectObject(dc,pen);let previous_brush=SelectObject(dc,GetStockObject(NULL_BRUSH));
            RoundRect(dc,area.left+2,area.top+2,area.right-2,area.bottom-2,12,12);
            SelectObject(dc,previous_brush);SelectObject(dc,previous_pen);DeleteObject(pen);
        }
        self.hits.push(Hit { rect:area, action, label:label.into() });
    }
    pub(super) unsafe fn draw_planner(&mut self, dc: HDC, layout: &Layout) {
        self.schedule_view.planner_card_regions.clear();
        let Some(schedule) = self.schedule.clone() else { return; };
        let left = layout.left; let right = layout.right;
        let week_number = self.schedule_view.week_number(&schedule);
        let Some(week) = schedule.weeks.iter().find(|week| week.number == week_number) else { return; };
        let current = schedule.active_week(); let preferences = self.store.config.planner.clone();
        let indices = self.planner_indices(); let favorites = schedule_ui::favorite_ids(&self.store, &schedule);
        text(dc,"Choose your series, then select the race weeks you want to drive.",rect(left,124,right,156),FONT_BODY,MUTED,false,false);
        text(dc,&format!("{}  ·  Week {} of {}",schedule.season,week_number,schedule.weeks.len()),rect(left,164,left+250,188),FONT_BODY,TEXT,true,false);
        let badge = current.map(|number| format!("Week {number} now")).unwrap_or_else(|| "Saved season".into());
        self.planner_control(dc,&badge,rect(right-100,164,right,188),Action::PlannerToday,current==Some(week_number),planner_accent());
        text_format(dc,&format!("Starts {} · Resets Tue 09:00 KST",week.start),rect(left+258,164,right-112,188),FONT_CAPTION,MUTED,false,DT_RIGHT|DT_SINGLELINE|DT_VCENTER|DT_END_ELLIPSIS);
        let count=schedule.weeks.len().max(1) as i32;
        for (position, item) in schedule.weeks.iter().enumerate() {
            let start=left+(right-left+3)*position as i32/count; let end=left+(right-left+3)*(position as i32+1)/count-3;
            fill(dc,rect(start,194,end,201),if current.is_some_and(|number| item.number<=number) { planner_accent() } else { LINE });
            text_format(dc,&item.number.to_string(),rect(start,203,end,226),12,if item.number==week_number { planner_accent() } else { MUTED },item.number==week_number,DT_CENTER|DT_SINGLELINE|DT_VCENTER);
            let area=rect(start,190,end,226); let action=Action::Week(item.number);
            if self.input.focus_visible && self.input.focused.as_ref()==Some(&action){let mut focus=area;DrawFocusRect(dc,&mut focus);}
            self.hits.push(Hit{rect:area,action,label:format!("Week {} · Starts {}{}",item.number,item.start,if current==Some(item.number){" · Current week"}else{""})});
        }
        for (position,category) in Category::ALL.into_iter().enumerate() {
            let start=left+(right-left+6)*position as i32/6;let end=left+(right-left+6)*(position as i32+1)/6-6;
            self.planner_control(dc,category.label(),rect(start,232,end,266),Action::PlannerCategoryToggle(category),category_selected(&preferences,category),planner_accent());
        }
        let mut x=left;
        for class in License::ALL {
            let width=if class==License::Pro{48}else{40};
            self.planner_control(dc,class_label(class),rect(x,278,x+width,310),Action::PlannerClass(Some(class)),preferences.classes.is_empty()||preferences.classes.contains(&class),class_color(class));
            if let Some(hit)=self.hits.last_mut(){hit.label=format!("License {} filter",class.label());}x+=width+6;
        }
        self.planner_control(dc,&format!("★ Favorites {}",favorites.len()),rect(left+286,278,left+386,310),Action::PlannerFavorites,preferences.favorites_only,planner_accent());
        self.planner_control(dc,&format!("Hidden {}",preferences.hidden_series.len()),rect(left+394,278,left+490,310),Action::PlannerHidden,preferences.hidden_only,planner_accent());
        self.planner_control(dc,"Reset",rect(left+498,278,left+562,310),Action::PlannerReset,false,planner_accent());
        self.planner_control(dc,&format!("{}  ▾",sort_label(preferences.sort)),rect(right-108,278,right,310),Action::PlannerSort,false,planner_accent());
        let search=search_area(layout); rounded(dc,search,CARD,LINE);
        text(dc,if preferences.query.is_empty(){"Search series, cars or tracks  ·  Ctrl+K"}else{&preferences.query},rect(search.left+12,search.top,search.right-12,search.bottom),FONT_BODY,if preferences.query.is_empty(){MUTED}else{TEXT},false,false);
        self.hits.push(Hit{rect:search,action:Action::PlannerSearch,label:format!("Search series, cars or tracks: {}",preferences.query)});
        self.planner_control(dc,"×",rect(right-36,320,right,358),Action::PlannerQueryClear,false,planner_accent());
        if let Some(hit)=self.hits.last_mut(){hit.label="Clear search".into();}
        text(dc,&format!("{} series · Choose race weeks in each card",indices.len()),rect(left,360,right,378),12,MUTED,false,false);
        let bounds=rect(left,self.schedule_view.body_top,right,layout.bottom);
        let saved=SaveDC(dc); IntersectClipRect(dc,bounds.left,bounds.top,bounds.right,bounds.bottom); let first_hit=self.hits.len();
        if indices.is_empty() {
            text(dc,"No series match your filters.",rect(left+16,bounds.top+16,right-16,bounds.top+52),FONT_SECTION,TEXT,true,false);
            text(dc,"Adjust your search or reset the filters.",rect(left+16,bounds.top+60,right-16,bounds.top+108),FONT_BODY,MUTED,false,true);
        }
        let mut y=bounds.top-self.scroll; let columns=self.schedule_view.columns;
        for (class,group) in self.planner_groups() {
            if let Some(class)=class {
                let collapsed=preferences.collapsed_classes.contains(&class);
                if y+GROUP_HEADER>bounds.top && y<bounds.bottom {
                    let action=Action::PlannerCollapse(class); let area=rect(left,y,right,y+GROUP_HEADER-4);
                    if self.input.hovered.as_ref()==Some(&action){fill(dc,area,HOVER);}
                    let label=format!("{}  LICENSE {}   {}",if collapsed{"▸"}else{"▾"},class_label(class),group.len());
                    text(dc,&label,rect(left+4,y,right-12,y+GROUP_HEADER-6),FONT_BODY,class_color(class),true,false);
                    fill(dc,rect(left+170,y+18,right-4,y+19),LINE);
                    if self.input.focus_visible && self.input.focused.as_ref()==Some(&action){let mut focus=rect(left+2,y+2,right-2,y+GROUP_HEADER-6);DrawFocusRect(dc,&mut focus);}
                    self.hits.push(Hit{rect:area,action,label:format!("License {} · {} series · {}",class.label(),group.len(),if collapsed{"Expand"}else{"Collapse"})});
                }
                y+=GROUP_HEADER; if collapsed {continue;}
            }
            for row in group.chunks(columns) {
                if y+CARD_HEIGHT>bounds.top && y<bounds.bottom {
                    for (column,index) in row.iter().enumerate() {
                        let start=left+(right-left+CARD_GAP)*column as i32/columns as i32;
                        let end=left+(right-left+CARD_GAP)*(column as i32+1)/columns as i32-CARD_GAP;
                        self.draw_planner_card(dc,&schedule,*index,rect(start,y,end,y+CARD_HEIGHT),bounds,&favorites,week_number,current);
                    }
                }
                y+=CARD_HEIGHT+CARD_GAP;
            }
        }
        for hit in &mut self.hits[first_hit..] { hit.rect=clipped(hit.rect,bounds).unwrap_or_else(||rect(0,0,0,0)); }
        self.hits.retain(|hit|hit.rect.left<hit.rect.right&&hit.rect.top<hit.rect.bottom); RestoreDC(dc,saved);
    }
    unsafe fn draw_planner_card(&mut self, dc:HDC, schedule:&Schedule, index:usize, area:RECT, bounds:RECT, favorites:&std::collections::BTreeSet<String>, selected_week:u8, current:Option<u8>) {
        let series=&schedule.catalog[index]; let accent=class_color(series.class);
        let left=area.left;let right=area.right;let top=area.top;
        rounded(dc,area,CARD,LINE);
        rounded(dc,rect(left+12,top+12,left+40,top+40),tint(CARD,accent,22),tint(CARD,accent,50));
        text_format(dc,class_label(series.class),rect(left+13,top+12,left+39,top+40),12,accent,true,DT_CENTER|DT_VCENTER|DT_SINGLELINE);
        text(dc,schedule_ui::series_title(&series.name),rect(left+50,top+8,right-84,top+62),FONT_BODY,TEXT,true,true);
        self.hits.push(Hit{rect:rect(left+48,top+8,right-82,top+62),action:Action::Inspect(series_details(series)),label:format!("{} · Series details",series.name)});
        self.planner_control(dc,if favorites.contains(&series.id){"★"}else{"☆"},rect(right-74,top+12,right-44,top+42),Action::Favorite(index),favorites.contains(&series.id),accent);
        if let Some(hit)=self.hits.last_mut(){hit.label=format!("{} · {} favorite",series.name,if favorites.contains(&series.id){"Remove"}else{"Add"});}
        let hidden=self.store.config.planner.hidden_series.contains(&series.id);
        self.planner_control(dc,if hidden{"+"}else{"−"},rect(right-38,top+12,right-8,top+42),Action::PlannerHide(index),hidden,accent);
        if let Some(hit)=self.hits.last_mut(){hit.label=format!("{} · {}",series.name,if hidden{"Unhide"}else{"Hide series"});}
        let fixed=series.name.split(|character:char| !character.is_alphanumeric()).any(|word|word.eq_ignore_ascii_case("fixed"));
        text(dc,&format!("{}{}",series.category.label(),if fixed{" · Fixed"}else{""}),rect(left+12,top+64,right-12,top+84),FONT_CAPTION,MUTED,false,false);
        let table_top=top+CARD_HEADER+TABLE_HEADER;let table_bottom=table_top+TABLE_HEIGHT;
        let check_left=right-40;let track_left=left+44;let layout_left=track_left+(check_left-track_left)*61/100;
        fill(dc,rect(left+1,top+CARD_HEADER,right-1,table_top),SIDE);
        text(dc,"WK",rect(left+12,top+CARD_HEADER,left+40,table_top),11,MUTED,false,false);
        text(dc,"CIRCUIT",rect(track_left,top+CARD_HEADER,layout_left-6,table_top),11,MUTED,false,false);
        text(dc,"LAYOUT",rect(layout_left,top+CARD_HEADER,check_left-4,table_top),11,MUTED,false,false);
        text_format(dc,"PLAN",rect(check_left,top+CARD_HEADER,right-5,table_top),10,MUTED,false,DT_CENTER|DT_SINGLELINE|DT_VCENTER);
        let max_scroll=(schedule.weeks.len() as i32*TABLE_ROW-TABLE_HEIGHT).max(0);
        let initial=schedule.weeks.iter().position(|week|week.number==selected_week).unwrap_or(0) as i32*TABLE_ROW-(TABLE_HEIGHT-TABLE_ROW)/2;
        let card_scroll=self.schedule_view.planner_card_scroll.entry(series.id.clone()).or_insert(initial.clamp(0,max_scroll));
        *card_scroll=(*card_scroll).clamp(0,max_scroll);let card_scroll=*card_scroll;
        let table=rect(left+1,table_top,right-1,table_bottom);
        if let Some(visible)=clipped(table,bounds){self.schedule_view.planner_card_regions.push((visible,series.id.clone(),max_scroll));}
        let saved=SaveDC(dc);IntersectClipRect(dc,table.left,table.top,table.right,table.bottom);
        for (position,week) in schedule.weeks.iter().enumerate() {
            let row_top=table_top+position as i32*TABLE_ROW-card_scroll;
            if row_top+TABLE_ROW<=table_top||row_top>=table_bottom{continue;}
            let entry=series.entry(&week.start);let is_current=current==Some(week.number);let highlighted=week.number==selected_week;
            let selected=self.selected(&Action::Select(week.number,index));
            if position%2==1{fill(dc,rect(left+1,row_top,right-1,row_top+TABLE_ROW),tint(CARD,SIDE,40));}
            if is_current||highlighted{fill(dc,rect(left+1,row_top,right-1,row_top+TABLE_ROW),if is_current { SELECTED } else { SIDE });}
            if is_current{fill(dc,rect(left+1,row_top,left+3,row_top+TABLE_ROW),accent);}
            fill(dc,rect(left+8,row_top+TABLE_ROW-1,right-8,row_top+TABLE_ROW),tint(CARD,LINE,55));
            let foreground=if current.is_some_and(|number|week.number<number){MUTED}else{TEXT};
            text(dc,&format!("{:02}",week.number),rect(left+12,row_top,left+40,row_top+TABLE_ROW),12,if is_current{accent}else{foreground},is_current,false);
            let track=entry.map(|entry|entry.track.as_str()).unwrap_or("No race"); let (track_name,configuration)=track.split_once(" - ").unwrap_or((track,"—"));
            text(dc,track_name,rect(track_left,row_top,layout_left-8,row_top+TABLE_ROW),12,foreground,is_current,false);
            text(dc,configuration,rect(layout_left,row_top,check_left-6,row_top+TABLE_ROW),11,MUTED,false,false);
            if let Some(entry)=entry {
                let body=format!("{}\n\nWeek {} · Starts {}\nTrack: {}\n\nCars\n{}\n\n{}\n{}",series.name,week.number,entry.start,entry.track,entry.vehicles,series.times,series.conditions);
                if let Some(visible)=clipped(rect(left+5,row_top,check_left-4,row_top+TABLE_ROW-1),table).and_then(|area|clipped(area,bounds)){
                    let action=Action::Inspect(body);
                    if self.input.hovered.as_ref()==Some(&action){fill(dc,rect(visible.left,visible.bottom-1,visible.right,visible.bottom),accent);}
                    if self.input.focus_visible&&self.input.focused.as_ref()==Some(&action){let mut focus=visible;DrawFocusRect(dc,&mut focus);}
                    self.hits.push(Hit{rect:visible,action,label:format!("{} · Week {} · {} · {} · Details",series.name,week.number,entry.start,entry.track)});
                }
                let first=self.hits.len();
                self.planner_control(dc,if selected{"✓"}else{""},rect(check_left+3,row_top+5,right-10,row_top+27),Action::Select(week.number,index),selected,accent);
                if let Some(hit)=self.hits.get_mut(first){hit.rect=clipped(hit.rect,table).and_then(|area|clipped(area,bounds)).unwrap_or_else(||rect(0,0,0,0));hit.label=format!("Week {} · {} · {} · {}",week.number,series.name,track,if selected{"Remove planned race"}else{"Plan race"});}
            }
        }
        if max_scroll>0 {
            let thumb_height=(TABLE_HEIGHT*TABLE_HEIGHT/(max_scroll+TABLE_HEIGHT)).max(24);let thumb_top=table_top+card_scroll*(TABLE_HEIGHT-thumb_height)/max_scroll;
            fill(dc,rect(right-5,thumb_top,right-2,thumb_top+thumb_height),tint(CARD,MUTED,55));
        }
        RestoreDC(dc,saved);fill(dc,rect(left+8,table_bottom,right-8,table_bottom+1),LINE);
        text(dc,frequency(series),rect(left+12,table_bottom+4,right-68,area.bottom-4),11,MUTED,false,false);
        self.planner_control(dc,"Details",rect(right-60,table_bottom+8,right-8,area.bottom-8),Action::SeriesInfo(index),false,accent);
        if let Some(hit)=self.hits.last_mut(){hit.label=format!("{} · Official schedule details",series.name);}
    }
    pub(super) fn planner_items(&self)->Vec<responsive_ui::Item>{
        use responsive_ui::Item;
        let Some(schedule)=&self.schedule else{return vec![Item::text("Refresh the schedule to continue.")];};
        let week=self.schedule_view.week_number(schedule);let preferences=&self.store.config.planner;
        let categories=if preferences.categories.is_empty(){preferences.category.map(|category|category.label().to_string()).unwrap_or_else(||"All disciplines".into())}else{preferences.categories.iter().map(|category|category.label()).collect::<Vec<_>>().join(" · ")};
        let classes=if preferences.classes.is_empty(){"All licenses".into()}else{preferences.classes.iter().map(|class|class.label()).collect::<Vec<_>>().join(" · ")};
        let mut items=vec![Item::action(format!("{} · Week {week} · Choose week",schedule.season),Action::WeekMenu,false),Item::action(format!("Filters · {categories} · {classes}{}{}",if preferences.favorites_only{" · Favorites"}else{""},if preferences.hidden_only{" · Hidden series"}else{""}),Action::PlannerFilters,false),Item::action(if preferences.query.is_empty(){"Search series, cars or tracks".into()}else{format!("Search: {}",preferences.query)},Action::PlannerSearch,false)];
        let indices=self.planner_indices();items.push(Item::text(format!("{} series · 12-week season",indices.len())));
        if schedule.active_week().is_none(){items.push(Item::text("Showing a saved season. Refresh to check the latest schedule."));}
        if indices.is_empty(){items.push(Item::text("No matching series. Adjust your search or filters."));}
        let favorites=schedule_ui::favorite_ids(&self.store,schedule);
        for (class,group) in self.planner_groups(){
            if let Some(class)=class{let collapsed=preferences.collapsed_classes.contains(&class);items.push(Item::action(format!("License {} · {} series · {}",class.label(),group.len(),if collapsed{"Expand"}else{"Collapse"}),Action::PlannerCollapse(class),collapsed));if collapsed{continue;}}
            for index in group{
                let series=&schedule.catalog[index];
                items.push(Item::action(format!("{}\n{} · License {}\n{}",series.name,series.category.label(),series.class.label(),frequency(series)),Action::Inspect(series_details(series)),false));
                items.push(Item::action(if favorites.contains(&series.id){"★ Remove favorite"}else{"☆ Add favorite"},Action::Favorite(index),favorites.contains(&series.id)));
                items.push(Item::action(if preferences.hidden_series.contains(&series.id){"Unhide"}else{"Hide series"},Action::PlannerHide(index),preferences.hidden_series.contains(&series.id)));
                for week in &schedule.weeks{
                    if let Some(entry)=series.entry(&week.start){let selected=self.selected(&Action::Select(week.number,index));items.push(Item::action(format!("Week {} · Starts {}{}\n{}\n{}",week.number,entry.start,if schedule.active_week()==Some(week.number){" · Current week"}else{""},entry.track,entry.vehicles),Action::Select(week.number,index),selected));}
                    else{items.push(Item::text(format!("Week {} · Starts {} · No race",week.number,week.start)));}
                }
            }
        }
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(name: &str) -> Box<State> {
        let mut state = regression_tests::review_state(name);
        let mut schedule: Schedule = serde_json::from_str(include_str!("../assets/schedule-2026-s4.json")).unwrap();
        schedule.expand_catalog(include_str!("../assets/schedule-full-fixture.txt")).unwrap();
        state.schedule=Some(schedule); state.store.config.favorite_series=Some(schedule_ui::favorite_ids(&state.store,state.schedule.as_ref().unwrap())); state.fixture=false; state.apply_planner_mode(); state.schedule_view.week=Some(9); state
    }
    #[test]
    fn planner_controls_are_bounded_and_never_overlap() {
        let mut state=state("planner-layout");
        unsafe { let dc=CreateCompatibleDC(null_mut());
            for (width,height) in [(1000,600),(1034,800),(1360,1010),(1920,1080),(640,480)] {
                for scroll in [0,300,10000] { state.scroll=scroll; state.draw(dc,width,height);
                    assert!(state.hits.iter().any(|hit| hit.action==Action::Week(9) || hit.action==Action::ScheduleTools));
                    for (i,hit) in state.hits.iter().enumerate() {
                        let a=hit.rect;assert!(a.left>=0&&a.top>=0&&a.right<=width&&a.bottom<=height&&a.left<a.right&&a.top<a.bottom,"{:?}",hit.action);
                        for other in &state.hits[i+1..] { let b=other.rect; assert!(!(a.left<b.right&&a.right>b.left&&a.top<b.bottom&&a.bottom>b.top),"overlap {:?} {:?} at {width}",hit.action,other.action); }
                    }
                }
            }
            DeleteDC(dc);
        }
    }
    #[test]
    fn weekly_plan_and_favorites_persist_without_changing_other_settings() {
        let mut state=state("planner-persistence");
        let apps=serde_json::to_value(&state.store.config.apps).unwrap();
        let index=state.planner_indices()[0];
        unsafe { state.action(null_mut(),Action::Select(9,index));state.action(null_mut(),Action::Favorite(index));state.action(null_mut(),Action::PlannerFavorites); }
        assert_eq!(state.planner_indices(),vec![index]);
        let saved=Store::open(state.store.root.clone()).unwrap();
        assert!(saved.config.planner.favorites_only);assert_eq!(saved.plans.week("2026-S4:9").len(),1);assert_eq!(serde_json::to_value(saved.config.apps).unwrap(),apps);
        unsafe { state.action(null_mut(),Action::PlannerMode(Mode::Season));state.action(null_mut(),Action::PlannerMode(Mode::Week)); }
        assert!(state.selected(&Action::Select(9,index)));assert_eq!(state.planner_indices(),vec![index]);
    }
    #[test]
    fn weekly_navigation_stays_in_week_view_and_failed_filter_save_rolls_back() {
        let mut state=state("planner-navigation");
        unsafe { state.action(null_mut(),Action::PlannerWeek(1)); }
        assert_eq!(state.schedule_view.week,Some(10));assert!(state.schedule_view.weekly);
        unsafe { state.action(null_mut(),Action::Week(4)); }
        assert_eq!(state.schedule_view.week,Some(4));assert!(state.schedule_view.weekly);
        let prior=state.store.config.planner.clone();let path=state.store.root.join("config.json");std::fs::remove_file(&path).unwrap();std::fs::create_dir(&path).unwrap();
        unsafe { state.action(null_mut(),Action::PlannerReset); }
        assert_eq!(state.store.config.planner,prior);std::fs::remove_dir(path).unwrap();
    }
}
