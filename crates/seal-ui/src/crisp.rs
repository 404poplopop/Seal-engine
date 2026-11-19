//! Painting for the editor's crisp square look: raised and sunken surfaces, LED slots
//! and bars, and the small inline reset button. Glows stay faint on purpose.

use egui::epaint::Shadow;
use egui::{Color32, Painter, Rect, Sense, Shape, Stroke, StrokeKind, Visuals};

/// Where crisp shapes go: straight to a painter, or into a list that is placed
/// behind a widget after it has been laid out.
pub trait Sink {
    fn put(&mut self, shape: Shape);
}

impl Sink for Vec<Shape> {
    fn put(&mut self, shape: Shape) {
        self.push(shape);
    }
}

impl Sink for &Painter {
    fn put(&mut self, shape: Shape) {
        self.add(shape);
    }
}

/// Paint `shapes` behind whatever was added to `painter` after `slot` was reserved.
pub fn place_behind(painter: &Painter, slot: egui::layers::ShapeIdx, shapes: Vec<Shape>) {
    painter.set(slot, Shape::Vec(shapes));
}

/// Corner radius for every control in this look.
pub const RADIUS: u8 = 0;
/// Width of an LED bar on the left edge of a field.
pub const LED_BAR_PX: f32 = 3.0;
/// Size of the inline reset button inside a field.
pub const RESET_PX: f32 = 16.0;

pub(crate) fn mix(base: Color32, tint: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let ch = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    Color32::from_rgba_unmultiplied(
        ch(base.r(), tint.r()),
        ch(base.g(), tint.g()),
        ch(base.b(), tint.b()),
        ch(base.a(), tint.a()),
    )
}

fn alpha(color: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

fn is_dark(v: &Visuals) -> bool {
    v.dark_mode
}

/// Vertical two-stop gradient over a rect.
pub fn vgrad(p: &mut impl Sink, r: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), top);
    mesh.colored_vertex(r.right_top(), top);
    mesh.colored_vertex(r.right_bottom(), bottom);
    mesh.colored_vertex(r.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.put(Shape::mesh(mesh));
}

/// Soft halo around a rect.
pub fn glow(p: &mut impl Sink, r: Rect, color: Color32, blur: u8, spread: u8) {
    p.put(Shape::Rect(Shadow { offset: [0, 0], blur, spread, color }.as_shape(r, RADIUS)));
}

fn hline(p: &mut impl Sink, x0: f32, x1: f32, y: f32, color: Color32) {
    p.put(Shape::line_segment([egui::pos2(x0, y), egui::pos2(x1, y)], Stroke::new(1.0, color)));
}

/// A recessed well: text and number fields, checkboxes, LED sockets.
pub fn sunken(p: &mut impl Sink, r: Rect, v: &Visuals, focused: bool, hovered: bool) {
    let accent = v.selection.stroke.color;
    let well = v.extreme_bg_color;
    let dark = is_dark(v);
    if focused {
        glow(p, r, alpha(accent, 70), 6, 1);
    }
    // Light lip under the well sells the depth.
    hline(
        p,
        r.left() + 1.0,
        r.right() - 1.0,
        r.bottom() + 0.5,
        Color32::from_white_alpha(if dark { 16 } else { 90 }),
    );
    let (top, bottom) = if focused {
        (mix(accent, Color32::BLACK, 0.22), mix(accent, Color32::BLACK, 0.38))
    } else if hovered {
        (mix(well, Color32::WHITE, 0.03), mix(well, Color32::WHITE, 0.06))
    } else {
        (mix(well, Color32::BLACK, if dark { 0.18 } else { 0.06 }), well)
    };
    vgrad(p, r, top, bottom);
    // Inner shadow along the top and left edges.
    let ink = Color32::from_black_alpha(if dark { 150 } else { 50 });
    hline(p, r.left(), r.right(), r.top() + 1.5, ink);
    hline(
        p,
        r.left(),
        r.right(),
        r.top() + 2.5,
        Color32::from_black_alpha(if dark { 50 } else { 18 }),
    );
    p.put(Shape::line_segment(
        [egui::pos2(r.left() + 1.5, r.top()), egui::pos2(r.left() + 1.5, r.bottom())],
        Stroke::new(1.0, Color32::from_black_alpha(if dark { 60 } else { 20 })),
    ));
    let edge = if focused {
        mix(accent, Color32::WHITE, 0.2)
    } else {
        Color32::from_black_alpha(if dark { 190 } else { 70 })
    };
    p.put(Shape::rect_stroke(r, RADIUS, Stroke::new(1.0, edge), StrokeKind::Inside));
    if focused {
        hline(p, r.left() + 1.0, r.right() - 1.0, r.top() + 1.5, Color32::from_white_alpha(28));
    }
}

/// A darker crisp backing behind text inside a focused field.
pub fn backing(p: &mut impl Sink, r: Rect, v: &Visuals) {
    let accent = v.selection.stroke.color;
    vgrad(p, r, mix(accent, Color32::BLACK, 0.52), mix(accent, Color32::BLACK, 0.46));
    hline(p, r.left(), r.right(), r.top() + 0.5, Color32::from_black_alpha(90));
    hline(p, r.left(), r.right(), r.bottom() + 0.5, alpha(mix(accent, Color32::WHITE, 0.5), 45));
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Raised {
    Normal,
    Hover,
    Pressed,
    Active,
}

/// A raised surface: buttons, chips and section headers.
pub fn raised(p: &mut impl Sink, r: Rect, v: &Visuals, state: Raised) {
    let r = snap(r);
    let accent = v.selection.stroke.color;
    let base = v.widgets.inactive.bg_fill;
    let dark = is_dark(v);
    match state {
        Raised::Active => {
            vgrad(p, r, mix(accent, Color32::WHITE, 0.06), mix(accent, Color32::BLACK, 0.18));
            p.put(Shape::rect_stroke(
                r,
                RADIUS,
                Stroke::new(1.0, mix(accent, Color32::BLACK, 0.65)),
                StrokeKind::Inside,
            ));
        }
        Raised::Pressed => {
            vgrad(p, r, mix(base, Color32::BLACK, 0.2), mix(base, Color32::BLACK, 0.1));
            hline(p, r.left(), r.right(), r.top() + 1.5, Color32::from_black_alpha(120));
            p.put(Shape::rect_stroke(
                r,
                RADIUS,
                Stroke::new(1.0, Color32::from_black_alpha(180)),
                StrokeKind::Inside,
            ));
        }
        Raised::Normal | Raised::Hover => {
            let lift = if state == Raised::Hover { 0.1 } else { 0.0 };
            // Crisp drop edge under the bottom; no blur, so the corners stay square.
            hline(
                p,
                r.left(),
                r.right(),
                r.bottom() + 0.5,
                Color32::from_black_alpha(if dark { 120 } else { 45 }),
            );
            vgrad(
                p,
                r,
                mix(base, Color32::WHITE, 0.10 + lift),
                mix(base, Color32::BLACK, 0.06 - lift * 0.5),
            );
            hline(
                p,
                r.left() + 1.0,
                r.right() - 1.0,
                r.top() + 1.5,
                Color32::from_white_alpha(if dark { 22 } else { 120 }),
            );
            p.put(Shape::rect_stroke(
                r,
                RADIUS,
                Stroke::new(1.0, Color32::from_black_alpha(if dark { 160 } else { 60 })),
                StrokeKind::Inside,
            ));
        }
    }
}

/// A recessed LED slot, lit or dark. `r` is the slot itself (a few px tall).
pub fn led_slot(p: &mut impl Sink, r: Rect, lit: bool, color: Color32) {
    let r = snap(r);
    p.put(Shape::rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(8, 10, 14, 235)));
    hline(p, r.left(), r.right(), r.bottom() + 0.5, Color32::from_white_alpha(40));
    // Inner lamp: one pixel in from the cutout so the dark rim keeps square corners.
    let inner = Rect::from_min_max(r.min + egui::vec2(1.0, 1.0), r.max - egui::vec2(1.0, 1.0));
    if lit {
        let led = mix(color, Color32::WHITE, 0.35);
        // Faint bloom only along the long edges, never round the corners.
        let bloom = Rect::from_min_max(
            egui::pos2(inner.left() + 2.0, r.top() - 1.0),
            egui::pos2(inner.right() - 2.0, r.bottom() + 1.0),
        );
        p.put(Shape::rect_filled(bloom, 0.0, alpha(led, 34)));
        p.put(Shape::rect_filled(
            r.shrink2(egui::vec2(0.0, 0.0)),
            0.0,
            Color32::from_rgba_unmultiplied(8, 10, 14, 235),
        ));
        p.put(Shape::rect_filled(inner.expand2(egui::vec2(0.0, 0.5)), 0.0, led));
        p.put(Shape::rect_filled(inner, 0.0, Color32::from_rgb(235, 246, 255)));
    } else {
        p.put(Shape::rect_filled(
            inner.expand2(egui::vec2(0.0, 0.5)),
            0.0,
            Color32::from_rgb(30, 34, 42),
        ));
        hline(p, inner.left(), inner.right(), r.top() + 1.5, Color32::from_black_alpha(160));
    }
}

/// Snap a rect to whole pixels so 1 px edges and corners stay crisp.
pub fn snap(r: Rect) -> Rect {
    Rect::from_min_max(
        egui::pos2(r.left().round(), r.top().round()),
        egui::pos2(r.right().round(), r.bottom().round()),
    )
}

/// An LED bar on a field's left edge with a faint halo. `glow_amount` 0..1.
/// Unlit bar for values with no axis colour or level.
pub const NEUTRAL_LED: Color32 = Color32::from_rgb(92, 100, 112);

pub fn led_bar(p: &mut impl Sink, field: Rect, color: Color32, glow_amount: f32) {
    let bar = Rect::from_min_max(
        egui::pos2(field.left() + 1.0, field.top() + 1.0),
        egui::pos2(field.left() + 1.0 + LED_BAR_PX, field.bottom() - 1.0),
    );
    let a = (glow_amount.clamp(0.0, 1.0) * 120.0) as u8;
    if a > 0 {
        glow(p, bar, alpha(color, a), 4, 0);
    }
    p.put(Shape::rect_filled(bar, 0.0, color));
    hline(p, bar.left(), bar.right(), bar.top() + 0.5, mix(color, Color32::WHITE, 0.4));
    hline(p, bar.left(), bar.right(), bar.bottom() - 0.5, mix(color, Color32::BLACK, 0.4));
    // Dark seam between the LED and the text.
    p.put(Shape::line_segment(
        [egui::pos2(bar.right() + 0.5, bar.top()), egui::pos2(bar.right() + 0.5, bar.bottom())],
        Stroke::new(1.0, Color32::from_black_alpha(170)),
    ));
}

/// LED color and glow for a light level in lux: dark at 0, grey when dim, warm when bright.
pub fn lux_led(lux: f32) -> (Color32, f32) {
    let t = ((1.0 + lux.max(0.0)).log10() / 2.5).clamp(0.0, 1.0);
    let dark = Color32::from_rgb(28, 30, 34);
    let grey = Color32::from_rgb(118, 120, 126);
    let warm = Color32::from_rgb(255, 226, 160);
    if t < 0.5 {
        (mix(dark, grey, t / 0.5), 0.0)
    } else {
        let k = (t - 0.5) / 0.5;
        (mix(grey, warm, k), k)
    }
}

const AMBER: Color32 = Color32::from_rgb(255, 184, 64);

/// Small amber reset button placed at `rect`. Returns true when clicked.
pub fn reset_button(ui: &mut egui::Ui, rect: Rect, id: egui::Id, tooltip: &str) -> bool {
    let response = ui.interact(rect, id, Sense::click());
    let response = crate::a11y::named_control(ui, response, egui::WidgetType::Button, tooltip)
        .on_hover_text(tooltip);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let painter = ui.painter().clone();
    let mut p = &painter;
    p.put(Shape::Rect(
        Shadow { offset: [0, 1], blur: 2, spread: 0, color: Color32::from_black_alpha(110) }
            .as_shape(rect, RADIUS),
    ));
    let (top, bottom) = if response.is_pointer_button_down_on() {
        (Color32::from_rgb(40, 36, 28), Color32::from_rgb(48, 43, 34))
    } else if response.hovered() {
        (Color32::from_rgb(74, 66, 50), Color32::from_rgb(56, 50, 38))
    } else {
        (Color32::from_rgb(62, 56, 44), Color32::from_rgb(48, 43, 34))
    };
    vgrad(&mut p, rect, top, bottom);
    hline(
        &mut p,
        rect.left() + 1.0,
        rect.right() - 1.0,
        rect.top() + 1.5,
        Color32::from_rgba_unmultiplied(255, 220, 150, 38),
    );
    p.put(Shape::rect_stroke(
        rect,
        RADIUS,
        Stroke::new(1.0, Color32::from_black_alpha(170)),
        StrokeKind::Inside,
    ));
    glow(&mut p, rect.shrink(4.0), alpha(AMBER, 40), 4, 0);
    reset_arrow(&painter, rect.center(), 4.0, AMBER);
    response.clicked()
}

/// Checkbox in the sunken style. Returns the box rect for callers that add marks.
pub fn checkbox(p: &mut impl Sink, r: Rect, v: &Visuals, on: bool, mixed: bool, hovered: bool) {
    sunken(p, r, v, on, hovered);
    let accent = v.selection.stroke.color;
    let mark = mix(accent, Color32::WHITE, 0.85);
    if on {
        let pts = [
            egui::pos2(r.left() + r.width() * 0.25, r.top() + r.height() * 0.52),
            egui::pos2(r.left() + r.width() * 0.43, r.top() + r.height() * 0.72),
            egui::pos2(r.left() + r.width() * 0.77, r.top() + r.height() * 0.30),
        ];
        glow(p, r.shrink(r.width() * 0.3), alpha(mix(accent, Color32::WHITE, 0.4), 60), 4, 0);
        p.put(egui::Shape::line(pts.to_vec(), Stroke::new(1.8, mark)));
    } else if mixed {
        let y = r.center().y;
        p.put(Shape::line_segment(
            [
                egui::pos2(r.left() + r.width() * 0.28, y),
                egui::pos2(r.right() - r.width() * 0.28, y),
            ],
            Stroke::new(1.8, mix(accent, Color32::WHITE, 0.5)),
        ));
    }
}

/// Section header arrow, crisp and bright.
pub fn arrow(p: &mut impl Sink, center: egui::Pos2, open: bool, color: Color32) {
    let s = 4.5;
    let pts = if open {
        vec![
            egui::pos2(center.x - s, center.y - s * 0.55),
            egui::pos2(center.x + s, center.y - s * 0.55),
            egui::pos2(center.x, center.y + s * 0.65),
        ]
    } else {
        vec![
            egui::pos2(center.x - s * 0.55, center.y - s),
            egui::pos2(center.x + s * 0.65, center.y),
            egui::pos2(center.x - s * 0.55, center.y + s),
        ]
    };
    p.put(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// A single-line text edit drawn as a sunken well, matching the number fields.
pub fn well_edit(ui: &mut egui::Ui, edit: egui::TextEdit<'_>) -> egui::Response {
    well_edit_margin(ui, edit, egui::Margin::symmetric(6, 3))
}

/// [`well_edit`] with a custom text margin (for example room for an LED bar).
pub fn well_edit_margin(
    ui: &mut egui::Ui,
    edit: egui::TextEdit<'_>,
    margin: egui::Margin,
) -> egui::Response {
    let slot = ui.painter().add(Shape::Noop);
    let response = ui.add(
        edit.frame(egui::Frame::NONE.inner_margin(margin)).vertical_align(egui::Align::Center),
    );
    let visuals = ui.visuals().clone();
    let mut shapes: Vec<Shape> = Vec::new();
    sunken(&mut shapes, response.rect, &visuals, response.has_focus(), response.hovered());
    place_behind(ui.painter(), slot, shapes);
    response
}

/// A clean circular "reset" arrow: an open ring with a solid head, turning clockwise.
pub fn reset_arrow(painter: &Painter, c: egui::Pos2, radius: f32, color: Color32) {
    let at = |deg: f32| {
        let a = deg.to_radians();
        egui::pos2(c.x + radius * a.cos(), c.y - radius * a.sin())
    };
    let points: Vec<egui::Pos2> =
        (0..=24).map(|i| at(300.0 - (i as f32) * (270.0 / 24.0))).collect();
    painter.add(Shape::line(points, Stroke::new(1.5, color)));
    // Head at the 30 degree end, pointing on round the circle (clockwise on screen).
    let end = at(30.0);
    let a = 30f32.to_radians();
    let dir = egui::vec2(a.sin(), a.cos());
    let side = egui::vec2(-dir.y, dir.x);
    painter.add(Shape::convex_polygon(
        vec![end + dir * 2.6, end + side * 2.4, end - side * 2.4],
        color,
        Stroke::NONE,
    ));
}

/// Dropdowns in the crisp style: a raised button like the chips, square corners.
/// The open list keeps the normal visuals so its rows still highlight on hover.
pub trait CrispCombo {
    fn show_crisp<R>(
        self,
        ui: &mut egui::Ui,
        menu: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<Option<R>>;
}

impl CrispCombo for egui::ComboBox {
    fn show_crisp<R>(
        self,
        ui: &mut egui::Ui,
        menu: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<Option<R>> {
        let saved = ui.visuals().clone();
        let slot = ui.painter().add(Shape::Noop);
        let inner = ui
            .scope(|ui| {
                let w = &mut ui.visuals_mut().widgets;
                for s in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
                    s.weak_bg_fill = Color32::TRANSPARENT;
                    s.bg_fill = Color32::TRANSPARENT;
                    s.bg_stroke = Stroke::NONE;
                    s.corner_radius = egui::CornerRadius::ZERO;
                    s.expansion = 0.0;
                }
                let menu_visuals = saved.clone();
                self.show_ui(ui, move |ui| {
                    *ui.visuals_mut() = menu_visuals;
                    menu(ui)
                })
            })
            .inner;
        let r = &inner.response;
        let state = if r.is_pointer_button_down_on() {
            Raised::Pressed
        } else if r.hovered() {
            Raised::Hover
        } else {
            Raised::Normal
        };
        let mut shapes: Vec<Shape> = Vec::new();
        raised(&mut shapes, r.rect, &saved, state);
        place_behind(ui.painter(), slot, shapes);
        inner
    }
}

/// [`well_edit`] placed at an exact rect (rename boxes and similar overlays).
pub fn put_well(ui: &mut egui::Ui, rect: Rect, edit: egui::TextEdit<'_>) -> egui::Response {
    let slot = ui.painter().add(Shape::Noop);
    let response = ui.put(
        rect,
        edit.frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 2)))
            .vertical_align(egui::Align::Center),
    );
    let visuals = ui.visuals().clone();
    let mut shapes: Vec<Shape> = Vec::new();
    sunken(&mut shapes, response.rect, &visuals, response.has_focus(), response.hovered());
    place_behind(ui.painter(), slot, shapes);
    response
}

/// A text value field in a property row: sunken well with an unlit LED bar, text
/// starting where number and vector values start. `right` leaves room for a reset.
pub fn value_well(ui: &mut egui::Ui, edit: egui::TextEdit<'_>, right: i8) -> egui::Response {
    let left = (1.0 + LED_BAR_PX + 6.0) as i8;
    let response = well_edit_margin(ui, edit, egui::Margin { left, right, top: 3, bottom: 3 });
    led_bar(&mut ui.painter(), response.rect, NEUTRAL_LED, 0.0);
    response
}

/// Raised state for a control from its response. `on` lights it as active.
pub fn state_of(response: &egui::Response, on: bool) -> Raised {
    if response.is_pointer_button_down_on() {
        Raised::Pressed
    } else if on {
        Raised::Active
    } else if response.hovered() {
        Raised::Hover
    } else {
        Raised::Normal
    }
}

/// A square plate under an icon control. With `always` false it stays flat until
/// hovered, pressed or `on`, so panel toolbars stay quiet.
pub fn plate(ui: &egui::Ui, rect: Rect, response: &egui::Response, on: bool, always: bool) {
    let state = state_of(response, on);
    if !always && state == Raised::Normal {
        return;
    }
    raised(&mut ui.painter(), rect, ui.visuals(), state);
}

/// A recessed tray that holds a group of raised buttons (for example the play controls).
pub fn tray(p: &mut impl Sink, r: Rect, v: &Visuals) {
    let r = snap(r);
    let dark = is_dark(v);
    p.put(Shape::rect_filled(r, 0.0, v.extreme_bg_color));
    hline(
        p,
        r.left(),
        r.right(),
        r.top() + 0.5,
        Color32::from_black_alpha(if dark { 150 } else { 50 }),
    );
    hline(
        p,
        r.left(),
        r.right(),
        r.bottom() - 0.5,
        Color32::from_white_alpha(if dark { 18 } else { 110 }),
    );
    p.put(Shape::rect_stroke(
        r,
        RADIUS,
        Stroke::new(1.0, Color32::from_black_alpha(if dark { 140 } else { 45 })),
        StrokeKind::Inside,
    ));
}

/// A text button drawn as a crisp raised chip. With `always` false it is flat until
/// hovered, pressed or `open` (menu bar titles).
pub fn button(
    ui: &mut egui::Ui,
    button: egui::Button<'_>,
    always: bool,
    open: bool,
) -> egui::Response {
    let slot = ui.painter().add(Shape::Noop);
    let response = add_frameless(ui, button);
    paint_behind_button(ui, slot, &response, always, open);
    response
}

/// Add a button with its usual padding but without egui's own frame, so a crisp
/// surface can be painted behind it.
pub fn add_frameless(ui: &mut egui::Ui, button: egui::Button<'_>) -> egui::Response {
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        for state in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            state.bg_fill = Color32::TRANSPARENT;
            state.weak_bg_fill = Color32::TRANSPARENT;
            state.bg_stroke = Stroke::NONE;
            state.expansion = 0.0;
        }
        ui.add(button)
    })
    .inner
}

/// A crisp raised button that opens a menu below it; it stays pressed while open.
pub fn menu_button<R>(
    ui: &mut egui::Ui,
    button: egui::Button<'_>,
    always: bool,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::Response {
    let slot = ui.painter().add(Shape::Noop);
    let response = add_frameless(ui, button);
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    paint_behind_button(ui, slot, &response, always, open);
    egui::Popup::menu(&response).show(body);
    response
}

/// Paint the raised chip for a frameless button into its reserved slot.
pub fn paint_behind_button(
    ui: &egui::Ui,
    slot: egui::layers::ShapeIdx,
    response: &egui::Response,
    always: bool,
    open: bool,
) {
    let state = if open && !response.is_pointer_button_down_on() {
        Raised::Pressed
    } else {
        state_of(response, false)
    };
    if always || state != Raised::Normal {
        let mut shapes = Vec::new();
        raised(&mut shapes, response.rect, ui.visuals(), state);
        place_behind(ui.painter(), slot, shapes);
    }
}

/// The main action of a dialog: a raised chip lit in the accent colour.
pub fn primary_button(ui: &mut egui::Ui, button: egui::Button<'_>) -> egui::Response {
    let slot = ui.painter().add(Shape::Noop);
    let response = ui
        .scope(|ui| {
            ui.visuals_mut().override_text_color = Some(Color32::WHITE);
            add_frameless(ui, button)
        })
        .inner;
    let state = if response.is_pointer_button_down_on() { Raised::Pressed } else { Raised::Active };
    let mut shapes = Vec::new();
    raised(&mut shapes, response.rect, ui.visuals(), state);
    place_behind(ui.painter(), slot, shapes);
    response
}

/// A square menu row: hover fill with an accent bar on the left, an optional
/// check mark in a fixed gutter, and the label. Matches the main menu rows.
pub fn menu_row(ui: &mut egui::Ui, checked: Option<bool>, label: &str) -> egui::Response {
    let height = crate::MENU_ROW_PX;
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::TextStyle::Body.resolve(ui.style()),
        ui.visuals().text_color(),
    );
    let width = (galley.size().x + 40.0).max(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    response.widget_info(|| match checked {
        Some(on) => egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, label),
        None => egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label),
    });
    let v = ui.visuals();
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, v.widgets.hovered.bg_fill);
        let bar = Rect::from_min_size(rect.left_top(), egui::vec2(2.0, rect.height()));
        ui.painter().rect_filled(bar, 0.0, v.selection.stroke.color);
    }
    let ink = v.text_color();
    if checked == Some(true) {
        let c = egui::pos2(rect.left() + 12.0, rect.center().y);
        let stroke = Stroke::new(1.6, ink);
        ui.painter().line_segment([c + egui::vec2(-4.0, 0.0), c + egui::vec2(-1.0, 3.0)], stroke);
        ui.painter().line_segment([c + egui::vec2(-1.0, 3.0), c + egui::vec2(4.5, -4.0)], stroke);
    }
    ui.painter().galley(
        egui::pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    response
}
