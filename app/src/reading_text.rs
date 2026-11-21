use super::*;
use windows_sys::Win32::UI::Controls::EM_SETSEL;

#[repr(C)]
#[derive(Default)]
struct ParagraphFormat {
    size: u32, mask: u32, numbering: u16, effects: u16,
    start_indent: i32, right_indent: i32, offset: i32, alignment: u16, tab_count: i16,
    tabs: [i32; 32], before: i32, after: i32, line_spacing: i32, style: i16,
    line_rule: u8, outline: u8, shading_weight: u16, shading_style: u16,
    numbering_start: u16, numbering_style: u16, numbering_tab: u16,
    border_space: u16, border_width: u16, borders: u16,
}

#[repr(C)]
#[derive(Default)]
struct CharacterFormat {
    size: u32, mask: u32, effects: u32, height: i32, offset: i32, color: u32,
    charset: u8, pitch: u8, face: [u16; 32],
}

pub(super) unsafe fn control_class() -> &'static str {
    use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
    static LIBRARY: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    if *LIBRARY.get_or_init(|| LoadLibraryExW(wide("Msftedit.dll").as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) as usize) != 0 { "RICHEDIT50W" } else { "EDIT" }
}

fn line_spacing(window: HWND) -> i32 {
    let native_dpi = unsafe { GetDpiForWindow(window) }.max(96);
    let scale = design::effective_dpi(native_dpi) as f64/native_dpi as f64;
    let leading = design::tokens()["typography"]["lineHeight"]["reading"].as_str().unwrap().parse::<f64>().unwrap();
    (16.0*leading*15.0*scale).round() as i32
}

pub(super) unsafe fn format(window: HWND) {
    let paragraph = ParagraphFormat { size: std::mem::size_of::<ParagraphFormat>() as u32, mask: 0x100, line_rule: 4, line_spacing: line_spacing(window), ..Default::default() };
    let character = CharacterFormat { size: std::mem::size_of::<CharacterFormat>() as u32, mask: 0x40000000, color: design::resolve(TEXT), ..Default::default() };
    SendMessageW(window, WM_USER+67, 0, design::resolve(CARD) as isize);
    SendMessageW(window, EM_SETSEL, 0, -1);
    SendMessageW(window, WM_USER+68, 4, &character as *const _ as isize);
    SendMessageW(window, WM_USER+71, 0, &paragraph as *const _ as isize);
    SendMessageW(window, EM_SETSEL, 0, 0);
}

pub(super) unsafe fn verified(window: HWND) -> bool {
    let mut paragraph = ParagraphFormat { size: std::mem::size_of::<ParagraphFormat>() as u32, ..Default::default() };
    SendMessageW(window, WM_USER+61, 0, &mut paragraph as *mut _ as isize);
    paragraph.mask & 0x100 != 0 && paragraph.line_rule == 4 && paragraph.line_spacing == line_spacing(window)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rich_edit_formats_match_win32_layouts() {
        assert_eq!(std::mem::size_of::<ParagraphFormat>(), 188);
        assert_eq!(std::mem::size_of::<CharacterFormat>(), 92);
    }
}
