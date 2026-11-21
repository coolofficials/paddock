use super::*;

pub(super) const LESSONS: [(&str,&str,&str);3]=[
    ("iRacing Driving School","Official fundamentals: braking, racing lines and car control.","https://www.iracing.com/iracing-101-the-iracing-driving-school/"),
    ("Danny Lee · Beginner course","Build consistent technique with a structured beginner course.","https://www.dannylee.co.uk/course"),
    ("Input Telemetry Overlay","An open source tool for reviewing steering, throttle and brake inputs.","https://github.com/mitchwarrenburg/input-telemetry-overlay"),
];

impl State {
    pub(super) unsafe fn profile_action(&mut self,window:HWND,action:Action){
        let previous=self.store.config.clone();
        match action {
            Action::ProfileMenu=>{
                let mut options:Vec<_>=self.store.config.profiles.iter().map(|profile|profile.name.clone()).collect();
                options.extend(["New profile…".into(),"Duplicate profile…".into(),"Rename profile…".into()]);
                let selected=self.store.config.profiles.iter().position(|profile|profile.id==self.store.config.active_profile);
                if let Some(index)=self.schedule_menu(window,&Action::ProfileMenu,&options,selected){
                    let count=self.store.config.profiles.len();
                    self.action(window,if index<count{Action::ProfileSelect(index)}else{match index-count{0=>Action::ProfileNew,1=>Action::ProfileDuplicate,_=>Action::ProfileRename}});
                }return;
            }
            Action::ProfileSelect(index)=>{if !self.store.config.select_profile(index){return;}}
            Action::ProfileNew|Action::ProfileDuplicate=>{
                let duplicate=matches!(action,Action::ProfileDuplicate);
                let Some(name)=query_dialog::name(window,"",if duplicate{"Duplicate game profile"}else{"New game profile"})else{return;};
                self.store.config.remember_profile();
                let id=format!("profile-{}",chrono::Utc::now().timestamp_micros());
                let apps=if duplicate{self.store.config.apps.clone()}else{vec![]};
                self.store.config.profiles.push(storage::AppProfile{id,name,apps,launcher_to_tray:self.store.config.launcher_to_tray});
                self.store.config.select_profile(self.store.config.profiles.len()-1);
            }
            Action::ProfileRename=>{
                let Some(name)=query_dialog::name(window,self.store.config.profile_name(),"Rename game profile")else{return;};
                if let Some(profile)=self.store.config.profiles.iter_mut().find(|profile|profile.id==self.store.config.active_profile){profile.name=name;}
            }
            Action::AppAdd=>{
                let Some(path)=choose_exe(window,"")else{return;};
                if self.store.config.apps.iter().any(|app|app.matches(&path)){self.message="This executable is already in the profile.".into();return;}
                let filename=Path::new(&path).file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                let known=storage::Config::default().apps.into_iter().find(|app|Path::new(&app.path).file_name().map(|name|name.to_string_lossy().to_lowercase())==Some(filename.clone()));
                let name=known.as_ref().map(|app|app.name.clone()).unwrap_or_else(||Path::new(&path).file_stem().unwrap_or_default().to_string_lossy().into_owned());
                let id=known.map(|app|app.id).filter(|id|!self.store.config.apps.iter().any(|app|&app.id==id)).unwrap_or_else(||format!("app-{}",chrono::Utc::now().timestamp_micros()));
                let working_dir=Path::new(&path).parent().unwrap().to_string_lossy().into_owned();
                let mut app=storage::AppSpec{id,name,path,working_dir,enabled:true,tray:false,rehide_after_game:false,extra_paths:vec![]};app.add_known_processes();self.store.config.apps.push(app);
            }
            Action::AppDetails(index)=>{
                let Some(app)=self.store.config.apps.get(index)else{return;};
                let labels=vec!["Rename app…".into(),"Change executable…".into(),"Remove from this profile".into(),"App details".into()];
                let title=app.name.clone();let body=format!("{}\n{}",app.path,self.controller.hide_status(&app.id));
                if let Some(choice)=self.schedule_menu(window,&Action::AppDetails(index),&labels,None){match choice{0=>self.action(window,Action::AppRename(index)),1=>self.action(window,Action::Browse(index)),2=>self.action(window,Action::AppRemove(index)),_=>dialogs::details(window,&title,&body)}}return;
            }
            Action::AppRename(index)=>{
                let Some(app)=self.store.config.apps.get(index)else{return;};
                let Some(name)=query_dialog::name(window,&app.name,"Rename app")else{return;};self.store.config.apps[index].name=name;
            }
            Action::AppRemove(index)=>{if index>=self.store.config.apps.len(){return;}self.store.config.apps.remove(index);}
            _=>return,
        }
        self.controller.status.clear();self.scroll=0;self.save_app_config(previous);
    }
    pub(super) unsafe fn draw_learn(&mut self,dc:HDC,layout:&Layout){
        let left=layout.left;let right=layout.right;
        text(dc,"START WITH THE FUNDAMENTALS",rect(left,128,right,156),11,MUTED,false,false);
        for(index,(title,description,_))in LESSONS.iter().enumerate(){
            let top=176+index as i32*136-self.scroll;
            rounded(dc,rect(left,top,right,top+112),CARD,LINE);
            text(dc,title,rect(left+24,top+16,right-152,top+46),FONT_SECTION,TEXT,true,false);
            text(dc,description,rect(left+24,top+50,right-152,top+94),FONT_BODY,MUTED,false,true);
            self.button(dc,"Open ↗",rect(right-120,top+36,right-24,top+76),Action::LearnOpen(index),false);
        }
    }
}
