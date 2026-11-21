export type AppSpec = {
  index: number;
  id: string;
  name: string;
  path: string;
  enabled: boolean;
  tray: boolean;
  rehide_after_game: boolean;
  exists: boolean;
  running: number;
  hide_status: string;
};
export type DetectedApp = {
  id: string;
  name: string;
  path: string;
  added: boolean;
};
export type Preferences = {
  category: string | null;
  categories: string[];
  classes: string[];
  query: string;
  favorites_only: boolean;
  collapsed_classes: string[];
  sort: string;
  mode: string;
};
export type Week = {
  number: number;
  start: string;
  track: string | null;
  vehicles: string | null;
  selected: boolean;
  [key: string]: unknown;
};
export type Series = {
  id: string;
  name: string;
  class: string;
  category: string;
  vehicles: string;
  conditions: string;
  times: string;
  weeks: Week[];
  favorite: boolean;
};
export type LineupEntry = {
  id: string;
  name: string;
  class: string;
  category: string;
  track: string;
  vehicles: string;
};
export type Snapshot = {
  version: string;
  phase: string;
  message: string;
  logs: string[];
  refreshing: boolean;
  apps: AppSpec[];
  detected_apps: DetectedApp[];
  profiles: { id: string; name: string; app_count: number }[];
  active_profile: string;
  profile_name: string;
  auto_hide: boolean;
  theme: "light" | "dark" | "system";
  preferences: Preferences;
  categories: { value: string; label: string }[];
  classes: { value: string; label: string }[];
  schedule: null | {
    season: string;
    fetched_at: string;
    current_week: number | null;
    weeks: { number: number; start: string }[];
    series: Series[];
    lineup: LineupEntry[];
    total_series: number;
    planned: number;
    favorite_count: number;
    can_undo: number[];
  };
};
