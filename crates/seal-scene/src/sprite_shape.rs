//! A spline terrain shape: control points, pictures and collider settings.
//!
//! Points are stored as one string so a damaged list fails to load instead of
//! being skipped. Each point is `x y corner` or `x y smooth ox oy`, separated
//! by semicolons. `ox oy` is the outgoing tangent of a smooth point, in the
//! object's own units. A smooth point's incoming tangent is the opposite of
//! that, so the curve is continuous. A corner has no tangents.

use seal_ecs::{SealField, SealValue, SaveCtx, LoadCtx};

/// One control point of a sprite shape spline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplinePoint {
    pub pos: [f32; 2],
    /// When false the curve corners here and the tangent is ignored.
    pub smooth: bool,
    /// Outgoing tangent, as an offset from `pos`. Ignored unless `smooth`.
    pub out: [f32; 2],
}

/// The point list as stored on the component. An empty list is allowed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointList(pub Vec<SplinePoint>);

impl SealField for PointList {
    fn save(&self, _: &SaveCtx) -> SealValue {
        SealValue::String(points_text(&self.0))
    }

    fn load(value: &SealValue, _: &LoadCtx) -> Result<Self, String> {
        match value {
            SealValue::String(text) => parse_points(text).map(PointList),
            other => Err(format!("sprite shape point list is damaged: {other:?}")),
        }
    }
}

/// Write points the way [`parse_points`] reads them.
pub fn points_text(points: &[SplinePoint]) -> String {
    points
        .iter()
        .map(|p| {
            let (x, y) = (num(p.pos[0]), num(p.pos[1]));
            if p.smooth {
                format!("{x} {y} smooth {} {}", num(p.out[0]), num(p.out[1]))
            } else {
                format!("{x} {y} corner")
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Read a point list. Empty text is an empty list. Anything else that is not
/// a sequence of complete points is an error.
pub fn parse_points(text: &str) -> Result<Vec<SplinePoint>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for part in text.split(';') {
        let part = part.trim();
        if part.is_empty() {
            return Err("sprite shape point list is damaged".into());
        }
        out.push(parse_one(part)?);
    }
    Ok(out)
}

fn parse_one(part: &str) -> Result<SplinePoint, String> {
    let tokens: Vec<&str> = part.split_whitespace().collect();
    let damaged = || "sprite shape point list is damaged".to_string();
    let num = |token: &str| token.parse::<f32>().ok().filter(|n| n.is_finite()).ok_or_else(damaged);
    match tokens.as_slice() {
        [x, y, "corner"] => Ok(SplinePoint {
            pos: [num(x)?, num(y)?],
            smooth: false,
            out: [0.0, 0.0],
        }),
        [x, y, "smooth", ox, oy] => Ok(SplinePoint {
            pos: [num(x)?, num(y)?],
            smooth: true,
            out: [num(ox)?, num(oy)?],
        }),
        _ => Err(damaged()),
    }
}

fn num(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".into()
    } else {
        trimmed.into()
    }
}

/// How the edge strip joins at a control point.
pub const CORNERS: &[&str] = &["miter", "bevel", "round"];

/// Spline terrain: a fill clipped to the closed outline, and an edge picture
/// along the outline. Empty `on` means the object is not a sprite shape.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteShape {
    pub on: bool,
    pub points: PointList,
    pub closed: bool,
    /// Picture tiled inside a closed shape, relative to the project folder.
    pub fill: String,
    /// World size of one fill tile, across and up.
    pub fill_scale: [f32; 2],
    /// Edge pictures for segments whose direction falls in each angle range.
    pub edge_top: String,
    pub edge_side: String,
    pub edge_bottom: String,
    /// Edge thickness in the object's units.
    pub thickness: f32,
    /// `miter`, `bevel` or `round`.
    pub corner: String,
    /// Draw order: higher layers draw on top, then higher `order`.
    pub layer: i32,
    pub order: i32,
    pub tint: [f32; 4],
    /// Direction ranges in degrees. A range that runs backwards wraps past 180.
    pub top_angles: [f32; 2],
    pub side_angles: [f32; 2],
    pub bottom_angles: [f32; 2],
    /// Build a collider that follows the spline.
    pub collider: bool,
    /// How far the drawn and collided curve may stray from the true spline.
    pub tolerance: f32,
}

impl Default for SpriteShape {
    fn default() -> Self {
        Self {
            on: false,
            points: PointList::default(),
            closed: false,
            fill: String::new(),
            fill_scale: [1.0, 1.0],
            edge_top: String::new(),
            edge_side: String::new(),
            edge_bottom: String::new(),
            thickness: 0.2,
            corner: "miter".into(),
            layer: 0,
            order: 0,
            tint: [1.0; 4],
            top_angles: [-40.0, 40.0],
            side_angles: [40.0, 140.0],
            bottom_angles: [140.0, -140.0],
            collider: false,
            tolerance: 0.05,
        }
    }
}

seal_ecs::text_component!(SpriteShape as "sprite_shape" {
    on, points, closed, fill, fill_scale, edge_top, edge_side, edge_bottom, thickness, corner,
    layer, order, tint, top_angles, side_angles, bottom_angles, collider, tolerance
});
