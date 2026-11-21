use super::*;

// Public builds use typography instead of redistributing vehicle photography.
impl State {
    pub(super) unsafe fn car_art(&self, dc: HDC, series: &crate::race_catalog::Series, area: RECT) {
        fill(dc, area, SIDE);
        text_format(dc, &format!("{}\nClass {}", series.category.label(), series.class.label()),
            rect(area.left+12, area.top+16, area.right-12, area.bottom-8),
            FONT_BODY, MUTED, true, DT_WORDBREAK | DT_CENTER);
    }
}
