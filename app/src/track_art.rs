use super::*;

// No circuit geometry is bundled in the public distribution.
#[cfg(test)]
pub(super) fn available(_name: &str) -> bool { false }

pub(super) unsafe fn draw(dc: HDC, _name: &str, area: RECT, _selected: bool) {
    text_format(dc, "Circuit layout unavailable", area, FONT_CAPTION, MUTED, false, DT_WORDBREAK | DT_CENTER);
}
