//! 2D materials: how a sprite's pixels are drawn.
//!
//! A material is either a ready-made effect (grayscale, outline, flash,
//! dissolve, wave, colour ramp) or a canvas shader file. A canvas shader is a
//! short list of `name = formula` lines run once for every pixel; it reads the
//! pixel's place, the time and the picture, and sets `r`, `g`, `b` and `a`.
//!
//! ```text
//! # Shimmer that scrolls across the picture.
//! glow = 0.5 + 0.5 * sin(u * 12 - time * 4)
//! r = mix(r, cr, glow * amount)
//! g = mix(g, cg, glow * amount)
//! b = mix(b, cb, glow * amount)
//! ```
//!
//! Formulas use numbers, `+ - * / %`, comparisons (1 when true, 0 when not),
//! brackets, the inputs `u v x y w h time amount speed cr cg cb ca r g b a`,
//! names set on earlier lines, and the functions `sin cos tan abs floor fract
//! sqrt pow min max clamp mix step smoothstep noise luma tex_r tex_g tex_b tex_a`.

use crate::{format_num, parse_tint, tint_hex, Entity};

pub use seal_scene::Material2d;

pub const EFFECTS: &[&str] = &[
    "none",
    "grayscale",
    "outline",
    "flash",
    "dissolve",
    "wave",
    "ramp",
    "particle_anim",
    "custom",
];
/// How the sprite combines with the picture behind it.
pub const BLENDS: &[&str] = &["alpha", "additive", "multiply", "premultiplied", "screen"];
/// How the sprite uses 2D lights.
pub const LIGHT_MODES: &[&str] = &["lit", "unlit", "light"];
/// Inspector fields, in order.
pub const KEYS: &[&str] =
    &["material2d", "mat_blend", "mat_light", "mat_color", "mat_amount", "mat_speed", "mat_shader"];
/// Extra inspector rows for the `particle_anim` effect only.
pub const SHEET_KEYS: &[&str] = &["mat_sheet_cols", "mat_sheet_rows", "mat_sheet_loop"];

pub fn material_field_keys(effect: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = KEYS.to_vec();
    if effect == "particle_anim" {
        out.extend(SHEET_KEYS);
    }
    out
}
/// File ending of canvas shader files.
pub const SHADER_EXT: &str = "shade2d";

fn effect_name(m: &Material2d) -> String {
    if m.effect.is_empty() {
        "none".into()
    } else {
        m.effect.clone()
    }
}

fn blend_name(m: &Material2d) -> String {
    match m.blend.as_str() {
        "" | "alpha" => "alpha".into(),
        other => other.into(),
    }
}

fn light_name(m: &Material2d) -> String {
    match m.light_mode.as_str() {
        "" | "lit" => "lit".into(),
        "unshaded" => "unlit".into(),
        "light-only" | "light_only" => "light".into(),
        other => other.into(),
    }
}

/// The material's rows. Blend and light mode stay visible with no effect.
pub fn fields(e: &Entity) -> Vec<(String, String)> {
    let m = &e.material2d;
    let mut out = vec![
        ("material2d".to_string(), effect_name(m)),
        ("mat_blend".into(), blend_name(m)),
        ("mat_light".into(), light_name(m)),
    ];
    if m.effect.is_empty() {
        return out;
    }
    if matches!(m.effect.as_str(), "outline" | "flash" | "dissolve" | "ramp" | "custom") {
        out.push(("mat_color".into(), tint_hex(&m.color)));
    }
    out.push(("mat_amount".into(), format_num(m.amount)));
    if matches!(m.effect.as_str(), "flash" | "dissolve" | "wave" | "custom") {
        out.push(("mat_speed".into(), format_num(m.speed)));
    }
    if m.effect == "custom" || m.effect == "particle_anim" {
        out.push(("mat_shader".into(), m.shader.clone()));
    }
    if m.effect == "particle_anim" {
        out.push(("mat_sheet_cols".into(), m.sheet_cols.max(1).to_string()));
        out.push(("mat_sheet_rows".into(), m.sheet_rows.max(1).to_string()));
        out.push((
            "mat_sheet_loop".into(),
            if m.sheet_loop { "1" } else { "0" }.into(),
        ));
        out.push(("mat_speed".into(), format_num(m.speed)));
    }
    out
}

pub fn field_text(e: &Entity, key: &str) -> Option<String> {
    let m = &e.material2d;
    Some(match key {
        "material2d" => effect_name(m),
        "mat_color" => tint_hex(&m.color),
        "mat_amount" => format_num(m.amount),
        "mat_speed" => format_num(m.speed),
        "mat_shader" => m.shader.clone(),
        "mat_blend" => blend_name(m),
        "mat_light" => light_name(m),
        "mat_sheet_cols" => m.sheet_cols.max(1).to_string(),
        "mat_sheet_rows" => m.sheet_rows.max(1).to_string(),
        "mat_sheet_loop" => if m.sheet_loop { "1" } else { "0" }.into(),
        _ => return None,
    })
}

/// Why a field's text is refused, or `None` when it is fine.
pub fn check(key: &str, value: &str) -> Option<&'static str> {
    let v = value.trim();
    let num =
        |lo: f32, hi: f32| v.parse::<f32>().is_ok_and(|n| n.is_finite() && (lo..=hi).contains(&n));
    let ok = match key {
        "material2d" => EFFECTS.contains(&v),
        "mat_color" => parse_tint(v).is_some(),
        "mat_amount" => num(0.0, 1.0),
        "mat_speed" => num(0.0, 100.0),
        "mat_shader" => v.is_empty() || v.to_ascii_lowercase().ends_with(&format!(".{SHADER_EXT}")),
        "mat_blend" => BLENDS.contains(&v),
        "mat_light" => {
            matches!(v, "lit" | "unlit" | "unshaded" | "light" | "light-only" | "light_only")
        }
        "mat_sheet_cols" | "mat_sheet_rows" => v.parse::<u32>().is_ok_and(|n| (1..=64).contains(&n)),
        "mat_sheet_loop" => matches!(v, "0" | "1" | "true" | "false" | "yes" | "no"),
        _ => false,
    };
    if ok {
        return None;
    }
    Some(match key {
        "mat_color" => "inspector-shape-color-invalid",
        "mat_shader" => "inspector-mat-shader-invalid",
        _ => "inspector-value-out-of-range",
    })
}

pub fn apply(e: &mut Entity, key: &str, value: &str) -> bool {
    let known = KEYS.contains(&key) || SHEET_KEYS.contains(&key);
    if !known || check(key, value).is_some() {
        return false;
    }
    let v = value.trim();
    let m = &mut e.material2d;
    match key {
        "material2d" => m.effect = if v == "none" { String::new() } else { v.into() },
        "mat_color" => m.color = parse_tint(v).unwrap_or(m.color),
        "mat_amount" => m.amount = v.parse().unwrap_or(m.amount),
        "mat_speed" => m.speed = v.parse().unwrap_or(m.speed),
        "mat_shader" => m.shader = v.replace('\\', "/"),
        "mat_sheet_cols" => m.sheet_cols = v.parse().unwrap_or(m.sheet_cols).max(1),
        "mat_sheet_rows" => m.sheet_rows = v.parse().unwrap_or(m.sheet_rows).max(1),
        "mat_sheet_loop" => m.sheet_loop = matches!(v, "1" | "true" | "yes" | "on"),
        "mat_blend" => m.blend = v.into(),
        "mat_light" => {
            m.light_mode = match v {
                "unshaded" => "unlit".into(),
                "light-only" | "light_only" => "light".into(),
                other => other.into(),
            }
        }
        _ => return false,
    }
    true
}

/// Whether the drawn pixels change with time, so the picture must be redrawn each frame.
pub fn animated(m: &Material2d) -> bool {
    match m.effect.as_str() {
        "flash" | "dissolve" | "wave" => m.speed > 0.0,
        "particle_anim" | "custom" => true,
        _ => false,
    }
}

fn particle_anim_frame(cols: u32, rows: u32, life_u: f32, r#loop: bool, time: f32, speed: f32) -> u32 {
    let total = cols.max(1) * rows.max(1);
    if total <= 1 {
        return 0;
    }
    let u = if speed > 0.0 {
        (time * speed).fract()
    } else {
        life_u.clamp(0.0, 1.0)
    };
    let idx = (u * total as f32).floor() as u32;
    if r#loop {
        idx.min(total - 1)
    } else {
        idx.min(total - 1)
    }
}

pub fn particle_anim_uv(m: &Material2d, life_u: f32, time: f32) -> (u32, [f32; 4]) {
    let cols = m.sheet_cols.max(1);
    let rows = m.sheet_rows.max(1);
    let frame = particle_anim_frame(cols, rows, life_u, m.sheet_loop, time, m.speed);
    let col = frame % cols;
    let row = frame / cols;
    let du = 1.0 / cols as f32;
    let dv = 1.0 / rows as f32;
    (frame, [col as f32 * du, row as f32 * dv, (col + 1) as f32 * du, (row + 1) as f32 * dv])
}

// ---------------------------------------------------------------- canvas shaders

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Num(f32),
    Var(usize),
    Neg(Box<Node>),
    Bin(char, Box<Node>, Box<Node>),
    Cmp(&'static str, Box<Node>, Box<Node>),
    Call(&'static str, Vec<Node>),
}

/// A canvas shader read from its text: pixel formulas, plus an optional
/// vertex section, light section, blend and light mode.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    names: Vec<String>,
    /// Pixel formulas. Older shaders are only this list.
    lines: Vec<(usize, Node)>,
    vertex: Vec<(usize, Node)>,
    light: Vec<(usize, Node)>,
    blend: Option<String>,
    light_mode: Option<String>,
}

/// Names every shader can read; the first eight are set fresh for each pixel.
const INPUTS: &[&str] = &[
    "r", "g", "b", "a", "u", "v", "x", "y", "w", "h", "time", "amount", "speed", "cr", "cg", "cb",
    "ca",
];
/// Extra names a light section can read. They stay 0 in a pixel formula.
const LIGHT_INPUTS: &[&str] =
    &["light_r", "light_g", "light_b", "light_amount", "nx", "ny", "nz", "n_dot"];

const FUNCS: &[(&str, usize)] = &[
    ("sin", 1),
    ("cos", 1),
    ("tan", 1),
    ("abs", 1),
    ("floor", 1),
    ("fract", 1),
    ("sqrt", 1),
    ("pow", 2),
    ("min", 2),
    ("max", 2),
    ("clamp", 3),
    ("mix", 3),
    ("step", 2),
    ("smoothstep", 3),
    ("noise", 2),
    ("luma", 0),
    ("tex_r", 2),
    ("tex_g", 2),
    ("tex_b", 2),
    ("tex_a", 2),
];

/// Why a shader line could not be read: its 1-based line number and the reason.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ShaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

struct Parser<'a> {
    chars: Vec<char>,
    at: usize,
    names: &'a mut Vec<String>,
}

impl Parser<'_> {
    fn skip(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }
    fn peek(&mut self) -> Option<char> {
        self.skip();
        self.chars.get(self.at).copied()
    }
    fn eat(&mut self, s: &str) -> bool {
        self.skip();
        let want: Vec<char> = s.chars().collect();
        if self.chars[self.at..].starts_with(&want) {
            self.at += want.len();
            true
        } else {
            false
        }
    }
    fn word(&mut self) -> Option<String> {
        self.skip();
        let start = self.at;
        while self.chars.get(self.at).is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_') {
            self.at += 1;
        }
        (self.at > start && !self.chars[start].is_ascii_digit())
            .then(|| self.chars[start..self.at].iter().collect())
            .or_else(|| {
                self.at = start;
                None
            })
    }
    fn compare(&mut self) -> Result<Node, String> {
        let left = self.sum()?;
        for op in ["<=", ">=", "==", "!=", "<", ">"] {
            if self.eat(op) {
                let right = self.sum()?;
                return Ok(Node::Cmp(op, Box::new(left), Box::new(right)));
            }
        }
        Ok(left)
    }
    fn sum(&mut self) -> Result<Node, String> {
        let mut left = self.product()?;
        loop {
            match self.peek() {
                Some(c @ ('+' | '-')) => {
                    self.at += 1;
                    left = Node::Bin(c, Box::new(left), Box::new(self.product()?));
                }
                _ => return Ok(left),
            }
        }
    }
    fn product(&mut self) -> Result<Node, String> {
        let mut left = self.unary()?;
        loop {
            match self.peek() {
                Some(c @ ('*' | '/' | '%')) => {
                    self.at += 1;
                    left = Node::Bin(c, Box::new(left), Box::new(self.unary()?));
                }
                _ => return Ok(left),
            }
        }
    }
    fn unary(&mut self) -> Result<Node, String> {
        if self.peek() == Some('-') {
            self.at += 1;
            return Ok(Node::Neg(Box::new(self.unary()?)));
        }
        self.atom()
    }
    fn atom(&mut self) -> Result<Node, String> {
        match self.peek() {
            Some('(') => {
                self.at += 1;
                let inner = self.compare()?;
                if !self.eat(")") {
                    return Err("a bracket is not closed".into());
                }
                Ok(inner)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => {
                let start = self.at;
                while self.chars.get(self.at).is_some_and(|c| c.is_ascii_digit() || *c == '.') {
                    self.at += 1;
                }
                let text: String = self.chars[start..self.at].iter().collect();
                text.parse().map(Node::Num).map_err(|_| format!("\"{text}\" is not a number"))
            }
            Some(_) => {
                let Some(name) = self.word() else {
                    return Err(format!("unexpected \"{}\"", self.chars[self.at]));
                };
                if self.eat("(") {
                    let Some(&(func, arity)) = FUNCS.iter().find(|(f, _)| *f == name) else {
                        return Err(format!("there is no function \"{name}\""));
                    };
                    let mut args = Vec::new();
                    if !self.eat(")") {
                        loop {
                            args.push(self.compare()?);
                            if self.eat(")") {
                                break;
                            }
                            if !self.eat(",") {
                                return Err(format!("\"{name}\" needs a comma or closing bracket"));
                            }
                        }
                    }
                    if args.len() != arity {
                        return Err(format!("\"{name}\" takes {arity} values, not {}", args.len()));
                    }
                    return Ok(Node::Call(func, args));
                }
                match self.names.iter().position(|n| *n == name) {
                    Some(i) => Ok(Node::Var(i)),
                    None => Err(format!("\"{name}\" is not set yet")),
                }
            }
            None => Err("a value is missing".into()),
        }
    }
}

fn blend_word(text: &str) -> Option<&'static str> {
    match text {
        "alpha" => Some("alpha"),
        "additive" => Some("additive"),
        "multiply" => Some("multiply"),
        "premultiplied" => Some("premultiplied"),
        "screen" => Some("screen"),
        _ => None,
    }
}

fn light_word(text: &str) -> Option<&'static str> {
    match text {
        "lit" => Some("lit"),
        "unlit" | "unshaded" => Some("unlit"),
        "light" | "light-only" | "light_only" => Some("light"),
        _ => None,
    }
}

impl Program {
    /// Read a canvas shader. Blank lines and text after `#` or `//` are ignored.
    /// A line that is only `vertex`, `light` or `pixel` switches section.
    /// `blend = alpha` and `light_mode = lit` set the material hints.
    pub fn parse(text: &str) -> Result<Program, ShaderError> {
        let mut names: Vec<String> = INPUTS.iter().map(|s| s.to_string()).collect();
        names.extend(LIGHT_INPUTS.iter().map(|s| s.to_string()));
        let mut lines = Vec::new();
        let mut vertex = Vec::new();
        let mut light = Vec::new();
        let mut blend = None;
        let mut light_mode = None;
        let mut section: &'static str = "pixel";
        for (index, raw) in text.lines().enumerate() {
            let line_no = index + 1;
            let fail = |message: String| ShaderError { line: line_no, message };
            let code = raw.split('#').next().unwrap_or("");
            let code = code.split("//").next().unwrap_or("").trim();
            if code.is_empty() {
                continue;
            }
            if let Some(next) = match code {
                "vertex" => Some("vertex"),
                "light" => Some("light"),
                "pixel" => Some("pixel"),
                _ => None,
            } {
                section = next;
                continue;
            }
            let Some((target, formula)) = code.split_once('=') else {
                return Err(fail("each line needs a name, \"=\" and a formula".into()));
            };
            let target = target.trim();
            let formula = formula.trim();
            if target.is_empty()
                || target.chars().next().is_some_and(|c| c.is_ascii_digit())
                || !target.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(fail(format!("\"{target}\" cannot be a name")));
            }
            if formula.starts_with('=') {
                return Err(fail("a line cannot start with a comparison".into()));
            }
            if target == "blend" {
                if let Some(mode) = blend_word(formula) {
                    blend = Some(mode.to_string());
                    continue;
                }
            }
            if target == "light_mode" {
                if let Some(mode) = light_word(formula) {
                    light_mode = Some(mode.to_string());
                    continue;
                }
            }
            let vertex_out = section == "vertex" && matches!(target, "x" | "y" | "u" | "v");
            let locked =
                (INPUTS[4..].contains(&target) || LIGHT_INPUTS.contains(&target)) && !vertex_out;
            if FUNCS.iter().any(|(f, _)| *f == target) || locked {
                return Err(fail(format!("\"{target}\" is an input and cannot be changed")));
            }
            let mut parser = Parser { chars: formula.chars().collect(), at: 0, names: &mut names };
            let node = parser.compare().map_err(fail)?;
            if parser.peek().is_some() {
                let rest: String = parser.chars[parser.at..].iter().collect();
                return Err(fail(format!("unexpected \"{}\"", rest.trim())));
            }
            let slot = match names.iter().position(|n| n == target) {
                Some(i) => i,
                None => {
                    names.push(target.to_string());
                    names.len() - 1
                }
            };
            let list = match section {
                "vertex" => &mut vertex,
                "light" => &mut light,
                _ => &mut lines,
            };
            list.push((slot, node));
        }
        Ok(Program { names, lines, vertex, light, blend, light_mode })
    }

    pub fn has_vertex(&self) -> bool {
        !self.vertex.is_empty()
    }

    pub fn has_light(&self) -> bool {
        !self.light.is_empty()
    }

    pub fn blend_mode(&self) -> Option<&str> {
        self.blend.as_deref()
    }

    pub fn light_mode(&self) -> Option<&str> {
        self.light_mode.as_deref()
    }

    fn vars_at(&self, set: &[(&str, f32)]) -> Vec<f32> {
        let mut vars = vec![0.0f32; self.names.len()];
        for (name, value) in set {
            if let Some(i) = self.names.iter().position(|n| n == name) {
                vars[i] = *value;
            }
        }
        vars
    }

    fn run(&self, lines: &[(usize, Node)], vars: &mut [f32]) {
        let pic = Picture { pixels: &[], width: 0, height: 0 };
        for (slot, node) in lines {
            let value = eval(node, vars, &pic);
            vars[*slot] = if value.is_finite() { value } else { 0.0 };
        }
    }

    /// Where a corner moves. With no vertex section the point stays put.
    pub fn vertex_at(
        &self,
        x: f32,
        y: f32,
        u: f32,
        v: f32,
        time: f32,
        amount: f32,
        speed: f32,
    ) -> [f32; 2] {
        if self.vertex.is_empty() {
            return [x, y];
        }
        let mut vars = self.vars_at(&[
            ("x", x),
            ("y", y),
            ("u", u),
            ("v", v),
            ("time", time),
            ("amount", amount),
            ("speed", speed),
        ]);
        self.run(&self.vertex, &mut vars);
        let slot = |name: &str, fallback: f32| {
            self.names.iter().position(|n| n == name).map(|i| vars[i]).unwrap_or(fallback)
        };
        [slot("x", x), slot("y", y)]
    }

    /// The colour a light section writes. With no light section this is the
    /// surface colour times the light colour.
    pub fn light_at_surface(
        &self,
        surface: [f32; 3],
        light: [f32; 3],
        amount: f32,
        normal: [f32; 3],
        n_dot: f32,
    ) -> [f32; 3] {
        if self.light.is_empty() {
            return [surface[0] * light[0], surface[1] * light[1], surface[2] * light[2]];
        }
        let mut vars = self.vars_at(&[
            ("r", surface[0]),
            ("g", surface[1]),
            ("b", surface[2]),
            ("a", 1.0),
            ("light_r", light[0]),
            ("light_g", light[1]),
            ("light_b", light[2]),
            ("light_amount", amount),
            ("nx", normal[0]),
            ("ny", normal[1]),
            ("nz", normal[2]),
            ("n_dot", n_dot),
        ]);
        self.run(&self.light, &mut vars);
        let slot = |name: &str, fallback: f32| {
            self.names.iter().position(|n| n == name).map(|i| vars[i]).unwrap_or(fallback)
        };
        [slot("r", surface[0]), slot("g", surface[1]), slot("b", surface[2])]
    }
}

/// Combine a sprite colour with the light already worked out.
/// `unlit` and `unshaded` ignore the light. `light` keeps only the light.
pub fn apply_light_mode(albedo: [f32; 3], incoming: [f32; 3], mode: &str) -> [f32; 3] {
    match mode {
        "unlit" | "unshaded" => albedo,
        "light" | "light-only" | "light_only" => incoming,
        _ => [albedo[0] * incoming[0], albedo[1] * incoming[1], albedo[2] * incoming[2]],
    }
}

/// Composite `src` over `dst`. `mode` is `alpha`, `additive`, `multiply` or `premultiplied`.
pub fn blend_pixel(dst: [f32; 4], src: [f32; 4], mode: &str) -> [f32; 4] {
    let clamp = |v: f32| v.clamp(0.0, 1.0);
    match mode {
        "additive" => [
            clamp(dst[0] + src[0] * src[3]),
            clamp(dst[1] + src[1] * src[3]),
            clamp(dst[2] + src[2] * src[3]),
            clamp(dst[3] + src[3]),
        ],
        "multiply" => {
            [clamp(dst[0] * src[0]), clamp(dst[1] * src[1]), clamp(dst[2] * src[2]), dst[3]]
        }
        "premultiplied" => [
            clamp(src[0] + dst[0] * (1.0 - src[3])),
            clamp(src[1] + dst[1] * (1.0 - src[3])),
            clamp(src[2] + dst[2] * (1.0 - src[3])),
            clamp(src[3] + dst[3] * (1.0 - src[3])),
        ],
        "screen" => [
            clamp(1.0 - (1.0 - dst[0]) * (1.0 - src[0] * src[3])),
            clamp(1.0 - (1.0 - dst[1]) * (1.0 - src[1] * src[3])),
            clamp(1.0 - (1.0 - dst[2]) * (1.0 - src[2] * src[3])),
            dst[3],
        ],
        _ => {
            let a = src[3].clamp(0.0, 1.0);
            [
                clamp(src[0] * a + dst[0] * (1.0 - a)),
                clamp(src[1] * a + dst[1] * (1.0 - a)),
                clamp(src[2] * a + dst[2] * (1.0 - a)),
                clamp(a + dst[3] * (1.0 - a)),
            ]
        }
    }
}

/// The program last read from `path`, reused until the file changes.
pub fn cached_program(path: &str) -> Option<Program> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::SystemTime;
    if path.is_empty() {
        return None;
    }
    static CACHE: OnceLock<Mutex<HashMap<String, (Option<SystemTime>, Program)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let stamp = std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
    let mut guard = cache.lock().ok()?;
    if let Some((saved, program)) = guard.get(path) {
        if *saved == stamp {
            return Some(program.clone());
        }
    }
    let text = std::fs::read_to_string(path).ok()?;
    let program = Program::parse(&text).ok()?;
    guard.insert(path.to_string(), (stamp, program.clone()));
    Some(program)
}

/// A picture to shade: RGBA8 pixels, row by row from the top.
pub struct Picture<'a> {
    pub pixels: &'a [u8],
    pub width: u32,
    pub height: u32,
}

impl Picture<'_> {
    /// Channel `c` (0 to 3) at `u`, `v` (0 to 1, v down) as 0 to 1; transparent outside.
    pub fn sample(&self, u: f32, v: f32, c: usize) -> f32 {
        if !(0.0..=1.0).contains(&u)
            || !(0.0..=1.0).contains(&v)
            || self.width == 0
            || self.height == 0
        {
            return 0.0;
        }
        let x = ((u * self.width as f32) as u32).min(self.width - 1);
        let y = ((v * self.height as f32) as u32).min(self.height - 1);
        self.at(x as i64, y as i64, c)
    }
    fn at(&self, x: i64, y: i64, c: usize) -> f32 {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return 0.0;
        }
        let i = ((y as usize * self.width as usize + x as usize) * 4) + c;
        self.pixels.get(i).map_or(0.0, |v| *v as f32 / 255.0)
    }
}

/// Repeatable value noise in 0..1 for a point.
pub fn noise(x: f32, y: f32) -> f32 {
    let hash = |ix: i32, iy: i32| {
        let mut h = (ix as u32).wrapping_mul(0x8da6_b343) ^ (iy as u32).wrapping_mul(0xd816_3841);
        h = (h ^ (h >> 13)).wrapping_mul(0x85eb_ca6b);
        h ^= h >> 16;
        (h & 0xffff) as f32 / 65535.0
    };
    let (fx, fy) = (x.floor(), y.floor());
    let (tx, ty) = (x - fx, y - fy);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let (ix, iy) = (fx as i32, fy as i32);
    let top = hash(ix, iy) + (hash(ix + 1, iy) - hash(ix, iy)) * sx;
    let bottom = hash(ix, iy + 1) + (hash(ix + 1, iy + 1) - hash(ix, iy + 1)) * sx;
    top + (bottom - top) * sy
}

fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

fn eval(node: &Node, vars: &[f32], pic: &Picture) -> f32 {
    match node {
        Node::Num(n) => *n,
        Node::Var(i) => vars[*i],
        Node::Neg(n) => -eval(n, vars, pic),
        Node::Bin(op, a, b) => {
            let (a, b) = (eval(a, vars, pic), eval(b, vars, pic));
            match op {
                '+' => a + b,
                '-' => a - b,
                '*' => a * b,
                '/' => {
                    if b == 0.0 {
                        0.0
                    } else {
                        a / b
                    }
                }
                _ => {
                    if b == 0.0 {
                        0.0
                    } else {
                        a.rem_euclid(b)
                    }
                }
            }
        }
        Node::Cmp(op, a, b) => {
            let (a, b) = (eval(a, vars, pic), eval(b, vars, pic));
            let yes = match *op {
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                "==" => (a - b).abs() < 1e-6,
                _ => (a - b).abs() >= 1e-6,
            };
            if yes {
                1.0
            } else {
                0.0
            }
        }
        Node::Call(f, args) => {
            let v: Vec<f32> = args.iter().map(|n| eval(n, vars, pic)).collect();
            match *f {
                "sin" => v[0].sin(),
                "cos" => v[0].cos(),
                "tan" => v[0].tan(),
                "abs" => v[0].abs(),
                "floor" => v[0].floor(),
                "fract" => v[0] - v[0].floor(),
                "sqrt" => v[0].max(0.0).sqrt(),
                "pow" => v[0].abs().powf(v[1]),
                "min" => v[0].min(v[1]),
                "max" => v[0].max(v[1]),
                "clamp" => v[0].max(v[1]).min(v[2]),
                "mix" => v[0] + (v[1] - v[0]) * v[2],
                "step" => {
                    if v[1] < v[0] {
                        0.0
                    } else {
                        1.0
                    }
                }
                "smoothstep" => {
                    let t = ((v[2] - v[0]) / (v[1] - v[0]).max(1e-6)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                }
                "noise" => noise(v[0], v[1]),
                "luma" => luma(vars[0], vars[1], vars[2]),
                "tex_r" => pic.sample(v[0], v[1], 0),
                "tex_g" => pic.sample(v[0], v[1], 1),
                "tex_b" => pic.sample(v[0], v[1], 2),
                _ => pic.sample(v[0], v[1], 3),
            }
        }
    }
}

/// The picture drawn through a material at `time` seconds: new RGBA8 pixels of the same size.
/// A `custom` material without a readable program draws the picture unchanged.
pub fn shade(pic: &Picture, m: &Material2d, program: Option<&Program>, time: f32) -> Vec<u8> {
    let (w, h) = (pic.width as usize, pic.height as usize);
    let mut out = pic.pixels.to_vec();
    if w == 0 || h == 0 || out.len() < w * h * 4 {
        return out;
    }
    let amount = m.amount.clamp(0.0, 1.0);
    let tau = std::f32::consts::TAU;
    let pulse = |speed: f32| if speed > 0.0 { 0.5 + 0.5 * (time * speed * tau).sin() } else { 1.0 };
    let to_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let [cr, cg, cb, ca] = m.color;
    match m.effect.as_str() {
        "grayscale" | "flash" | "ramp" => {
            let k = if m.effect == "flash" { amount * pulse(m.speed) } else { amount };
            for px in out.chunks_exact_mut(4) {
                let [r, g, b] = [px[0], px[1], px[2]].map(|c| c as f32 / 255.0);
                let l = luma(r, g, b);
                let target = match m.effect.as_str() {
                    "grayscale" => [l, l, l],
                    "flash" => [cr, cg, cb],
                    _ => [l * cr, l * cg, l * cb],
                };
                for (c, (from, to)) in [r, g, b].into_iter().zip(target).enumerate() {
                    px[c] = to_byte(from + (to - from) * k);
                }
            }
        }
        "outline" => {
            let thick = (1.0 + amount * 3.0).round() as i64;
            for y in 0..h as i64 {
                for x in 0..w as i64 {
                    if pic.at(x, y, 3) >= 0.5 {
                        continue;
                    }
                    let near = (-thick..=thick).any(|dy| {
                        (-thick..=thick).any(|dx| {
                            dx * dx + dy * dy <= thick * thick && pic.at(x + dx, y + dy, 3) >= 0.5
                        })
                    });
                    if near {
                        let i = (y as usize * w + x as usize) * 4;
                        out[i..i + 4].copy_from_slice(&[
                            to_byte(cr),
                            to_byte(cg),
                            to_byte(cb),
                            to_byte(ca),
                        ]);
                    }
                }
            }
        }
        "dissolve" => {
            let cut = amount * pulse(m.speed);
            for y in 0..h {
                for x in 0..w {
                    let n = noise(x as f32 * 0.35, y as f32 * 0.35);
                    let i = (y * w + x) * 4;
                    if cut >= 1.0 || n < cut {
                        out[i + 3] = 0;
                    } else if cut > 0.0 && n < cut + 0.08 && out[i + 3] > 0 {
                        out[i..i + 3].copy_from_slice(&[to_byte(cr), to_byte(cg), to_byte(cb)]);
                    }
                }
            }
        }
        "particle_anim" => {
            let cols = m.sheet_cols.max(1);
            let rows = m.sheet_rows.max(1);
            let frame = particle_anim_frame(cols, rows, 1.0, m.sheet_loop, time, m.speed);
            let col = frame % cols;
            let row = frame / cols;
            let du = 1.0 / cols as f32;
            let dv = 1.0 / rows as f32;
            let u0 = col as f32 * du;
            let v0 = row as f32 * dv;
            let u1 = (col + 1) as f32 * du;
            let v1 = (row + 1) as f32 * dv;
            for y in 0..h {
                for x in 0..w {
                    let su = u0 + (x as f32 + 0.5) / w as f32 * (u1 - u0);
                    let sv = v0 + (y as f32 + 0.5) / h as f32 * (v1 - v0);
                    let i = (y * w + x) * 4;
                    for c in 0..4 {
                        out[i + c] = to_byte(pic.sample(su, sv, c));
                    }
                }
            }
        }
        "wave" => {
            for y in 0..h {
                let v = (y as f32 + 0.5) / h as f32;
                let shift = amount * 0.08 * w as f32 * (v * tau * 2.0 + time * m.speed * tau).sin();
                for x in 0..w {
                    let from = (x as f32 - shift).round() as i64;
                    let i = (y * w + x) * 4;
                    for c in 0..4 {
                        out[i + c] = to_byte(pic.at(from, y as i64, c));
                    }
                }
            }
        }
        "custom" => {
            let Some(program) = program else { return out };
            let mut vars = vec![0.0f32; program.names.len()];
            let base = [w as f32, h as f32, time, amount, m.speed, cr, cg, cb, ca];
            let n = base.len().min(vars.len().saturating_sub(8));
            vars[8..8 + n].copy_from_slice(&base[..n]);
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) * 4;
                    for c in 0..4 {
                        vars[c] = pic.pixels[i + c] as f32 / 255.0;
                    }
                    vars[4] = (x as f32 + 0.5) / w as f32;
                    vars[5] = (y as f32 + 0.5) / h as f32;
                    vars[6] = x as f32;
                    vars[7] = y as f32;
                    for (slot, node) in &program.lines {
                        let value = eval(node, &vars, pic);
                        vars[*slot] = if value.is_finite() { value } else { 0.0 };
                    }
                    for c in 0..4 {
                        out[i + c] = to_byte(vars[c]);
                    }
                }
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_read_formulas_and_refuse_mistakes() {
        let p = Program::parse("# tint\nk = 0.5\nr = mix(r, 1, k) // half way\n").unwrap();
        assert_eq!(p.lines.len(), 2);
        assert_eq!(Program::parse("r = sin(").unwrap_err().line, 1);
        assert!(Program::parse("\nr = glow").unwrap_err().message.contains("glow"));
        assert!(Program::parse("u = 1").unwrap_err().message.contains("input"));
        assert!(Program::parse("r = pow(1)").unwrap_err().message.contains("takes 2"));
        assert!(Program::parse("r 1").is_err());
    }

    #[test]
    fn vertex_and_light_sections_run_and_a_bad_line_names_itself() {
        let p = Program::parse(
            "blend = additive\nlight_mode = unlit\nvertex\nx = x + 2\ny = y + sin(0)\nlight\nr = light_r\ng = light_g\nb = light_b\n",
        )
        .unwrap();
        assert_eq!(p.blend_mode(), Some("additive"));
        assert_eq!(p.light_mode(), Some("unlit"));
        let moved = p.vertex_at(1.0, 4.0, 0.0, 0.0, 0.0, 1.0, 0.0);
        assert!((moved[0] - 3.0).abs() < 1e-4 && (moved[1] - 4.0).abs() < 1e-4, "{moved:?}");
        let lit = p.light_at_surface([1.0, 0.0, 0.0], [0.0, 0.5, 0.0], 1.0, [0.0, 0.0, 1.0], 1.0);
        assert!((lit[1] - 0.5).abs() < 1e-4 && lit[0].abs() < 1e-4, "{lit:?}");
        let err = Program::parse("vertex\nx = (\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.to_string().contains("line 2"));
        let plain = Program::parse("r = r\n").unwrap();
        assert_eq!(plain.vertex_at(3.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0), [3.0, 4.0]);
    }

    #[test]
    fn unlit_ignores_the_light_and_blend_math_is_stable() {
        let albedo = [0.2, 0.4, 0.6];
        let incoming = [1.0, 0.0, 0.0];
        assert_eq!(apply_light_mode(albedo, incoming, "unlit"), albedo);
        assert_eq!(apply_light_mode(albedo, incoming, "unshaded"), albedo);
        assert_eq!(apply_light_mode(albedo, incoming, "light"), incoming);
        assert_eq!(apply_light_mode(albedo, incoming, "lit"), [0.2, 0.0, 0.0]);
        let dst = [0.5, 0.5, 0.5, 1.0];
        let src = [1.0, 0.0, 0.0, 0.5];
        let alpha = blend_pixel(dst, src, "alpha");
        assert!((alpha[0] - 0.75).abs() < 1e-4, "{alpha:?}");
        let add = blend_pixel(dst, src, "additive");
        assert!((add[0] - 1.0).abs() < 1e-4, "{add:?}");
        let mul = blend_pixel(dst, src, "multiply");
        assert!((mul[0] - 0.5).abs() < 1e-4 && mul[1].abs() < 1e-4, "{mul:?}");
        let pre = blend_pixel(dst, [0.5, 0.0, 0.0, 0.5], "premultiplied");
        assert!((pre[0] - 0.75).abs() < 1e-4, "{pre:?}");
        let screen = blend_pixel(dst, src, "screen");
        assert!(screen[0] > dst[0], "{screen:?}");
    }

    #[test]
    fn noise_is_repeatable_and_in_range() {
        for i in 0..50 {
            let n = noise(i as f32 * 0.37, i as f32 * 1.3);
            assert!((0.0..=1.0).contains(&n));
            assert_eq!(n, noise(i as f32 * 0.37, i as f32 * 1.3));
        }
    }
}

/// Read a canvas shader file, or say why it cannot be used.
pub fn load_program(path: &std::path::Path) -> Result<Program, String> {
    let text =
        std::fs::read_to_string(path).map_err(|_| format!("cannot read {}", path.display()))?;
    Program::parse(&text).map_err(|e| e.to_string())
}
