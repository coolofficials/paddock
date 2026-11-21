use super::*;
use std::{ffi::c_void, sync::atomic::{AtomicU32, Ordering}};
use windows_sys::{core::{BSTR, GUID, HRESULT}, Win32::{System::Variant::{VARIANT, VT_I4}, UI::{Accessibility::*, Controls::{STATE_SYSTEM_FOCUSABLE, STATE_SYSTEM_UNAVAILABLE, STATE_SYSTEM_PRESSED}}}};

pub(super) const ACTION_MESSAGE: u32 = WM_APP+12;
pub(super) struct Apartment(bool);
impl Drop for Apartment { fn drop(&mut self) { if self.0 { unsafe { windows_sys::Win32::System::Com::CoUninitialize(); } } } }
pub(super) unsafe fn initialize() -> Apartment { Apartment(windows_sys::Win32::System::Com::CoInitializeEx(null(), windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32) >= 0) }
const ACCESSIBLE: GUID = GUID::from_u128(0x618736e0_3c3d_11cf_810c_00aa00389b71);
const UNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
const DISPATCH: GUID = GUID::from_u128(0x00020400_0000_0000_c000_000000000046);
type TextProperty = unsafe extern "system" fn(*mut Accessible, VARIANT, *mut BSTR) -> HRESULT;
type VariantProperty = unsafe extern "system" fn(*mut Accessible, VARIANT, *mut VARIANT) -> HRESULT;
#[repr(C)]
struct VTable {
    query: unsafe extern "system" fn(*mut Accessible, *const GUID, *mut *mut c_void) -> HRESULT,
    add: unsafe extern "system" fn(*mut Accessible) -> u32,
    release: unsafe extern "system" fn(*mut Accessible) -> u32,
    type_count: unsafe extern "system" fn(*mut Accessible, *mut u32) -> HRESULT,
    type_info: unsafe extern "system" fn(*mut Accessible, u32, u32, *mut *mut c_void) -> HRESULT,
    names: unsafe extern "system" fn(*mut Accessible, *const GUID, *const *const u16, u32, u32, *mut i32) -> HRESULT,
    invoke: unsafe extern "system" fn(*mut Accessible, i32, *const GUID, u32, u16, *mut c_void, *mut VARIANT, *mut c_void, *mut u32) -> HRESULT,
    parent: unsafe extern "system" fn(*mut Accessible, *mut *mut c_void) -> HRESULT,
    count: unsafe extern "system" fn(*mut Accessible, *mut i32) -> HRESULT,
    child: unsafe extern "system" fn(*mut Accessible, VARIANT, *mut *mut c_void) -> HRESULT,
    name: TextProperty, value: TextProperty, description: TextProperty, role: VariantProperty, state: VariantProperty, help: TextProperty,
    help_topic: unsafe extern "system" fn(*mut Accessible, *mut BSTR, VARIANT, *mut i32) -> HRESULT,
    shortcut: TextProperty,
    focus: unsafe extern "system" fn(*mut Accessible, *mut VARIANT) -> HRESULT,
    selection: unsafe extern "system" fn(*mut Accessible, *mut VARIANT) -> HRESULT,
    default_action: TextProperty,
    select: unsafe extern "system" fn(*mut Accessible, i32, VARIANT) -> HRESULT,
    location: unsafe extern "system" fn(*mut Accessible, *mut i32, *mut i32, *mut i32, *mut i32, VARIANT) -> HRESULT,
    navigate: unsafe extern "system" fn(*mut Accessible, i32, VARIANT, *mut VARIANT) -> HRESULT,
    hit_test: unsafe extern "system" fn(*mut Accessible, i32, i32, *mut VARIANT) -> HRESULT,
    do_action: unsafe extern "system" fn(*mut Accessible, VARIANT) -> HRESULT,
    put_name: unsafe extern "system" fn(*mut Accessible, VARIANT, BSTR) -> HRESULT,
    put_value: unsafe extern "system" fn(*mut Accessible, VARIANT, BSTR) -> HRESULT,
}
#[repr(C)]
struct Accessible { table: &'static VTable, references: AtomicU32, window: HWND, state_id: isize }
unsafe fn state<'a>(object: *mut Accessible) -> Option<&'a mut State> { let pointer = GetWindowLongPtrW((*object).window, GWLP_USERDATA); if pointer == 0 || pointer != (*object).state_id { None } else { (pointer as *mut State).as_mut() } }
unsafe fn identifier(child: &VARIANT) -> Option<i32> { (child.Anonymous.Anonymous.vt == VT_I4).then_some(child.Anonymous.Anonymous.Anonymous.lVal) }
unsafe fn number(output: *mut VARIANT, value: i32) -> HRESULT { if output.is_null() { return E_POINTER; } *output = VARIANT::default(); (*output).Anonymous.Anonymous.vt = VT_I4; (*output).Anonymous.Anonymous.Anonymous.lVal = value; S_OK }
unsafe fn string(output: *mut BSTR, value: &str) -> HRESULT { if output.is_null() { return E_POINTER; } *output = SysAllocString(wide(value).as_ptr()); if (*output).is_null() { E_OUTOFMEMORY } else { S_OK } }
unsafe extern "system" fn query(object: *mut Accessible, iid: *const GUID, output: *mut *mut c_void) -> HRESULT {
    if iid.is_null() || output.is_null() { return E_POINTER; } *output = null_mut();
    let matches = |expected: GUID| (*iid).data1 == expected.data1 && (*iid).data2 == expected.data2 && (*iid).data3 == expected.data3 && (*iid).data4 == expected.data4;
    if matches(ACCESSIBLE) || matches(UNKNOWN) || matches(DISPATCH) { *output = object as _; add(object); S_OK } else { E_NOINTERFACE }
}
unsafe extern "system" fn add(object: *mut Accessible) -> u32 { (*object).references.fetch_add(1, Ordering::Relaxed)+1 }
unsafe extern "system" fn release(object: *mut Accessible) -> u32 { let remaining = (*object).references.fetch_sub(1, Ordering::AcqRel)-1; if remaining == 0 { drop(Box::from_raw(object)); } remaining }
unsafe extern "system" fn type_count(_: *mut Accessible, output: *mut u32) -> HRESULT { if output.is_null() { return E_POINTER; } *output = 0; S_OK }
unsafe extern "system" fn type_info(_: *mut Accessible, _: u32, _: u32, output: *mut *mut c_void) -> HRESULT { if !output.is_null() { *output = null_mut(); } E_NOTIMPL }
unsafe extern "system" fn names(_: *mut Accessible, _: *const GUID, _: *const *const u16, _: u32, _: u32, _: *mut i32) -> HRESULT { E_NOTIMPL }
unsafe extern "system" fn invoke(_: *mut Accessible, _: i32, _: *const GUID, _: u32, _: u16, _: *mut c_void, _: *mut VARIANT, _: *mut c_void, _: *mut u32) -> HRESULT { E_NOTIMPL }
unsafe extern "system" fn parent(_: *mut Accessible, output: *mut *mut c_void) -> HRESULT { if output.is_null() { return E_POINTER; } *output = null_mut(); S_FALSE }
unsafe extern "system" fn child(object: *mut Accessible, _: VARIANT, output: *mut *mut c_void) -> HRESULT { parent(object, output) }
unsafe extern "system" fn count(object: *mut Accessible, output: *mut i32) -> HRESULT { if output.is_null() { return E_POINTER; } *output = state(object).map(|state| state.hits.len() as i32).unwrap_or(0); S_OK }
unsafe extern "system" fn name(object: *mut Accessible, child: VARIANT, output: *mut BSTR) -> HRESULT {
    let Some(state) = state(object) else { return E_FAIL; }; let Some(identifier) = identifier(&child) else { return E_INVALIDARG; };
    if identifier == 0 { string(output, PAGE_TITLES[state.page as usize]) } else if let Some(hit) = state.hits.get(identifier.saturating_sub(1) as usize) { string(output, &hit.label) } else { E_INVALIDARG }
}
unsafe extern "system" fn value(object: *mut Accessible, child: VARIANT, output: *mut BSTR) -> HRESULT {
    if identifier(&child) == Some(0) { if let Some(state) = state(object) { return string(output, &state.input.reading); } }
    string(output, "")
}
unsafe extern "system" fn empty(_: *mut Accessible, _: VARIANT, output: *mut BSTR) -> HRESULT { string(output, "") }
fn action_role(action: &Action) -> u32 {
    match action {
        Action::Page(_) => ROLE_SYSTEM_PAGETAB,
        Action::PlannerCategoryToggle(_) | Action::PlannerHidden | Action::PlannerHide(_) | Action::PlannerCollapse(_) | Action::PlannerClass(_) | Action::PlannerFavorites | Action::PlannerBeginner | Action::Select(_, _) | Action::ConditionalSeries | Action::Favorite(_) | Action::Enable(_) | Action::TrayMode(_) | Action::LauncherTray => ROLE_SYSTEM_CHECKBUTTON,
        Action::FavoritesOnly(_) | Action::PlannerMode(_) => ROLE_SYSTEM_RADIOBUTTON,
        Action::ProfileMenu | Action::AppDetails(_) | Action::PlannerFilters | Action::PlannerCategory | Action::ToolsMenu | Action::ScheduleTools | Action::SeriesJump | Action::WeekMenu | Action::CategoryMenu | Action::LicenseMenu | Action::ThemeMenu => ROLE_SYSTEM_BUTTONMENU,
        _ => ROLE_SYSTEM_PUSHBUTTON
    }
}
unsafe extern "system" fn role(object: *mut Accessible, child: VARIANT, output: *mut VARIANT) -> HRESULT {
    let role = identifier(&child).and_then(|index| index.checked_sub(1)).filter(|index| *index >= 0).and_then(|index| state(object)?.hits.get(index as usize)).map(|hit| action_role(&hit.action)).unwrap_or(ROLE_SYSTEM_CLIENT);
    number(output, role as i32)
}
unsafe extern "system" fn flags(object: *mut Accessible, child: VARIANT, output: *mut VARIANT) -> HRESULT {
    let Some(state) = state(object) else { return E_FAIL; }; let Some(identifier) = identifier(&child) else { return E_INVALIDARG; };
    let mut flags = STATE_SYSTEM_FOCUSABLE;
    if let Some(hit) = identifier.checked_sub(1).filter(|index| *index >= 0).and_then(|index| state.hits.get(index as usize)) {
        if state.selected(&hit.action) {
            flags |= match hit.action { Action::Page(_) => STATE_SYSTEM_SELECTED, _ => STATE_SYSTEM_CHECKED };
        }
        if !state.enabled(&hit.action) { flags |= STATE_SYSTEM_UNAVAILABLE; }
        if state.input.focused.as_ref() == Some(&hit.action) { flags |= STATE_SYSTEM_FOCUSED; }
        if state.input.pressed.as_ref() == Some(&hit.action) { flags |= STATE_SYSTEM_PRESSED; }
    }
    number(output, flags as i32)
}
unsafe extern "system" fn help_topic(_: *mut Accessible, output: *mut BSTR, _: VARIANT, topic: *mut i32) -> HRESULT { if !topic.is_null() { *topic = -1; } if !output.is_null() { *output = null_mut(); } S_FALSE }
unsafe extern "system" fn focus(object: *mut Accessible, output: *mut VARIANT) -> HRESULT { let index = state(object).and_then(|state| state.hits.iter().position(|hit| state.input.focused.as_ref() == Some(&hit.action))).map(|index| index+1).unwrap_or(0); number(output, index as i32) }
unsafe extern "system" fn selection(object: *mut Accessible, output: *mut VARIANT) -> HRESULT {
    if output.is_null() { return E_POINTER; }
    let Some(state) = state(object) else { return E_FAIL; };
    if let Some(index) = state.hits.iter().position(|hit| matches!(hit.action, Action::Page(_)) && state.selected(&hit.action)) { number(output, index as i32+1) }
    else { *output = VARIANT::default(); S_FALSE }
}
unsafe extern "system" fn default_action(_: *mut Accessible, child: VARIANT, output: *mut BSTR) -> HRESULT { string(output, if identifier(&child) == Some(0) { "" } else { "실행" }) }
unsafe extern "system" fn select(object: *mut Accessible, flags: i32, child: VARIANT) -> HRESULT {
    if flags as u32 & SELFLAG_TAKEFOCUS == 0 { return E_INVALIDARG; }
    let Some(state) = state(object) else { return E_FAIL; }; let index = identifier(&child).unwrap_or(0)-1;
    if index < 0 { SetFocus((*object).window); return S_OK; }
    let Some(hit) = state.hits.get(index as usize) else { return E_INVALIDARG; }; state.input.focused = Some(hit.action.clone()); state.input.focus_visible = true; state.input.viewport.reveal((*object).window, hit.rect); SetFocus((*object).window); State::redraw((*object).window); S_OK
}
unsafe extern "system" fn location(object: *mut Accessible, left: *mut i32, top: *mut i32, width: *mut i32, height: *mut i32, child: VARIANT) -> HRESULT {
    if [left, top, width, height].iter().any(|pointer| pointer.is_null()) { return E_POINTER; }
    let Some(state) = state(object) else { return E_FAIL; }; let index = identifier(&child).unwrap_or(-1);
    let mut area = RECT::default();
    if index == 0 { GetClientRect((*object).window, &mut area); } else if let Some(hit) = state.hits.get((index-1) as usize) { let dpi = design::effective_dpi(GetDpiForWindow((*object).window)); let viewport = &state.input.viewport; area = rect((hit.rect.left-viewport.x)*dpi/96, (hit.rect.top-viewport.y)*dpi/96, (hit.rect.right-viewport.x)*dpi/96, (hit.rect.bottom-viewport.y)*dpi/96); } else { return E_INVALIDARG; }
    let mut point = POINT { x: area.left, y: area.top }; ClientToScreen((*object).window, &mut point); *left = point.x; *top = point.y; *width = area.right-area.left; *height = area.bottom-area.top; S_OK
}
unsafe extern "system" fn navigate(object: *mut Accessible, direction: i32, child: VARIANT, output: *mut VARIANT) -> HRESULT {
    let Some(state) = state(object) else { return E_FAIL; }; let current = identifier(&child).unwrap_or(0);
    let next = match direction as u32 { NAVDIR_FIRSTCHILD => 1, NAVDIR_LASTCHILD => state.hits.len() as i32, NAVDIR_NEXT | NAVDIR_DOWN => current+1, NAVDIR_PREVIOUS | NAVDIR_UP => current-1, _ => 0 };
    if next < 1 || next > state.hits.len() as i32 { if !output.is_null() { *output = VARIANT::default(); } return S_FALSE; } number(output, next)
}
unsafe extern "system" fn hit_test(object: *mut Accessible, horizontal: i32, vertical: i32, output: *mut VARIANT) -> HRESULT {
    let Some(state) = state(object) else { return E_FAIL; }; let mut point = POINT { x: horizontal, y: vertical }; ScreenToClient((*object).window, &mut point); let dpi = design::effective_dpi(GetDpiForWindow((*object).window));
    let action = state.hit_at((point.x*96/dpi+state.input.viewport.x, point.y*96/dpi+state.input.viewport.y)); let index = state.hits.iter().position(|hit| Some(&hit.action) == action.as_ref()).map(|index| index+1).unwrap_or(0); number(output, index as i32)
}
unsafe extern "system" fn do_action(object: *mut Accessible, child: VARIANT) -> HRESULT {
    let Some(state) = state(object) else { return E_FAIL; }; let index = identifier(&child).unwrap_or(0)-1;
    let Some(action) = state.hits.get(index as usize).filter(|hit| state.enabled(&hit.action)).map(|hit| hit.action.clone()) else { return E_INVALIDARG; };
    SendMessageW((*object).window, ACTION_MESSAGE, 0, &action as *const Action as isize); S_OK
}
unsafe extern "system" fn put(_: *mut Accessible, _: VARIANT, _: BSTR) -> HRESULT { E_ACCESSDENIED }
static TABLE: VTable = VTable { query, add, release, type_count, type_info, names, invoke, parent, count, child, name, value, description: value, role, state: flags, help: empty, help_topic, shortcut: empty, focus, selection, default_action, select, location, navigate, hit_test, do_action, put_name: put, put_value: put };
pub(super) unsafe fn get_object(window: HWND, parameter: WPARAM) -> LRESULT {
    let object = Box::into_raw(Box::new(Accessible { table: &TABLE, references: AtomicU32::new(1), window, state_id: GetWindowLongPtrW(window, GWLP_USERDATA) }));
    let result = LresultFromObject(&ACCESSIBLE, parameter, object as _); release(object); result
}
pub(super) unsafe fn announce(window: HWND, state: &State) {
    if let Some(index) = state.hits.iter().position(|hit| state.input.focused.as_ref() == Some(&hit.action)) { NotifyWinEvent(EVENT_OBJECT_FOCUS, window, OBJID_CLIENT, index as i32+1); }
}
pub(super) unsafe fn verify(window: HWND, expected: usize) -> bool {
    let result = SendMessageW(window, WM_GETOBJECT, 0, OBJID_CLIENT as isize);
    let mut pointer: *mut c_void = null_mut();
    if ObjectFromLresult(result, &ACCESSIBLE, 0, &mut pointer) != S_OK || pointer.is_null() { return false; }
    let object = pointer as *mut Accessible; let table = &*(*(pointer as *const *const VTable));
    let mut count = 0; let mut child = VARIANT::default(); number(&mut child, 1);
    let mut label: BSTR = null(); let mut selected = VARIANT::default(); let mut description: BSTR = null(); let mut root = VARIANT::default(); number(&mut root, 0);
    let valid = (table.count)(object, &mut count) == S_OK && count as usize == expected && (table.name)(object, child, &mut label) == S_OK && SysStringLen(label) > 0 && (table.selection)(object, &mut selected) == S_OK && identifier(&selected).map(|value| value > 0).unwrap_or(false) && (table.value)(object, root, &mut description) == S_OK && SysStringLen(description) > 0;
    SysFreeString(label); SysFreeString(description); (table.release)(object); valid
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_race_selections_are_checkboxes_not_exclusive_radio_buttons() {
        assert_eq!(action_role(&Action::Select(5, 2)), ROLE_SYSTEM_CHECKBUTTON);
        assert_eq!(action_role(&Action::ConditionalSeries), ROLE_SYSTEM_CHECKBUTTON);
        assert_eq!(action_role(&Action::WeekMenu), ROLE_SYSTEM_BUTTONMENU);
    }
}

