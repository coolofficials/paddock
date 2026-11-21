use super::*;
use windows_sys::Win32::UI::Controls::SetScrollInfo;

#[derive(Default)]
pub(super) struct Viewport { pub x: i32, pub y: i32, width: i32, height: i32 }
impl Viewport {
    pub fn width(&self) -> i32 { self.width.max(160) }
    pub fn height(&self) -> i32 { self.height.max(120) }
    pub unsafe fn update(&mut self, window: HWND) {
        let mut bounds = RECT::default(); GetClientRect(window, &mut bounds);
        let dpi = design::effective_dpi(GetDpiForWindow(window));
        self.width = bounds.right*96/dpi; self.height = bounds.bottom*96/dpi;
        self.x = self.x.clamp(0, 0); self.y = self.y.clamp(0, 0);
        for (bar, extent, available, position) in [(SB_HORZ, self.width(), self.width, self.x), (SB_VERT, self.height(), self.height, self.y)] {
            let info = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_RANGE | SIF_PAGE | SIF_POS, nMin: 0, nMax: extent-1, nPage: available.max(1) as u32, nPos: position, nTrackPos: 0 };
            SetScrollInfo(window, bar, &info, 1);
        }
    }
    pub unsafe fn scroll(&mut self, window: HWND, vertical: bool, request: u32) {
        let bar = if vertical { SB_VERT } else { SB_HORZ };
        let mut info = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_ALL, ..SCROLLINFO::default() }; GetScrollInfo(window, bar, &mut info);
        let next = match request as i32 { SB_LINEUP => info.nPos-32, SB_LINEDOWN => info.nPos+32, SB_PAGEUP => info.nPos-info.nPage as i32, SB_PAGEDOWN => info.nPos+info.nPage as i32, SB_THUMBPOSITION | SB_THUMBTRACK => info.nTrackPos, SB_TOP => 0, SB_BOTTOM => info.nMax, _ => info.nPos };
        if vertical { self.y = next; } else { self.x = next; } self.update(window);
    }
    pub unsafe fn reveal(&mut self, window: HWND, area: RECT) {
        if area.left < self.x { self.x = area.left; } else if area.right > self.x+self.width { self.x = area.right-self.width; }
        if area.top < self.y { self.y = area.top; } else if area.bottom > self.y+self.height { self.y = area.bottom-self.height; }
        self.update(window);
    }
}
