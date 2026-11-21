use std::{collections::BTreeMap, sync::OnceLock};
use windows_sys::Win32::{Foundation::POINT, Graphics::Gdi::*};

pub unsafe fn draw(dc: HDC, name: &str, left: i32, top: i32, size: i32, role: u32) {
    static PATHS: OnceLock<BTreeMap<String, Vec<Vec<[f64; 2]>>>> = OnceLock::new();
    let paths = PATHS.get_or_init(|| serde_json::from_str(include_str!("../assets/framework/lucide-paths.json")).unwrap());
    let Some(strokes) = paths.get(name) else { return; };
    let brush = LOGBRUSH { lbStyle: BS_SOLID, lbColor: crate::design::resolve(role), lbHatch: 0 };
    let pen = ExtCreatePen((PS_GEOMETRIC | PS_SOLID | PS_ENDCAP_ROUND | PS_JOIN_ROUND) as u32, (size as f64*1.75/24.0).round().max(1.0) as u32, &brush, 0, std::ptr::null());
    let previous = SelectObject(dc, pen);
    for stroke in strokes {
        let points: Vec<_> = stroke.iter().map(|point| POINT { x: left+(point[0]*size as f64/24.0).round() as i32, y: top+(point[1]*size as f64/24.0).round() as i32 }).collect();
        Polyline(dc, points.as_ptr(), points.len() as i32);
    }
    SelectObject(dc, previous); DeleteObject(pen);
}
