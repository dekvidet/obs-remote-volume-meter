use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
const FLOOR: f32 = -60.0;
const LABELS: &[i32] = &[-60, -50, -40, -30, -20, -15, -10, -5, 0];

#[derive(Clone)]
pub struct Meter {
    target: f32,
    pub level: f32,
    input_peak: f32,
    pub peak: f32,
    hold: f32,
    clipped: bool,
}
impl Default for Meter {
    fn default() -> Self {
        Self {
            target: FLOOR,
            level: FLOOR,
            input_peak: FLOOR,
            peak: FLOOR,
            hold: 0.0,
            clipped: false,
        }
    }
}
impl Meter {
    pub fn set(&mut self, magnitude: f32, peak: f32) {
        self.target = db(magnitude);
        self.input_peak = db(peak);
        self.clipped |= self.input_peak >= -0.1;
    }
    pub fn update(&mut self, dt: f32) {
        if self.target >= self.level {
            self.level = self.target
        } else {
            self.level = (self.level - 32.0 * dt).max(self.target)
        }
        if self.input_peak >= self.peak {
            self.peak = self.input_peak;
            self.hold = 1.0
        } else if self.hold > 0.0 {
            self.hold -= dt
        } else {
            self.peak = (self.peak - 18.0 * dt).max(FLOOR)
        }
    }
    pub fn moving(&self) -> bool {
        self.level > FLOOR || self.peak > FLOOR
    }
}

pub fn draw(
    ui: &mut egui::Ui,
    meter: &Meter,
    channel: usize,
    count: usize,
    vertical: bool,
    large: bool,
    vertical_height: Option<f32>,
) {
    if vertical {
        draw_vertical(ui, meter, channel, count, large, vertical_height);
    } else {
        draw_horizontal(ui, meter, channel, count, large);
    }
}

pub fn vertical_group_width(channel_count: usize, large: bool, spacing: f32) -> f32 {
    if channel_count == 0 {
        return 0.0;
    }
    let bar_width = if large { 54.0 } else { 30.0 };
    let ordinary_scale_width = 4.0;
    let final_scale_width = if large { 4.0 } else { 36.0 };
    bar_width * channel_count as f32
        + ordinary_scale_width * channel_count.saturating_sub(1) as f32
        + final_scale_width
        + spacing * channel_count.saturating_sub(1) as f32
}

fn draw_horizontal(ui: &mut egui::Ui, meter: &Meter, channel: usize, count: usize, large: bool) {
    let scale = !large && channel + 1 == count;
    let bar_height = if large { 34.0 } else { 8.0 };
    let height = bar_height + if scale { 18.0 } else { 4.0 };
    let (response, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), height), Sense::hover());
    let r = response.rect;
    let label_width = if large { 0.0 } else { 24.0 };
    if !large {
        painter.text(
            Pos2::new(r.left(), r.top() + bar_height / 2.0),
            Align2::LEFT_CENTER,
            channel_label(channel, count),
            FontId::monospace(10.0),
            Color32::from_gray(150),
        );
    }
    let bar = Rect::from_min_max(
        Pos2::new(r.left() + label_width, r.top()),
        Pos2::new(r.right(), r.top() + bar_height),
    );
    paint_horizontal_bar(&painter, bar, meter);
    if scale {
        paint_scale(&painter, bar);
    }
}

fn draw_vertical(
    ui: &mut egui::Ui,
    meter: &Meter,
    channel: usize,
    count: usize,
    large: bool,
    height: Option<f32>,
) {
    let scale = !large && channel + 1 == count;
    let bar_width = if large { 54.0 } else { 30.0 };
    let scale_width = if scale { 36.0 } else { 4.0 };
    let width = bar_width + scale_width;
    let height = height.unwrap_or_else(|| ui.available_height()).max(160.0);
    let (response, painter) = ui.allocate_painter(Vec2::new(width, height), Sense::hover());
    let r = response.rect;
    let label_height = if large { 0.0 } else { 18.0 };
    let bar = Rect::from_min_max(
        r.min,
        Pos2::new(r.left() + bar_width, r.bottom() - label_height),
    );
    paint_vertical_bar(&painter, bar, meter);
    if scale {
        paint_vertical_scale(&painter, bar);
    }
    if !large {
        painter.text(
            Pos2::new(r.center().x, r.bottom()),
            Align2::CENTER_BOTTOM,
            channel_label(channel, count),
            FontId::monospace(10.0),
            Color32::from_gray(150),
        );
    }
}

fn channel_label(channel: usize, count: usize) -> &'static str {
    if count == 1 {
        "M"
    } else if channel == 0 {
        "L"
    } else if channel == 1 {
        "R"
    } else {
        "•"
    }
}

fn paint_horizontal_bar(p: &egui::Painter, r: Rect, m: &Meter) {
    p.rect_filled(r, 1.0, Color32::from_rgb(14, 16, 16));
    let active = x(r, m.level);
    let mut sx = r.left() + 1.0;
    while sx < r.right() - 1.0 {
        let d = from_x(r, sx + 1.5);
        let sr = Rect::from_min_max(
            Pos2::new(sx, r.top() + 1.0),
            Pos2::new((sx + 3.0).min(r.right() - 1.0), r.bottom() - 1.0),
        );
        p.rect_filled(sr, 0.0, color(d, sx <= active));
        sx += 4.0;
    }
    if m.peak > FLOOR {
        let px = x(r, m.peak);
        p.line_segment(
            [Pos2::new(px, r.top()), Pos2::new(px, r.bottom())],
            Stroke::new(
                2.0,
                if m.clipped {
                    Color32::WHITE
                } else {
                    color(m.peak, true)
                },
            ),
        );
    }
    p.rect_stroke(
        r,
        1.0,
        Stroke::new(1.0, Color32::from_rgb(7, 8, 9)),
        StrokeKind::Inside,
    );
}

fn paint_vertical_bar(p: &egui::Painter, r: Rect, m: &Meter) {
    p.rect_filled(r, 1.0, Color32::from_rgb(14, 16, 16));
    let active = y(r, m.level);
    let mut sy = r.bottom() - 1.0;
    while sy > r.top() + 1.0 {
        let d = from_y(r, sy - 1.5);
        let sr = Rect::from_min_max(
            Pos2::new(r.left() + 1.0, (sy - 3.0).max(r.top() + 1.0)),
            Pos2::new(r.right() - 1.0, sy),
        );
        p.rect_filled(sr, 0.0, color(d, sy >= active));
        sy -= 4.0;
    }
    if m.peak > FLOOR {
        let py = y(r, m.peak);
        p.line_segment(
            [Pos2::new(r.left(), py), Pos2::new(r.right(), py)],
            Stroke::new(
                2.0,
                if m.clipped {
                    Color32::WHITE
                } else {
                    color(m.peak, true)
                },
            ),
        );
    }
    p.rect_stroke(
        r,
        1.0,
        Stroke::new(1.0, Color32::from_rgb(7, 8, 9)),
        StrokeKind::Inside,
    );
}
fn paint_scale(p: &egui::Painter, r: Rect) {
    for n in -30..=0 {
        let d = n as f32 * 2.0;
        let px = x(r, d);
        let major = LABELS.contains(&(d as i32));
        let h = if major { 5.0 } else { 3.0 };
        p.line_segment(
            [
                Pos2::new(px, r.bottom() + 2.0),
                Pos2::new(px, r.bottom() + 2.0 + h),
            ],
            Stroke::new(1.0, Color32::from_gray(if major { 150 } else { 80 })),
        );
        if major {
            p.text(
                Pos2::new(px, r.bottom() + 8.0),
                Align2::CENTER_TOP,
                format!("{d:.0}"),
                FontId::monospace(8.0),
                Color32::from_gray(140),
            );
        }
    }
}

fn paint_vertical_scale(p: &egui::Painter, r: Rect) {
    for n in -30..=0 {
        let d = n as f32 * 2.0;
        let py = y(r, d);
        let major = LABELS.contains(&(d as i32));
        let width = if major { 5.0 } else { 3.0 };
        p.line_segment(
            [
                Pos2::new(r.right() + 2.0, py),
                Pos2::new(r.right() + 2.0 + width, py),
            ],
            Stroke::new(1.0, Color32::from_gray(if major { 150 } else { 80 })),
        );
        if major {
            p.text(
                Pos2::new(r.right() + 9.0, py),
                Align2::LEFT_CENTER,
                format!("{d:.0}"),
                FontId::monospace(8.0),
                Color32::from_gray(140),
            );
        }
    }
}
fn color(d: f32, on: bool) -> Color32 {
    match (d, on) {
        (d, true) if d >= -9.0 => Color32::from_rgb(236, 64, 56),
        (d, true) if d >= -20.0 => Color32::from_rgb(239, 199, 54),
        (_, true) => Color32::from_rgb(74, 203, 74),
        (d, false) if d >= -9.0 => Color32::from_rgb(79, 29, 27),
        (d, false) if d >= -20.0 => Color32::from_rgb(81, 69, 29),
        (_, false) => Color32::from_rgb(27, 69, 31),
    }
}
fn db(v: f32) -> f32 {
    if v <= 0.0 || !v.is_finite() {
        FLOOR
    } else {
        (20.0 * v.log10()).clamp(FLOOR, 0.0)
    }
}
fn x(r: Rect, d: f32) -> f32 {
    r.left() + ((d.clamp(FLOOR, 0.0) - FLOOR) / -FLOOR) * r.width()
}
fn from_x(r: Rect, x: f32) -> f32 {
    FLOOR + ((x - r.left()) / r.width()).clamp(0.0, 1.0) * -FLOOR
}
fn y(r: Rect, d: f32) -> f32 {
    r.bottom() - ((d.clamp(FLOOR, 0.0) - FLOOR) / -FLOOR) * r.height()
}
fn from_y(r: Rect, y: f32) -> f32 {
    FLOOR + ((r.bottom() - y) / r.height()).clamp(0.0, 1.0) * -FLOOR
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn converts_db() {
        assert_eq!(db(1.0), 0.0);
        assert!((db(0.1) + 20.0).abs() < 0.001);
        assert_eq!(db(0.0), FLOOR)
    }
    #[test]
    fn decays() {
        let mut m = Meter::default();
        m.set(0.5, 0.5);
        m.update(0.02);
        let loud = m.level;
        m.set(0.0, 0.0);
        m.update(0.1);
        assert!(m.level < loud && m.level > FLOOR)
    }
}
