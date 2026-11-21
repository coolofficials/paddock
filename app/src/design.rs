use serde::{Deserialize, Serialize};
use std::{cell::Cell, sync::OnceLock};
use windows_sys::Win32::{Graphics::Gdi::*, System::Registry::*, UI::{Accessibility::*, WindowsAndMessaging::*}};

const ROLE: u32 = 0x80000000;
pub const CANVAS: u32 = ROLE;
pub const SURFACE: u32 = ROLE+1;
pub const SUBTLE: u32 = ROLE+2;
pub const HOVER: u32 = ROLE+3;
pub const PRESSED: u32 = ROLE+4;
pub const TEXT: u32 = ROLE+5;
pub const SECONDARY: u32 = ROLE+6;
pub const BORDER: u32 = ROLE+7;
pub const CONTROL_BORDER: u32 = ROLE+8;
pub const ACCENT: u32 = ROLE+9;
pub const ACCENT_HOVER: u32 = ROLE+10;
pub const LINK: u32 = ROLE+11;
pub const ON_ACCENT: u32 = ROLE+12;
pub const SELECTION: u32 = ROLE+13;
pub const SELECTION_TEXT: u32 = ROLE+14;
pub const FOCUS: u32 = ROLE+15;
pub const SUCCESS_SURFACE: u32 = ROLE+16;
pub const SUCCESS_TEXT: u32 = ROLE+17;
pub const WARNING_SURFACE: u32 = ROLE+18;
pub const WARNING_TEXT: u32 = ROLE+19;
pub const DANGER_SURFACE: u32 = ROLE+20;
pub const DANGER_TEXT: u32 = ROLE+21;
pub const INFO_SURFACE: u32 = ROLE+22;
pub const INFO_TEXT: u32 = ROLE+23;
const NAMES: [&str; 24] = ["canvas", "surface", "surface-subtle", "surface-hover", "surface-pressed", "text", "text-secondary", "border", "border-control", "accent", "accent-hover", "link", "on-accent", "selection", "selection-text", "focus", "success-surface", "success-text", "warning-surface", "warning-text", "danger-surface", "danger-text", "info-surface", "info-text"];
pub const FONT: &str = "Pretendard Variable";

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme { #[default] System, Light, Dark }
impl Theme {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];
    pub fn label(self) -> &'static str { match self { Self::System => "System", Self::Light => "Light", Self::Dark => "Dark" } }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Density { #[default] Comfortable, Compact }
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Preferences { pub theme: Theme, pub density: Density }
impl Preferences { pub fn legacy() -> Self { Self { theme: Theme::Light, ..Self::default() } } }
#[derive(Clone, Copy)]
struct Context { preferences: Preferences, dark: bool, high_contrast: bool, text_scale: u32 }
thread_local! { static CONTEXT: Cell<Context> = const { Cell::new(Context { preferences: Preferences { theme: Theme::Light, density: Density::Comfortable }, dark: false, high_contrast: false, text_scale: 100 }) }; }

pub fn tokens() -> &'static serde_json::Value {
    static TOKENS: OnceLock<serde_json::Value> = OnceLock::new();
    TOKENS.get_or_init(|| serde_json::from_str(include_str!("../assets/framework/tokens.json")).expect("Bundled design tokens"))
}
pub fn dimension(section: &str, key: &str) -> i32 { tokens()[section][key].as_str().unwrap().trim_end_matches("px").parse().unwrap() }
pub fn set(preferences: Preferences) { CONTEXT.with(|context| { let mut value = context.get(); value.preferences = preferences; context.set(value); }); }
pub fn compact() -> bool { CONTEXT.with(|context| context.get().preferences.density == Density::Compact) }
pub fn control_height() -> i32 { tokens()["density"][if compact() { "compact" } else { "comfortable" }]["control"].as_str().unwrap().trim_end_matches("px").parse().unwrap() }
pub fn row_height() -> i32 { tokens()["density"][if compact() { "compact" } else { "comfortable" }]["row"].as_str().unwrap().trim_end_matches("px").parse().unwrap() }
pub fn dark() -> bool { CONTEXT.with(|context| { let value = context.get(); match value.preferences.theme { Theme::Dark => true, Theme::Light => false, Theme::System => value.dark } }) }
pub fn effective_dpi(dpi: u32) -> i32 { CONTEXT.with(|context| (dpi.max(96)*context.get().text_scale/100) as i32) }
pub fn palette(dark: bool) -> [u32; 24] {
    static LIGHT: OnceLock<[u32; 24]> = OnceLock::new(); static DARK: OnceLock<[u32; 24]> = OnceLock::new();
    *(if dark { &DARK } else { &LIGHT }).get_or_init(|| NAMES.map(|name| { let rgb = u32::from_str_radix(tokens()["themes"][if dark { "dark" } else { "light" }][name].as_str().unwrap().trim_start_matches('#'), 16).unwrap(); ((rgb & 255)<<16) | (rgb & 65280) | (rgb>>16) }))
}
pub fn resolve(role: u32) -> u32 {
    if role & ROLE == 0 { return role; }
    let index = (role-ROLE) as usize;
    if CONTEXT.with(|context| context.get().high_contrast) {
        let system = match role {
            TEXT | SECONDARY | SELECTION_TEXT | SUCCESS_TEXT | WARNING_TEXT | DANGER_TEXT | INFO_TEXT => COLOR_WINDOWTEXT,
            ACCENT | ACCENT_HOVER | LINK | FOCUS => COLOR_HIGHLIGHT,
            ON_ACCENT => COLOR_HIGHLIGHTTEXT,
            BORDER | CONTROL_BORDER => COLOR_WINDOWTEXT,
            _ => COLOR_WINDOW,
        };
        return unsafe { GetSysColor(system) };
    }
    palette(dark())[index]
}
fn registry_number(path: &str, name: &str) -> Option<u32> {
    let mut value = 0u32; let mut size = 4u32;
    let result = unsafe { RegGetValueW(HKEY_CURRENT_USER, crate::storage::wide(path).as_ptr(), crate::storage::wide(name).as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), &mut value as *mut _ as _, &mut size) };
    (result == 0).then_some(value)
}
pub fn refresh_system() {
    let mut contrast: HIGHCONTRASTW = unsafe { std::mem::zeroed() }; contrast.cbSize = std::mem::size_of::<HIGHCONTRASTW>() as u32;
    unsafe { SystemParametersInfoW(SPI_GETHIGHCONTRAST, contrast.cbSize, &mut contrast as *mut _ as _, 0); }
    CONTEXT.with(|context| { let mut value = context.get(); value.dark = registry_number(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "AppsUseLightTheme") == Some(0); value.high_contrast = contrast.dwFlags & HCF_HIGHCONTRASTON != 0; value.text_scale = registry_number(r"Software\Microsoft\Accessibility", "TextScaleFactor").unwrap_or(100).clamp(100, 225); context.set(value); });
}
pub fn ensure_font() -> bool {
    static FONT_HANDLE: OnceLock<usize> = OnceLock::new();
    *FONT_HANDLE.get_or_init(|| { let bytes = include_bytes!("../assets/framework/fonts/PretendardVariable.ttf"); let mut count = 0u32; unsafe { AddFontMemResourceEx(bytes.as_ptr() as _, bytes.len() as u32, std::ptr::null_mut(), &mut count) as usize } }) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    fn luminance(value: u32) -> f64 {
        let channel = |shift: u32| { let level = ((value>>shift)&255) as f64/255.0; if level <= 0.04045 { level/12.92 } else { ((level+0.055)/1.055).powf(2.4) } };
        0.2126*channel(0)+0.7152*channel(8)+0.0722*channel(16)
    }
    #[test]
    fn framework_text_and_functional_boundaries_pass_both_themes() {
        for dark in [false, true] {
            let colors = palette(dark);
            let contrast = |first: u32, second: u32| { let first = luminance(colors[(first-ROLE) as usize]); let second = luminance(colors[(second-ROLE) as usize]); (first.max(second)+0.05)/(first.min(second)+0.05) };
            for surface in [CANVAS, SURFACE, SUBTLE, HOVER, PRESSED] { for text in [TEXT, SECONDARY] { assert!(contrast(text, surface) >= 4.5, "{dark}: {text} / {surface}"); } }
            for (text, surface) in [(ON_ACCENT, ACCENT), (ON_ACCENT, ACCENT_HOVER), (SELECTION_TEXT, SELECTION), (LINK, SELECTION), (SECONDARY, SELECTION), (SUCCESS_TEXT, SUCCESS_SURFACE), (WARNING_TEXT, WARNING_SURFACE), (DANGER_TEXT, DANGER_SURFACE), (INFO_TEXT, INFO_SURFACE)] { assert!(contrast(text, surface) >= 4.5); }
            for surface in [CANVAS, SURFACE, SUBTLE] { assert!(contrast(CONTROL_BORDER, surface) >= 3.0); assert!(contrast(FOCUS, surface) >= 3.0); }
        }
    }
    #[test]
    fn preferences_round_trip_and_density_preserves_typography() {
        for theme in Theme::ALL { for density in [Density::Comfortable, Density::Compact] { let preferences = Preferences { theme, density }; assert_eq!(serde_json::from_str::<Preferences>(&serde_json::to_string(&preferences).unwrap()).unwrap(), preferences); set(preferences); assert!(control_height() >= 32); assert!(row_height() >= 40); } }
        set(Preferences::legacy()); assert_eq!(Preferences::default().theme, Theme::System);
        assert!(ensure_font());
    }
}
