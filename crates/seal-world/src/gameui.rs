//! In-game UI screens: anchored layout, containers, controls, themes, and
//! keyboard, mouse and gamepad focus.
//!
//! A screen is a `.sealui` text file in the project. Each node line names a
//! kind and a name; the indented lines under it set its properties, and
//! nodes indented further are its children:
//!
//! ```text
//! seal ui 1
//! theme ui/dark.sealtheme
//! panel Menu
//!   anchor 0.5 0.5 0.5 0.5
//!   offset -150 -110 150 110
//!   vbox Items
//!     anchor 0 0 1 1
//!     button Start
//!       text Play
//! ```
//!
//! Children of a panel or the screen are placed by anchors (0 to 1 across the
//! parent) and pixel offsets from those anchors. Children of a vbox, hbox,
//! grid or scroll are stacked; their `size` sets how much room they take.
//! Nothing here draws: [`draw`] turns a screen into rectangles, text and
//! pictures that the editor's play view and packaged games paint.

use std::collections::{BTreeMap, BTreeSet};

pub const SCREEN_EXT: &str = "sealui";
pub const THEME_EXT: &str = "sealtheme";

/// Every node kind, containers first.
pub const KINDS: &[&str] = &[
    "panel", "vbox", "hbox", "grid", "scroll", "button", "label", "image", "slider", "toggle",
    "dropdown", "input", "list", "progress",
];
pub const CONTAINERS: &[&str] = &["panel", "vbox", "hbox", "grid", "scroll"];
/// Controls keyboard and gamepad focus can land on.
pub const FOCUSABLE: &[&str] = &["button", "slider", "toggle", "dropdown", "input", "list"];
/// Properties a node can set, in the order they are written.
pub const PROPS: &[&str] = &[
    "anchor",
    "offset",
    "size",
    "text",
    "placeholder",
    "picture",
    "color",
    "value",
    "min",
    "max",
    "step",
    "checked",
    "options",
    "selected",
    "columns",
    "spacing",
    "padding",
    "font_size",
    "font",
    "align",
    "visible",
    "enabled",
    "typewriter",
    "wave",
    "shake",
    "slice",
    "slice_mode",
];

/// Anchor presets for the designer: name and anchor (left, top, right, bottom).
pub const ANCHOR_PRESETS: &[(&str, [f32; 4])] = &[
    ("top-left", [0.0, 0.0, 0.0, 0.0]),
    ("top", [0.5, 0.0, 0.5, 0.0]),
    ("top-right", [1.0, 0.0, 1.0, 0.0]),
    ("left", [0.0, 0.5, 0.0, 0.5]),
    ("center", [0.5, 0.5, 0.5, 0.5]),
    ("right", [1.0, 0.5, 1.0, 0.5]),
    ("bottom-left", [0.0, 1.0, 0.0, 1.0]),
    ("bottom", [0.5, 1.0, 0.5, 1.0]),
    ("bottom-right", [1.0, 1.0, 1.0, 1.0]),
    ("top-wide", [0.0, 0.0, 1.0, 0.0]),
    ("bottom-wide", [0.0, 1.0, 1.0, 1.0]),
    ("left-tall", [0.0, 0.0, 0.0, 1.0]),
    ("right-tall", [1.0, 0.0, 1.0, 1.0]),
    ("fill", [0.0, 0.0, 1.0, 1.0]),
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(&self, p: [f32; 2]) -> bool {
        p[0] >= self.x && p[1] >= self.y && p[0] < self.x + self.w && p[1] < self.y + self.h
    }
    pub fn center(&self) -> [f32; 2] {
        [self.x + self.w * 0.5, self.y + self.h * 0.5]
    }
    pub fn inset(&self, by: f32) -> Rect {
        Rect::new(
            self.x + by,
            self.y + by,
            (self.w - 2.0 * by).max(0.0),
            (self.h - 2.0 * by).max(0.0),
        )
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = (self.x + self.w).min(o.x + o.w);
        let b = (self.y + self.h).min(o.y + o.h);
        Rect::new(x, y, (r - x).max(0.0), (b - y).max(0.0))
    }
}

/// One node of a screen. Nodes are kept flat, parents before children.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: String,
    pub name: String,
    pub parent: Option<usize>,
    /// Left, top, right, bottom, each 0 to 1 across the parent.
    pub anchor: [f32; 4],
    /// Pixels from the anchors: left, top, right, bottom.
    pub offset: [f32; 4],
    /// Width and height inside a stacking container; 0 picks a size from the content.
    pub size: [f32; 2],
    pub text: String,
    pub placeholder: String,
    pub image: String,
    /// Tint for panels, labels and pictures; `None` uses the theme.
    pub color: Option<[f32; 4]>,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub checked: bool,
    /// Dropdown choices and list rows.
    pub options: Vec<String>,
    pub selected: usize,
    pub columns: usize,
    /// `None` uses the theme.
    pub spacing: Option<f32>,
    pub padding: Option<f32>,
    pub font_size: Option<f32>,
    /// A .ttf or .otf file in the project; empty uses the theme's font.
    pub font: String,
    /// left, center or right.
    pub align: String,
    pub visible: bool,
    pub enabled: bool,
    /// Characters revealed per second. 0 shows the whole text at once.
    pub typewriter: f32,
    /// Up-and-down motion per character: amplitude in pixels, speed in cycles per second.
    pub wave: [f32; 2],
    /// Per-character shake: amplitude in pixels, speed in cycles per second.
    pub shake: [f32; 2],
    /// Nine-slice borders in picture pixels for `image` nodes.
    pub slice: [f32; 4],
    /// `stretch` or `tile` for nine-slice edges.
    pub slice_mode: String,
}

impl Node {
    pub fn new(kind: &str, name: &str) -> Self {
        let mut n = Node {
            kind: kind.into(),
            name: name.into(),
            parent: None,
            anchor: [0.0; 4],
            offset: [0.0; 4],
            size: [0.0; 2],
            text: String::new(),
            placeholder: String::new(),
            image: String::new(),
            color: None,
            value: 0.0,
            min: 0.0,
            max: 1.0,
            step: 0.1,
            checked: false,
            options: Vec::new(),
            selected: 0,
            columns: 2,
            spacing: None,
            padding: None,
            font_size: None,
            font: String::new(),
            align: String::new(),
            visible: true,
            enabled: true,
            typewriter: 0.0,
            wave: [0.0, 0.0],
            shake: [0.0, 0.0],
            slice: [0.0; 4],
            slice_mode: "stretch".into(),
        };
        // New controls start with something to see.
        match kind {
            "button" | "label" | "toggle" => n.text = name.into(),
            "dropdown" | "list" => n.options = vec!["One".into(), "Two".into(), "Three".into()],
            "input" => n.placeholder = "Type here".into(),
            "progress" => n.value = 0.5,
            _ => {}
        }
        if kind == "label" {
            n.align = "left".into();
        } else {
            n.align = "center".into();
        }
        n
    }

    pub fn is_container(&self) -> bool {
        CONTAINERS.contains(&self.kind.as_str())
    }
    pub fn focusable(&self) -> bool {
        FOCUSABLE.contains(&self.kind.as_str()) && self.enabled
    }
    /// The value as 0 to 1 between min and max.
    pub fn fraction(&self) -> f32 {
        if self.max <= self.min {
            return 0.0;
        }
        ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }
    fn set_value(&mut self, v: f32) {
        let mut v = v.clamp(self.min.min(self.max), self.max.max(self.min));
        if self.step > 0.0 {
            v = self.min + ((v - self.min) / self.step).round() * self.step;
            v = v.clamp(self.min.min(self.max), self.max.max(self.min));
        }
        // Keep steps like 0.1 from drifting to 0.30000001.
        self.value = (v * 10000.0).round() / 10000.0;
    }

    /// A property as text, as the file and the designer show it.
    pub fn prop(&self, key: &str) -> Option<String> {
        let nums = |v: &[f32]| v.iter().map(|x| num(*x)).collect::<Vec<_>>().join(" ");
        Some(match key {
            "anchor" => nums(&self.anchor),
            "offset" => nums(&self.offset),
            "size" => nums(&self.size),
            "text" => self.text.clone(),
            "placeholder" => self.placeholder.clone(),
            "picture" => self.image.clone(),
            "color" => self.color.map(|c| hex(&c)).unwrap_or_default(),
            "value" => num(self.value),
            "min" => num(self.min),
            "max" => num(self.max),
            "step" => num(self.step),
            "checked" => yes(self.checked),
            "options" => self.options.join(" | "),
            "selected" => self.selected.to_string(),
            "columns" => self.columns.to_string(),
            "spacing" => self.spacing.map(num).unwrap_or_default(),
            "padding" => self.padding.map(num).unwrap_or_default(),
            "font_size" => self.font_size.map(num).unwrap_or_default(),
            "font" => self.font.clone(),
            "align" => self.align.clone(),
            "visible" => yes(self.visible),
            "enabled" => yes(self.enabled),
            "typewriter" => {
                if self.typewriter == 0.0 {
                    String::new()
                } else {
                    num(self.typewriter)
                }
            }
            "wave" => pair_text(self.wave),
            "shake" => pair_text(self.shake),
            "slice" => crate::slice_text(&self.slice),
            "slice_mode" => self.slice_mode.clone(),
            _ => return None,
        })
    }

    /// Set a property from text. Returns why it was refused, or `None` when it took.
    pub fn set_prop(&mut self, key: &str, value: &str) -> Option<&'static str> {
        let v = value.trim();
        let floats = |n: usize| -> Option<Vec<f32>> {
            let parts: Vec<f32> =
                v.split_whitespace().map(|p| p.parse::<f32>()).collect::<Result<_, _>>().ok()?;
            (parts.len() == n && parts.iter().all(|x| x.is_finite())).then_some(parts)
        };
        let float = || v.parse::<f32>().ok().filter(|x| x.is_finite());
        let opt_float = |lo: f32, hi: f32| -> Result<Option<f32>, ()> {
            if v.is_empty() {
                return Ok(None);
            }
            match float() {
                Some(x) if (lo..=hi).contains(&x) => Ok(Some(x)),
                _ => Err(()),
            }
        };
        let flag = || match v {
            "yes" | "true" | "1" | "on" => Some(true),
            "no" | "false" | "0" | "off" => Some(false),
            _ => None,
        };
        match key {
            "anchor" => {
                let Some(a) = floats(4) else { return Some("ui-error-anchor") };
                if a.iter().any(|x| !(0.0..=1.0).contains(x)) || a[0] > a[2] || a[1] > a[3] {
                    return Some("ui-error-anchor");
                }
                self.anchor = [a[0], a[1], a[2], a[3]];
            }
            "offset" => {
                let Some(o) = floats(4) else { return Some("ui-error-offset") };
                self.offset = [o[0], o[1], o[2], o[3]];
            }
            "size" => match floats(2) {
                Some(s) if s.iter().all(|x| *x >= 0.0) => self.size = [s[0], s[1]],
                _ => return Some("ui-error-size"),
            },
            "text" => self.text = value.to_string(),
            "placeholder" => self.placeholder = value.to_string(),
            "picture" => self.image = v.replace('\\', "/"),
            "color" => {
                if v.is_empty() {
                    self.color = None;
                } else {
                    let Some(c) = parse_hex(v) else { return Some("ui-error-color") };
                    self.color = Some(c);
                }
            }
            "value" | "min" | "max" => {
                let Some(x) = float() else { return Some("ui-error-number") };
                match key {
                    "value" => self.value = x,
                    "min" => self.min = x,
                    _ => self.max = x,
                }
            }
            "step" => match float() {
                Some(x) if x >= 0.0 => self.step = x,
                _ => return Some("ui-error-number"),
            },
            "checked" | "visible" | "enabled" => {
                let Some(b) = flag() else { return Some("ui-error-flag") };
                match key {
                    "checked" => self.checked = b,
                    "visible" => self.visible = b,
                    _ => self.enabled = b,
                }
            }
            "options" => {
                self.options =
                    v.split('|').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                self.selected = self.selected.min(self.options.len().saturating_sub(1));
            }
            "selected" => match v.parse::<usize>() {
                Ok(i) if i < self.options.len().max(1) => self.selected = i,
                _ => return Some("ui-error-selected"),
            },
            "columns" => match v.parse::<usize>() {
                Ok(c) if (1..=32).contains(&c) => self.columns = c,
                _ => return Some("ui-error-columns"),
            },
            "spacing" | "padding" => {
                let Ok(x) = opt_float(0.0, 1000.0) else { return Some("ui-error-number") };
                if key == "spacing" {
                    self.spacing = x;
                } else {
                    self.padding = x;
                }
            }
            "font_size" => {
                let Ok(x) = opt_float(4.0, 400.0) else { return Some("ui-error-font-size") };
                self.font_size = x;
            }
            "font" => match font_path(v) {
                Some(f) => self.font = f,
                None => return Some("ui-error-font"),
            },
            "align" => {
                if !matches!(v, "left" | "center" | "right") {
                    return Some("ui-error-align");
                }
                self.align = v.into();
            }
            "typewriter" => match opt_float(0.0, 400.0) {
                Ok(x) => self.typewriter = x.unwrap_or(0.0),
                Err(()) => return Some("ui-error-number"),
            },
            "wave" => match pair_or_empty(v) {
                Some(p) => self.wave = p,
                None => return Some("ui-error-pair"),
            },
            "shake" => match pair_or_empty(v) {
                Some(p) => self.shake = p,
                None => return Some("ui-error-pair"),
            },
            "slice" => {
                let Some(s) = crate::parse_slice(v) else { return Some("ui-error-slice") };
                self.slice = s;
            }
            "slice_mode" => {
                if crate::sprite_slice::MODES.contains(&v) {
                    self.slice_mode = v.to_string();
                } else {
                    return Some("ui-error-slice-mode");
                }
            }
            _ => return Some("ui-error-property"),
        }
        None
    }

    /// Which properties mean something for this kind, for the designer.
    pub fn props(&self) -> Vec<&'static str> {
        let mut out = vec!["anchor", "offset", "size", "visible"];
        let extra: &[&str] = match self.kind.as_str() {
            "panel" => &["color", "padding"],
            "vbox" | "hbox" | "scroll" => &["spacing", "padding", "color"],
            "grid" => &["columns", "spacing", "padding", "color"],
            "button" => &["text", "font_size", "font", "enabled", "typewriter", "wave", "shake"],
            "label" => {
                &["text", "font_size", "font", "align", "color", "typewriter", "wave", "shake"]
            }
            "image" => &["picture", "color", "slice", "slice_mode"],
            "slider" => &["value", "min", "max", "step", "enabled"],
            "toggle" => &["text", "font", "checked", "enabled"],
            "dropdown" => &["options", "selected", "font", "enabled"],
            "input" => &["text", "placeholder", "font", "enabled"],
            "list" => &["options", "selected", "font", "enabled"],
            "progress" => &["value", "min", "max", "color"],
            _ => &[],
        };
        out.extend_from_slice(extra);
        out
    }
}

fn yes(b: bool) -> String {
    if b { "yes" } else { "no" }.into()
}

fn pair_text(v: [f32; 2]) -> String {
    if v == [0.0, 0.0] {
        String::new()
    } else {
        format!("{} {}", num(v[0]), num(v[1]))
    }
}

fn pair_or_empty(v: &str) -> Option<[f32; 2]> {
    if v.is_empty() {
        return Some([0.0, 0.0]);
    }
    let parts: Vec<&str> = v.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let a = parts[0].parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 0.0)?;
    let b = parts[1].parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 0.0)?;
    Some([a, b])
}

pub fn num(x: f32) -> String {
    let r = (x * 10000.0).round() / 10000.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// `#RRGGBB` or `#RRGGBBAA` to 0..1 RGBA.
pub fn parse_hex(text: &str) -> Option<[f32; 4]> {
    let h = text.trim().strip_prefix('#')?;
    if !(h.len() == 6 || h.len() == 8) || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|b| b as f32 / 255.0);
    Some([byte(0)?, byte(2)?, byte(4)?, if h.len() == 8 { byte(6)? } else { 1.0 }])
}

pub fn hex(c: &[f32; 4]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if b(c[3]) == 255 {
        format!("#{:02X}{:02X}{:02X}", b(c[0]), b(c[1]), b(c[2]))
    } else {
        format!("#{:02X}{:02X}{:02X}{:02X}", b(c[0]), b(c[1]), b(c[2]), b(c[3]))
    }
}

/// A screen: its theme file and its nodes, parents first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Screen {
    pub theme: String,
    /// A `.strings` file. Label text that starts with `@key` shows that entry.
    pub strings: String,
    pub nodes: Vec<Node>,
}

/// A mistake in a screen or theme file, with its line.
#[derive(Clone, Debug, PartialEq)]
pub struct FileError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl Screen {
    /// A new screen: one full-screen panel holding a centred column with a title and a button.
    pub fn starter() -> Self {
        let mut s = Screen::default();
        let mut root = Node::new("panel", "Screen");
        root.anchor = [0.0, 0.0, 1.0, 1.0];
        root.color = Some([0.0, 0.0, 0.0, 0.0]);
        s.nodes.push(root);
        let mut col = Node::new("vbox", "Menu");
        col.anchor = [0.5, 0.5, 0.5, 0.5];
        col.offset = [-140.0, -80.0, 140.0, 80.0];
        col.parent = Some(0);
        s.nodes.push(col);
        let mut title = Node::new("label", "Title");
        title.text = "My Game".into();
        title.align = "center".into();
        title.font_size = Some(32.0);
        title.parent = Some(1);
        s.nodes.push(title);
        let mut play = Node::new("button", "Play");
        play.parent = Some(1);
        s.nodes.push(play);
        s
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.name == name)
    }

    pub fn children(&self, i: Option<usize>) -> impl Iterator<Item = usize> + '_ {
        self.nodes.iter().enumerate().filter(move |(_, n)| n.parent == i).map(|(j, _)| j)
    }

    /// Whether the node and every parent above it are visible.
    pub fn shown(&self, mut i: usize) -> bool {
        loop {
            let n = &self.nodes[i];
            if !n.visible {
                return false;
            }
            match n.parent {
                Some(p) => i = p,
                None => return true,
            }
        }
    }

    /// A name no node has yet, from `base`.
    pub fn free_name(&self, base: &str) -> String {
        let base: String = base.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
        let base = if base.is_empty() { "Node".to_string() } else { base };
        if self.find(&base).is_none() {
            return base;
        }
        (2..).map(|k| format!("{base}{k}")).find(|n| self.find(n).is_none()).unwrap()
    }

    /// Add a node of `kind` under `parent` (its last child). Returns its index.
    pub fn add(&mut self, kind: &str, parent: Option<usize>) -> Option<usize> {
        if !KINDS.contains(&kind) {
            return None;
        }
        // Adding while a control is selected puts the new node beside it.
        let parent = parent.filter(|&p| p < self.nodes.len());
        let parent = match parent {
            Some(p) if !self.nodes[p].is_container() => self.nodes[p].parent,
            other => other,
        };
        let name = self.free_name(&capital(kind));
        let mut n = Node::new(kind, &name);
        n.parent = parent;
        // Free placement: a sensible box near the top left of the parent.
        let (w, h) = default_size(&n, &Theme::default());
        n.offset = [20.0, 20.0, 20.0 + w, 20.0 + h];
        let at = self.subtree_end(parent);
        self.insert(at, n);
        Some(at)
    }

    fn subtree_end(&self, parent: Option<usize>) -> usize {
        let Some(p) = parent else { return self.nodes.len() };
        let mut end = p + 1;
        while end < self.nodes.len() && self.is_under(end, p) {
            end += 1;
        }
        end
    }

    fn is_under(&self, mut i: usize, ancestor: usize) -> bool {
        while let Some(p) = self.nodes[i].parent {
            if p == ancestor {
                return true;
            }
            i = p;
        }
        false
    }

    fn insert(&mut self, at: usize, node: Node) {
        for n in &mut self.nodes {
            if let Some(p) = n.parent.as_mut() {
                if *p >= at {
                    *p += 1;
                }
            }
        }
        self.nodes.insert(at, node);
    }

    /// Remove a node and everything inside it.
    pub fn remove(&mut self, i: usize) {
        if i >= self.nodes.len() {
            return;
        }
        let end = self.subtree_end(Some(i));
        let gone = end - i;
        self.nodes.drain(i..end);
        for n in &mut self.nodes {
            if let Some(p) = n.parent {
                if p >= end {
                    n.parent = Some(p - gone);
                }
            }
        }
    }

    /// Swap a node (with its children) with the sibling before (`up`) or after it.
    /// Returns the node's new index.
    pub fn move_sibling(&mut self, i: usize, up: bool) -> usize {
        let parent = self.nodes[i].parent;
        let sibs: Vec<usize> = self.children(parent).collect();
        let Some(k) = sibs.iter().position(|&s| s == i) else { return i };
        let (a, b) = if up {
            if k == 0 {
                return i;
            }
            (sibs[k - 1], i)
        } else {
            if k + 1 >= sibs.len() {
                return i;
            }
            (i, sibs[k + 1])
        };
        // Blocks [a, b) and [b, end_b) trade places.
        let end_b = self.subtree_end(Some(b));
        let first: Vec<Node> = self.nodes[a..b].to_vec();
        let second: Vec<Node> = self.nodes[b..end_b].to_vec();
        let shift_second = a as isize - b as isize;
        let shift_first = second.len() as isize;
        let remap = |p: usize| -> usize {
            if p >= a && p < b {
                (p as isize + shift_first) as usize
            } else if p >= b && p < end_b {
                (p as isize + shift_second) as usize
            } else {
                p
            }
        };
        let mut block: Vec<Node> = second.into_iter().chain(first).collect();
        for n in &mut block {
            n.parent = n.parent.map(remap);
        }
        // Both blocks are whole subtrees, so nothing outside them points into them.
        self.nodes.splice(a..end_b, block);
        remap(i)
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("seal ui 1\n");
        if !self.theme.is_empty() {
            out.push_str(&format!("theme {}\n", self.theme));
        }
        if !self.strings.is_empty() {
            out.push_str(&format!("strings {}\n", self.strings));
        }
        let blank = |kind: &str| Node::new(kind, "x");
        for (i, n) in self.nodes.iter().enumerate() {
            let depth = self.depth(i);
            let pad = "  ".repeat(depth);
            out.push_str(&format!("{pad}{} {}\n", n.kind, n.name));
            let d = blank(&n.kind);
            for key in PROPS {
                let (now, was) = (n.prop(key).unwrap_or_default(), d.prop(key).unwrap_or_default());
                if now != was {
                    out.push_str(&format!("{pad}  {key} {}\n", quote(&now)));
                }
            }
        }
        out
    }

    pub fn depth(&self, mut i: usize) -> usize {
        let mut d = 0;
        while let Some(p) = self.nodes[i].parent {
            d += 1;
            i = p;
        }
        d
    }

    pub fn from_text(text: &str) -> Result<Screen, FileError> {
        let err = |line: usize, message: String| FileError { line, message };
        let mut screen = Screen::default();
        // Open nodes by depth: (indent, node index).
        let mut stack: Vec<(usize, usize)> = Vec::new();
        let mut seen_header = false;
        for (k, raw) in text.lines().enumerate() {
            let line = k + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let indent = raw.len() - raw.trim_start().len();
            if !seen_header {
                if trimmed != "seal ui 1" {
                    return Err(err(line, "the file must start with \"seal ui 1\"".into()));
                }
                seen_header = true;
                continue;
            }
            let (word, rest) = trimmed.split_once(char::is_whitespace).unwrap_or((trimmed, ""));
            let rest = rest.trim();
            if indent == 0 && word == "theme" {
                screen.theme = unquote(rest).replace('\\', "/");
                continue;
            }
            if indent == 0 && word == "strings" {
                let path = unquote(rest).replace('\\', "/");
                if !path.is_empty()
                    && (!path.ends_with(&format!(".{}", crate::text_sprites::STRINGS_EXT))
                        || path.split('/').any(|p| p == ".."))
                {
                    return Err(err(line, "strings must be a .strings file in the project".into()));
                }
                screen.strings = path;
                continue;
            }
            if KINDS.contains(&word) {
                if rest.is_empty() || rest.contains(char::is_whitespace) {
                    return Err(err(line, format!("{word} needs one name with no spaces")));
                }
                if screen.find(rest).is_some() {
                    return Err(err(line, format!("two nodes are called {rest}")));
                }
                while stack.last().is_some_and(|(d, _)| *d >= indent) {
                    stack.pop();
                }
                let parent = stack.last().map(|(_, i)| *i);
                if let Some(p) = parent {
                    if !screen.nodes[p].is_container() {
                        return Err(err(
                            line,
                            format!("{} cannot hold other nodes", screen.nodes[p].name),
                        ));
                    }
                }
                let mut n = Node::new(word, rest);
                n.parent = parent;
                screen.nodes.push(n);
                stack.push((indent, screen.nodes.len() - 1));
                continue;
            }
            if PROPS.contains(&word) {
                let Some(&(d, i)) = stack.last() else {
                    return Err(err(line, format!("{word} is not under a node")));
                };
                if indent <= d {
                    return Err(err(line, format!("indent {word} under its node")));
                }
                if let Some(why) = screen.nodes[i].set_prop(word, &unquote(rest)) {
                    return Err(err(line, format!("{word}: {}", plain_error(why))));
                }
                continue;
            }
            return Err(err(line, format!("unknown word \"{word}\"")));
        }
        if !seen_header {
            return Err(err(1, "the file is empty".into()));
        }
        Ok(screen)
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
}

fn quote(s: &str) -> String {
    if s.is_empty()
        || s.starts_with(char::is_whitespace)
        || s.ends_with(char::is_whitespace)
        || s.starts_with('"')
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let inner = &s[1..s.len() - 1];
        let mut out = String::new();
        let mut esc = false;
        for c in inner.chars() {
            if esc {
                out.push(c);
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else {
                out.push(c);
            }
        }
        out
    } else {
        s.to_string()
    }
}

/// Plain English for a refusal key, for file errors (the editor uses its own messages).
pub fn plain_error(key: &str) -> &'static str {
    match key {
        "ui-error-anchor" => {
            "four numbers from 0 to 1: left, top, right, bottom (left not past right)"
        }
        "ui-error-offset" => "four numbers: left, top, right, bottom",
        "ui-error-size" => "two numbers, 0 or more: width and height",
        "ui-error-color" => "a color like #FF8800 or #FF880080",
        "ui-error-number" => "a number",
        "ui-error-pair" => "two numbers: amount and speed, or nothing",
        "ui-error-flag" => "yes or no",
        "ui-error-selected" => "the number of a choice, counting from 0",
        "ui-error-columns" => "a whole number from 1 to 32",
        "ui-error-font-size" => "a size from 4 to 400",
        "ui-error-align" => "left, center or right",
        "ui-error-font" => "a .ttf or .otf file in the project, or nothing for the default font",
        _ => "not a property",
    }
}

/// Colors and sizes every control uses unless it sets its own.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub font_size: f32,
    /// A .ttf or .otf file in the project; empty uses Seal's built-in font.
    pub font: String,
    pub text: [f32; 4],
    pub dim_text: [f32; 4],
    pub panel: [f32; 4],
    pub button: [f32; 4],
    pub button_hover: [f32; 4],
    pub button_press: [f32; 4],
    pub field: [f32; 4],
    pub accent: [f32; 4],
    pub border: [f32; 4],
    pub focus: [f32; 4],
    pub padding: f32,
    pub spacing: f32,
    pub corner: f32,
}

impl Default for Theme {
    fn default() -> Self {
        let c = |s: &str| parse_hex(s).unwrap();
        Theme {
            font_size: 18.0,
            font: String::new(),
            text: c("#F2F2F2"),
            dim_text: c("#9AA0A8"),
            panel: c("#1C2027E6"),
            button: c("#2F6FD6"),
            button_hover: c("#3D82F0"),
            button_press: c("#2357AA"),
            field: c("#101318"),
            accent: c("#3D82F0"),
            border: c("#00000099"),
            focus: c("#FFD24A"),
            padding: 10.0,
            spacing: 8.0,
            corner: 4.0,
        }
    }
}

/// Theme keys, in file order.
pub const THEME_KEYS: &[&str] = &[
    "font_size",
    "font",
    "text",
    "dim_text",
    "panel",
    "button",
    "button_hover",
    "button_press",
    "field",
    "accent",
    "border",
    "focus",
    "padding",
    "spacing",
    "corner",
];

impl Theme {
    pub fn get(&self, key: &str) -> Option<String> {
        Some(match key {
            "font_size" => num(self.font_size),
            "font" => self.font.clone(),
            "padding" => num(self.padding),
            "spacing" => num(self.spacing),
            "corner" => num(self.corner),
            _ => hex(self.color_mut_ref(key)?),
        })
    }
    fn color_mut_ref(&self, key: &str) -> Option<&[f32; 4]> {
        Some(match key {
            "text" => &self.text,
            "dim_text" => &self.dim_text,
            "panel" => &self.panel,
            "button" => &self.button,
            "button_hover" => &self.button_hover,
            "button_press" => &self.button_press,
            "field" => &self.field,
            "accent" => &self.accent,
            "border" => &self.border,
            "focus" => &self.focus,
            _ => return None,
        })
    }
    pub fn set(&mut self, key: &str, value: &str) -> Option<&'static str> {
        let v = value.trim();
        let size = |lo: f32, hi: f32| v.parse::<f32>().ok().filter(|x| (lo..=hi).contains(x));
        match key {
            "font" => match font_path(v) {
                Some(f) => self.font = f,
                None => return Some("ui-error-font"),
            },
            "font_size" | "padding" | "spacing" | "corner" => {
                let (lo, hi, why) = match key {
                    "font_size" => (4.0, 400.0, "ui-error-font-size"),
                    "corner" => (0.0, 100.0, "ui-error-number"),
                    _ => (0.0, 200.0, "ui-error-number"),
                };
                let Some(x) = size(lo, hi) else { return Some(why) };
                match key {
                    "font_size" => self.font_size = x,
                    "padding" => self.padding = x,
                    "spacing" => self.spacing = x,
                    _ => self.corner = x,
                }
            }
            _ => {
                let Some(c) = parse_hex(v) else { return Some("ui-error-color") };
                let slot = match key {
                    "text" => &mut self.text,
                    "dim_text" => &mut self.dim_text,
                    "panel" => &mut self.panel,
                    "button" => &mut self.button,
                    "button_hover" => &mut self.button_hover,
                    "button_press" => &mut self.button_press,
                    "field" => &mut self.field,
                    "accent" => &mut self.accent,
                    "border" => &mut self.border,
                    "focus" => &mut self.focus,
                    _ => return Some("ui-error-property"),
                };
                *slot = c;
            }
        }
        None
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("seal theme 1\n");
        for k in THEME_KEYS {
            out.push_str(&format!("{k} {}\n", self.get(k).unwrap_or_default()));
        }
        out
    }

    /// Keys a file leaves out keep the default look.
    pub fn from_text(text: &str) -> Result<Theme, FileError> {
        let mut theme = Theme::default();
        let mut seen = false;
        for (k, raw) in text.lines().enumerate() {
            let line = k + 1;
            let t = raw.trim();
            if t.is_empty() || t.starts_with("//") {
                continue;
            }
            if !seen {
                if t != "seal theme 1" {
                    return Err(FileError {
                        line,
                        message: "the file must start with \"seal theme 1\"".into(),
                    });
                }
                seen = true;
                continue;
            }
            let (key, value) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
            if !THEME_KEYS.contains(&key) {
                return Err(FileError { line, message: format!("unknown theme key \"{key}\"") });
            }
            if let Some(why) = theme.set(key, value) {
                return Err(FileError { line, message: format!("{key}: {}", plain_error(why)) });
            }
        }
        Ok(theme)
    }
}

/// Rough text width: the play view and the designer use a proportional font
/// whose average glyph is about 0.55 of its size.
pub fn text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.55
}

/// Width used for layout. Plain text stays on the old measure so existing screens do not move.
fn label_width(text: &str, size: f32) -> f32 {
    if text.contains("[img=") {
        crate::text_sprites::measure_width(text, size)
    } else {
        text_width(text, size)
    }
}

fn font(n: &Node, theme: &Theme) -> f32 {
    n.font_size.unwrap_or(theme.font_size)
}

/// The font file a node draws with: its own, else the theme's, else "" (built in).
pub fn font_file<'a>(n: &'a Node, theme: &'a Theme) -> &'a str {
    if n.font.is_empty() {
        &theme.font
    } else {
        &n.font
    }
}

/// Turn project-relative picture and font paths in a paint list into full paths.
pub fn resolve_paths(draws: &mut [Draw], root: &std::path::Path) {
    for d in draws {
        match d {
            Draw::Picture { path, .. } => *path = root.join(&*path).to_string_lossy().into_owned(),
            Draw::Text { font, .. } if !font.is_empty() => {
                *font = root.join(&*font).to_string_lossy().into_owned()
            }
            _ => {}
        }
    }
}

/// Font files a screen can use.
pub const FONT_EXTS: &[&str] = &["ttf", "otf"];

/// A project-relative font path, or `Some("")` for none; `None` when it is not a font path.
fn font_path(v: &str) -> Option<String> {
    let v = unquote(v.trim()).replace('\\', "/");
    if v.is_empty() {
        return Some(v);
    }
    let ext = v.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let inside = !v.starts_with('/') && !v.contains(':') && !v.split('/').any(|p| p == "..");
    (v.contains('.') && FONT_EXTS.contains(&ext.as_str()) && inside).then_some(v)
}

/// The size a node asks for inside a stacking container.
pub fn default_size(n: &Node, theme: &Theme) -> (f32, f32) {
    let f = font(n, theme);
    let row = (f * 2.0).round();
    let (w, h) = match n.kind.as_str() {
        "label" => (label_width(&n.text, f) + 8.0, (f * 1.5).round()),
        "button" => ((label_width(&n.text, f) + 40.0).max(140.0), row),
        "toggle" => (text_width(&n.text, f) + row + 8.0, row),
        "slider" => (200.0, row),
        "dropdown" | "input" => (200.0, row),
        "progress" => (200.0, (f * 0.9).round()),
        "image" => (64.0, 64.0),
        "list" => (200.0, row * n.options.len().clamp(1, 6) as f32),
        _ => (200.0, 120.0),
    };
    (if n.size[0] > 0.0 { n.size[0] } else { w }, if n.size[1] > 0.0 { n.size[1] } else { h })
}

/// Where each shown node sits on a screen of `size` pixels, and the area it may draw in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    /// By node index; `None` for hidden nodes.
    pub rects: Vec<Option<Rect>>,
    pub clips: Vec<Rect>,
    /// How far each scroll's content reaches below its top (for its scroll bar).
    pub content: BTreeMap<usize, f32>,
}

impl Layout {
    pub fn rect(&self, i: usize) -> Option<Rect> {
        self.rects.get(i).copied().flatten()
    }
    /// The topmost shown node at `p` whose clip area contains it.
    pub fn hit(&self, screen: &Screen, p: [f32; 2], want: impl Fn(&Node) -> bool) -> Option<usize> {
        (0..screen.nodes.len()).rev().find(|&i| {
            want(&screen.nodes[i])
                && self.rect(i).is_some_and(|r| r.contains(p) && self.clips[i].contains(p))
        })
    }
}

pub fn layout(
    screen: &Screen,
    theme: &Theme,
    size: [f32; 2],
    scroll: &BTreeMap<usize, f32>,
) -> Layout {
    let n = screen.nodes.len();
    let full = Rect::new(0.0, 0.0, size[0], size[1]);
    let mut out = Layout { rects: vec![None; n], clips: vec![full; n], content: BTreeMap::new() };
    place_children(screen, theme, None, full, full, scroll, &mut out);
    out
}

fn place_children(
    screen: &Screen,
    theme: &Theme,
    parent: Option<usize>,
    area: Rect,
    clip: Rect,
    scroll: &BTreeMap<usize, f32>,
    out: &mut Layout,
) {
    let kids: Vec<usize> = screen.children(parent).filter(|&i| screen.nodes[i].visible).collect();
    let kind = parent.map_or("panel", |p| screen.nodes[p].kind.as_str());
    let (pad, gap) = match parent {
        Some(p) => {
            let node = &screen.nodes[p];
            (node.padding.unwrap_or(0.0), node.spacing.unwrap_or(theme.spacing))
        }
        None => (0.0, theme.spacing),
    };
    let inner = area.inset(pad);
    let mut rects: Vec<(usize, Rect)> = Vec::new();
    match kind {
        "vbox" | "scroll" => {
            let shift = if kind == "scroll" {
                parent.and_then(|p| scroll.get(&p)).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            let mut y = inner.y - shift;
            for &k in &kids {
                let (w, h) = default_size(&screen.nodes[k], theme);
                let w = if screen.nodes[k].size[0] > 0.0 { w.min(inner.w) } else { inner.w };
                let x = match screen.nodes[k].align.as_str() {
                    _ if w >= inner.w => inner.x,
                    "left" => inner.x,
                    "right" => inner.x + inner.w - w,
                    _ => inner.x + (inner.w - w) * 0.5,
                };
                rects.push((k, Rect::new(x, y, w, h)));
                y += h + gap;
            }
            if let Some(p) = parent {
                out.content.insert(p, (y - gap + shift - inner.y).max(0.0));
            }
        }
        "hbox" => {
            let mut x = inner.x;
            for &k in &kids {
                let (w, _) = default_size(&screen.nodes[k], theme);
                rects.push((k, Rect::new(x, inner.y, w, inner.h)));
                x += w + gap;
            }
        }
        "grid" => {
            let cols = parent.map_or(2, |p| screen.nodes[p].columns.max(1));
            let cw = ((inner.w - gap * (cols as f32 - 1.0)) / cols as f32).max(0.0);
            let mut y = inner.y;
            for row in kids.chunks(cols) {
                let h = row
                    .iter()
                    .map(|&k| default_size(&screen.nodes[k], theme).1)
                    .fold(0.0, f32::max);
                for (c, &k) in row.iter().enumerate() {
                    rects.push((k, Rect::new(inner.x + c as f32 * (cw + gap), y, cw, h)));
                }
                y += h + gap;
            }
        }
        _ => {
            for &k in &kids {
                let node = &screen.nodes[k];
                let (a, o) = (node.anchor, node.offset);
                let l = inner.x + inner.w * a[0] + o[0];
                let t = inner.y + inner.h * a[1] + o[1];
                let r = inner.x + inner.w * a[2] + o[2];
                let b = inner.y + inner.h * a[3] + o[3];
                rects.push((k, Rect::new(l, t, (r - l).max(0.0), (b - t).max(0.0))));
            }
        }
    }
    let child_clip = if kind == "scroll" { clip.intersect(&area) } else { clip };
    for (k, r) in rects {
        out.rects[k] = Some(r);
        out.clips[k] = child_clip;
        if screen.nodes[k].is_container() {
            place_children(screen, theme, Some(k), r, child_clip, scroll, out);
        }
    }
}

/// What happened to a control, for scripts.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    Clicked(String),
    Changed(String),
}

impl UiEvent {
    /// The script event this fires: `ui_<Name>_clicked` or `ui_<Name>_changed`.
    pub fn script_event(&self) -> String {
        match self {
            UiEvent::Clicked(n) => format!("ui_{n}_clicked"),
            UiEvent::Changed(n) => format!("ui_{n}_changed"),
        }
    }
}

/// A screen while the game runs: live values, focus, hover and press.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Live {
    pub screen: Screen,
    pub theme: Theme,
    pub focus: Option<usize>,
    pub hover: Option<usize>,
    pub pressed: Option<usize>,
    /// The dropdown whose choices are open, and the choice under the pointer or keys.
    pub open: Option<(usize, usize)>,
    pub scroll: BTreeMap<usize, f32>,
    pub events: Vec<UiEvent>,
    pub size: [f32; 2],
    dragging: Option<usize>,
    /// Whether keys or a gamepad moved focus last (so the focus frame shows).
    pub keyboard: bool,
    /// Project folder, so inline pictures and string tables can be read.
    pub project: Option<std::path::PathBuf>,
    /// Glyphs a typewriter has revealed, by control name.
    pub revealed: BTreeMap<String, f32>,
    /// Typewriters that have already reported they finished.
    pub reveal_done: BTreeSet<String>,
    /// Typewriters skipped to the end.
    pub reveal_all: BTreeSet<String>,
    /// String-table keys already reported as missing.
    pub string_warned: BTreeSet<String>,
    /// Warnings from the last draw, for the Console.
    pub warnings: Vec<String>,
}

/// How tall one dropdown choice row is.
fn choice_row(n: &Node, theme: &Theme) -> f32 {
    (font(n, theme) * 1.8).round()
}

impl Live {
    pub fn new(screen: Screen, theme: Theme) -> Self {
        Live { screen, theme, size: [1280.0, 720.0], ..Default::default() }
    }

    pub fn layout(&self) -> Layout {
        layout(&self.screen, &self.theme, self.size, &self.scroll)
    }

    pub fn resize(&mut self, size: [f32; 2]) {
        self.size = size;
    }

    /// Returns whether a skip just finished this control.
    fn apply_typewriter(&mut self, element: &str, skip: bool) -> bool {
        let kind_ok = self
            .screen
            .nodes
            .iter()
            .any(|n| n.name == element && matches!(n.kind.as_str(), "label" | "button"));
        if !kind_ok {
            return false;
        }
        if skip {
            let total = self.glyph_total(element) as f32;
            self.reveal_all.insert(element.to_string());
            self.revealed.insert(element.to_string(), total);
            self.reveal_done.insert(element.to_string())
        } else {
            self.reveal_all.remove(element);
            self.reveal_done.remove(element);
            self.revealed.insert(element.to_string(), 0.0);
            false
        }
    }

    fn glyph_total(&self, element: &str) -> usize {
        let Some(n) = self.screen.nodes.iter().find(|n| n.name == element) else {
            return 0;
        };
        let text = resolved_label(self, &n.text);
        let size = font(n, &self.theme);
        crate::text_sprites::layout_glyphs(&text, size).len()
    }

    fn step_typewriters(&mut self, dt: f32) -> Vec<String> {
        self.note_missing_strings();
        let jobs: Vec<(String, f32, usize)> = self
            .screen
            .nodes
            .iter()
            .filter(|n| n.typewriter > 0.0 && matches!(n.kind.as_str(), "label" | "button"))
            .map(|n| {
                let text = resolved_label(self, &n.text);
                let total = crate::text_sprites::layout_glyphs(&text, font(n, &self.theme)).len();
                (n.name.clone(), n.typewriter, total)
            })
            .collect();
        let mut finished = Vec::new();
        for (name, cps, total) in jobs {
            if self.reveal_all.contains(&name) {
                continue;
            }
            let shown = self.revealed.entry(name.clone()).or_insert(0.0);
            let (next, just) = crate::text_sprites::step_typewriter(*shown, cps, dt, total);
            *shown = next;
            if just && self.reveal_done.insert(name.clone()) {
                finished.push(name);
            }
        }
        finished
    }

    fn note_missing_strings(&mut self) {
        let Some(root) = self.project.clone() else { return };
        if self.screen.strings.is_empty() {
            return;
        }
        let table = crate::text_sprites::load_table_cached(&root.join(&self.screen.strings)).ok();
        let keys: Vec<crate::text_sprites::LabelKey> = self
            .screen
            .nodes
            .iter()
            .filter(|n| matches!(n.kind.as_str(), "label" | "button"))
            .filter_map(|n| crate::text_sprites::label_key(&n.text))
            .collect();
        for key in keys {
            let missing = match &table {
                Some(t) => crate::text_sprites::lookup(t, &key.key, &key.values).missing,
                None => true,
            };
            if missing && self.string_warned.insert(key.key.clone()) {
                self.warnings.push(crate::text_sprites::missing_key_warning(&key.key));
            }
        }
    }

    fn usable(&self, i: usize) -> bool {
        let n = &self.screen.nodes[i];
        n.focusable() && self.screen.shown(i)
    }

    /// Focusable shown controls in reading order (file order).
    pub fn focus_order(&self) -> Vec<usize> {
        let lay = self.layout();
        (0..self.screen.nodes.len()).filter(|&i| self.usable(i) && lay.rect(i).is_some()).collect()
    }

    fn open_choices(&self, lay: &Layout) -> Option<(usize, Vec<Rect>)> {
        let (d, _) = self.open?;
        let r = lay.rect(d)?;
        let n = &self.screen.nodes[d];
        let h = choice_row(n, &self.theme);
        Some((
            d,
            (0..n.options.len())
                .map(|k| Rect::new(r.x, r.y + r.h + k as f32 * h, r.w, h))
                .collect(),
        ))
    }

    pub fn pointer_move(&mut self, p: [f32; 2]) {
        let lay = self.layout();
        if let Some((d, rows)) = self.open_choices(&lay) {
            if let Some(k) = rows.iter().position(|r| r.contains(p)) {
                self.open = Some((d, k));
            }
        }
        self.hover = lay.hit(&self.screen, p, |n| n.focusable());
        if let Some(s) = self.dragging {
            self.slide_to(s, p, &lay);
        }
    }

    fn slide_to(&mut self, s: usize, p: [f32; 2], lay: &Layout) {
        let Some(r) = lay.rect(s) else { return };
        let knob = r.h * 0.5;
        let t = ((p[0] - r.x - knob) / (r.w - 2.0 * knob).max(1.0)).clamp(0.0, 1.0);
        let n = &mut self.screen.nodes[s];
        let before = n.value;
        n.set_value(n.min + t * (n.max - n.min));
        if n.value != before {
            let name = n.name.clone();
            self.events.push(UiEvent::Changed(name));
        }
    }

    pub fn pointer_down(&mut self, p: [f32; 2]) {
        self.keyboard = false;
        let lay = self.layout();
        if let Some((d, rows)) = self.open_choices(&lay) {
            if let Some(k) = rows.iter().position(|r| r.contains(p)) {
                self.pick(d, k);
            }
            self.open = None;
            return;
        }
        let hit = lay.hit(&self.screen, p, |n| n.focusable());
        self.focus = hit;
        self.pressed = hit;
        let Some(i) = hit else { return };
        match self.screen.nodes[i].kind.as_str() {
            "slider" => {
                self.dragging = Some(i);
                self.slide_to(i, p, &lay);
            }
            "list" => {
                let r = lay.rect(i).unwrap();
                let rows = self.screen.nodes[i].options.len().max(1);
                let row_h = r.h / rows.clamp(1, 6) as f32;
                let k = ((p[1] - r.y) / row_h).floor() as usize;
                if k < self.screen.nodes[i].options.len() {
                    self.pick(i, k);
                }
            }
            _ => {}
        }
    }

    pub fn pointer_up(&mut self, p: [f32; 2]) {
        self.dragging = None;
        let Some(i) = self.pressed.take() else { return };
        let lay = self.layout();
        if lay.rect(i).is_some_and(|r| r.contains(p)) {
            self.activate(i);
        }
    }

    /// Scroll the scroll container under `p` by `dy` pixels.
    pub fn wheel(&mut self, p: [f32; 2], dy: f32) {
        let lay = self.layout();
        let Some(s) = lay.hit(&self.screen, p, |n| n.kind == "scroll") else { return };
        let r = lay.rect(s).unwrap();
        let reach = lay.content.get(&s).copied().unwrap_or(0.0);
        let most = (reach - r.h + self.screen.nodes[s].padding.unwrap_or(0.0) * 2.0).max(0.0);
        let now = self.scroll.get(&s).copied().unwrap_or(0.0);
        self.scroll.insert(s, (now + dy).clamp(0.0, most));
    }

    fn pick(&mut self, i: usize, k: usize) {
        let n = &mut self.screen.nodes[i];
        if n.selected != k {
            n.selected = k;
            let name = n.name.clone();
            self.events.push(UiEvent::Changed(name));
        }
    }

    /// Press a control: click a button, flip a toggle, open or close a dropdown.
    pub fn activate(&mut self, i: usize) {
        if !self.usable(i) {
            return;
        }
        let name = self.screen.nodes[i].name.clone();
        match self.screen.nodes[i].kind.as_str() {
            "button" => self.events.push(UiEvent::Clicked(name)),
            "toggle" => {
                let n = &mut self.screen.nodes[i];
                n.checked = !n.checked;
                self.events.push(UiEvent::Changed(name));
            }
            "dropdown" => {
                self.open = match self.open {
                    Some((d, k)) if d == i => {
                        self.pick(i, k);
                        None
                    }
                    _ => Some((i, self.screen.nodes[i].selected)),
                };
            }
            _ => {}
        }
    }

    /// A key or gamepad button by name. Returns whether the UI used it.
    pub fn key(&mut self, key: &str) -> bool {
        // Letters and Space typed into a focused text field arrive as text, not as moves.
        let typing = self.focus.is_some_and(|f| self.screen.nodes[f].kind == "input");
        if typing && (key.chars().count() == 1 || key == "Space" || key == "SpaceBar") {
            return true;
        }
        let key = match key {
            "DPadUp" | "GamepadUp" | "W" => "Up",
            "DPadDown" | "GamepadDown" | "S" => "Down",
            "DPadLeft" | "GamepadLeft" | "A" => "Left",
            "DPadRight" | "GamepadRight" | "D" => "Right",
            "GamepadA" | "GamepadSouth" | "SpaceBar" | "Space" => "Enter",
            "GamepadB" | "GamepadEast" => "Escape",
            "Tab" => "Tab",
            other => other,
        };
        // Typing keys go to a focused text field first.
        if typing && key == "Backspace" {
            let f = self.focus.unwrap();
            let n = &mut self.screen.nodes[f];
            if n.text.pop().is_some() {
                let name = n.name.clone();
                self.events.push(UiEvent::Changed(name));
            }
            return true;
        }
        let order = self.focus_order();
        if order.is_empty() {
            return false;
        }
        self.keyboard = true;
        if let Some((d, k)) = self.open {
            let count = self.screen.nodes[d].options.len();
            match key {
                "Up" => self.open = Some((d, k.saturating_sub(1))),
                "Down" => self.open = Some((d, (k + 1).min(count.saturating_sub(1)))),
                "Enter" => self.activate(d),
                "Escape" => self.open = None,
                _ => return false,
            }
            return true;
        }
        let Some(f) = self.focus.filter(|f| order.contains(f)) else {
            // Nothing focused yet: any navigation key lands on the first control.
            if matches!(key, "Up" | "Down" | "Left" | "Right" | "Tab" | "Shift+Tab" | "Enter") {
                self.focus = order.first().copied();
                return true;
            }
            return false;
        };
        let kind = self.screen.nodes[f].kind.clone();
        match (kind.as_str(), key) {
            ("slider", "Left" | "Right") => {
                let n = &mut self.screen.nodes[f];
                let step = if n.step > 0.0 { n.step } else { (n.max - n.min) / 10.0 };
                let before = n.value;
                n.set_value(n.value + if key == "Left" { -step } else { step });
                if n.value != before {
                    let name = n.name.clone();
                    self.events.push(UiEvent::Changed(name));
                }
                true
            }
            ("list", "Up" | "Down") => {
                let n = &self.screen.nodes[f];
                let k = n.selected;
                let next = if key == "Up" {
                    k.checked_sub(1)
                } else {
                    Some(k + 1).filter(|&j| j < n.options.len())
                };
                match next {
                    Some(j) => self.pick(f, j),
                    None => self.move_focus(f, key),
                }
                true
            }
            (_, "Tab") => {
                let at = order.iter().position(|&o| o == f).unwrap();
                self.focus = Some(order[(at + 1) % order.len()]);
                true
            }
            (_, "Shift+Tab") => {
                let at = order.iter().position(|&o| o == f).unwrap();
                self.focus = Some(order[(at + order.len() - 1) % order.len()]);
                true
            }
            (_, "Up" | "Down" | "Left" | "Right") => {
                self.move_focus(f, key);
                true
            }
            (_, "Enter") => {
                self.activate(f);
                true
            }
            (_, "Escape") => {
                self.focus = None;
                true
            }
            _ => false,
        }
    }

    /// Move focus to the nearest control in a direction; stays put when there is none.
    fn move_focus(&mut self, from: usize, dir: &str) {
        let lay = self.layout();
        let Some(r) = lay.rect(from) else { return };
        let c = r.center();
        let (dx, dy) = match dir {
            "Up" => (0.0, -1.0),
            "Down" => (0.0, 1.0),
            "Left" => (-1.0, 0.0),
            _ => (1.0, 0.0),
        };
        let best = self
            .focus_order()
            .into_iter()
            .filter(|&i| i != from)
            .filter_map(|i| {
                let o = lay.rect(i)?.center();
                let (vx, vy) = (o[0] - c[0], o[1] - c[1]);
                let along = vx * dx + vy * dy;
                if along <= 0.5 {
                    return None;
                }
                let side = (vx * dy - vy * dx).abs();
                Some((i, along + side * 2.0))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = best {
            self.focus = Some(i);
        }
    }

    /// Typed text for a focused text field.
    pub fn type_text(&mut self, text: &str) {
        let Some(f) = self.focus else { return };
        let n = &mut self.screen.nodes[f];
        if n.kind != "input" || !n.enabled {
            return;
        }
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        if clean.is_empty() {
            return;
        }
        n.text.push_str(&clean);
        let name = n.name.clone();
        self.events.push(UiEvent::Changed(name));
    }

    /// A control's value for scripts: slider and progress values, 1 or 0 for
    /// toggles, the choice number for dropdowns and lists.
    pub fn value_of(&self, name: &str) -> Option<f64> {
        let n = &self.screen.nodes[self.screen.find(name)?];
        Some(match n.kind.as_str() {
            "toggle" => f64::from(u8::from(n.checked)),
            "dropdown" | "list" => n.selected as f64,
            _ => f64::from(n.value),
        })
    }

    /// A control's text for scripts: an input's text, a dropdown's or list's choice, or a label.
    pub fn text_of(&self, name: &str) -> Option<String> {
        let n = &self.screen.nodes[self.screen.find(name)?];
        Some(match n.kind.as_str() {
            "dropdown" | "list" => n.options.get(n.selected).cloned().unwrap_or_default(),
            _ => n.text.clone(),
        })
    }
}

/// Something to paint, in screen pixels, clipped to `clip`.
#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    Fill {
        rect: Rect,
        color: [f32; 4],
        corner: f32,
        clip: Rect,
    },
    Frame {
        rect: Rect,
        color: [f32; 4],
        width: f32,
        corner: f32,
        clip: Rect,
    },
    /// `font` is a project font file, or empty for the built-in font.
    Text {
        rect: Rect,
        text: String,
        size: f32,
        font: String,
        color: [f32; 4],
        align: String,
        clip: Rect,
    },
    Picture {
        rect: Rect,
        path: String,
        tint: [f32; 4],
        /// Texture rectangle inside the picture, 0 to 1. The whole picture is 0 0 1 1.
        crop: [f32; 4],
        clip: Rect,
        slice: [f32; 4],
        slice_mode: String,
    },
}

fn mul(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

fn resolved_label(live: &Live, raw: &str) -> String {
    let Some(key) = crate::text_sprites::label_key(raw) else {
        return raw.to_string();
    };
    let Some(root) = live.project.as_deref() else {
        return key.key;
    };
    if live.screen.strings.is_empty() {
        return key.key;
    }
    let Ok(table) = crate::text_sprites::load_table_cached(&root.join(&live.screen.strings)) else {
        return key.key;
    };
    crate::text_sprites::lookup(&table, &key.key, &key.values).text
}

fn rich_text(n: &Node, text: &str) -> bool {
    text.contains("[img=") || n.typewriter > 0.0 || n.wave != [0.0, 0.0] || n.shake != [0.0, 0.0]
}

struct Caption<'a> {
    live: &'a Live,
    out: &'a mut Vec<Draw>,
    size: f32,
    font: &'a str,
    color: [f32; 4],
    clip: Rect,
    time: f32,
}

/// Draw a label or button caption. Plain text stays one text draw.
fn emit_text(paint: &mut Caption<'_>, n: &Node, rect: Rect, raw: &str, align: &str) {
    let text = resolved_label(paint.live, raw);
    if !rich_text(n, &text) {
        paint.out.push(Draw::Text {
            rect,
            text,
            size: paint.size,
            font: paint.font.to_string(),
            color: paint.color,
            align: align.to_string(),
            clip: paint.clip,
        });
        return;
    }
    let glyphs = crate::text_sprites::layout_glyphs(&text, paint.size);
    let shown = if n.typewriter <= 0.0 || !paint.live.revealed.contains_key(&n.name) {
        glyphs.len()
    } else {
        crate::text_sprites::visible_glyphs(&glyphs, paint.live.revealed[&n.name])
    };
    let total = glyphs
        .iter()
        .map(|g| match g {
            crate::text_sprites::Glyph::Char { w, .. }
            | crate::text_sprites::Glyph::Sprite { w, .. } => *w,
        })
        .sum::<f32>();
    let start = match align {
        "center" => rect.x + (rect.w - total) * 0.5,
        "right" => rect.x + rect.w - total,
        _ => rect.x,
    };
    for (i, g) in glyphs.iter().take(shown).enumerate() {
        let shift = crate::text_sprites::glyph_shift(
            i, paint.time, n.wave[0], n.wave[1], n.shake[0], n.shake[1],
        );
        match g {
            crate::text_sprites::Glyph::Char { ch, x, w } => {
                paint.out.push(Draw::Text {
                    rect: Rect::new(start + x + shift[0], rect.y + shift[1], *w, rect.h),
                    text: ch.to_string(),
                    size: paint.size,
                    font: paint.font.to_string(),
                    color: paint.color,
                    align: "left".into(),
                    clip: paint.clip,
                });
            }
            crate::text_sprites::Glyph::Sprite { path, clip: anim, x, w, .. } => {
                let (image, crop) = crate::text_sprites::inline_image(
                    paint.live.project.as_deref(),
                    path,
                    anim,
                    paint.time,
                )
                .map(|(p, uv)| (p.to_string_lossy().into_owned(), uv))
                .unwrap_or_else(|| (path.clone(), [0.0, 0.0, 1.0, 1.0]));
                let side = *w;
                let y = rect.y + (rect.h - side) * 0.5 + shift[1];
                paint.out.push(Draw::Picture {
                    rect: Rect::new(start + x + shift[0], y, side, side),
                    path: image,
                    tint: [1.0, 1.0, 1.0, 1.0],
                    crop,
                    clip: paint.clip,
                    slice: [0.0; 4],
                    slice_mode: String::new(),
                });
            }
        }
    }
}

/// Paint list for a live screen; `time` blinks the text cursor.
pub fn draw(live: &Live, time: f32) -> Vec<Draw> {
    let lay = live.layout();
    let theme = &live.theme;
    let s = &live.screen;
    let mut out = Vec::new();
    let mut overlay = Vec::new();
    for (i, n) in s.nodes.iter().enumerate() {
        if !s.shown(i) {
            continue;
        }
        let Some(r) = lay.rect(i) else { continue };
        let clip = lay.clips[i];
        let f = font(n, theme);
        let ff = font_file(n, theme).to_string();
        let text_color = if n.enabled { theme.text } else { theme.dim_text };
        let corner = theme.corner;
        let fill = |out: &mut Vec<Draw>, rect: Rect, color: [f32; 4]| {
            out.push(Draw::Fill { rect, color, corner, clip })
        };
        let text = |out: &mut Vec<Draw>, rect: Rect, t: &str, color: [f32; 4], align: &str| {
            out.push(Draw::Text {
                rect,
                text: t.to_string(),
                size: f,
                font: ff.clone(),
                color,
                align: align.to_string(),
                clip,
            })
        };
        let hot = live.hover == Some(i) && n.enabled;
        let down = live.pressed == Some(i) && n.enabled;
        match n.kind.as_str() {
            "panel" => fill(&mut out, r, n.color.unwrap_or(theme.panel)),
            "vbox" | "hbox" | "grid" => {
                if let Some(c) = n.color {
                    fill(&mut out, r, c);
                }
            }
            "scroll" => {
                fill(&mut out, r, n.color.unwrap_or(theme.field));
                let reach = lay.content.get(&i).copied().unwrap_or(0.0);
                if reach > r.h {
                    let at = live.scroll.get(&i).copied().unwrap_or(0.0);
                    let bar_h = (r.h * r.h / reach).max(16.0);
                    let y = r.y + (r.h - bar_h) * (at / (reach - r.h)).clamp(0.0, 1.0);
                    fill(&mut out, Rect::new(r.x + r.w - 6.0, y, 4.0, bar_h), theme.dim_text);
                }
            }
            "button" => {
                let c = if !n.enabled {
                    mul(theme.button, 0.5)
                } else if down {
                    theme.button_press
                } else if hot {
                    theme.button_hover
                } else {
                    theme.button
                };
                fill(&mut out, r, c);
                emit_text(
                    &mut Caption {
                        live,
                        out: &mut out,
                        size: f,
                        font: &ff,
                        color: text_color,
                        clip,
                        time,
                    },
                    n,
                    r,
                    &n.text,
                    "center",
                );
            }
            "label" => emit_text(
                &mut Caption {
                    live,
                    out: &mut out,
                    size: f,
                    font: &ff,
                    color: n.color.unwrap_or(theme.text),
                    clip,
                    time,
                },
                n,
                r,
                &n.text,
                &n.align,
            ),
            "image" => out.push(Draw::Picture {
                rect: r,
                path: n.image.clone(),
                tint: n.color.unwrap_or([1.0; 4]),
                crop: [0.0, 0.0, 1.0, 1.0],
                clip,
                slice: n.slice,
                slice_mode: n.slice_mode.clone(),
            }),
            "slider" => {
                let knob = r.h * 0.5;
                let track =
                    Rect::new(r.x + knob, r.y + r.h * 0.5 - 3.0, (r.w - 2.0 * knob).max(0.0), 6.0);
                fill(&mut out, track, theme.field);
                let filled = Rect::new(track.x, track.y, track.w * n.fraction(), track.h);
                fill(&mut out, filled, if n.enabled { theme.accent } else { theme.dim_text });
                let kx = track.x + track.w * n.fraction();
                let k = Rect::new(
                    kx - knob * 0.7,
                    r.y + r.h * 0.5 - knob * 0.7,
                    knob * 1.4,
                    knob * 1.4,
                );
                fill(&mut out, k, if hot || down { theme.button_hover } else { theme.text });
            }
            "toggle" => {
                let side = (r.h * 0.7).round();
                let b = Rect::new(r.x, r.y + (r.h - side) * 0.5, side, side);
                fill(&mut out, b, theme.field);
                out.push(Draw::Frame {
                    rect: b,
                    color: if hot { theme.accent } else { theme.border },
                    width: 1.0,
                    corner,
                    clip,
                });
                if n.checked {
                    fill(&mut out, b.inset(side * 0.22), theme.accent);
                }
                text(
                    &mut out,
                    Rect::new(r.x + side + 8.0, r.y, (r.w - side - 8.0).max(0.0), r.h),
                    &n.text,
                    text_color,
                    "left",
                );
            }
            "dropdown" => {
                fill(&mut out, r, if hot { mul(theme.field, 1.6) } else { theme.field });
                out.push(Draw::Frame { rect: r, color: theme.border, width: 1.0, corner, clip });
                let shown = n.options.get(n.selected).cloned().unwrap_or_default();
                text(
                    &mut out,
                    Rect::new(r.x + 10.0, r.y, (r.w - 34.0).max(0.0), r.h),
                    &shown,
                    text_color,
                    "left",
                );
                text(
                    &mut out,
                    Rect::new(r.x + r.w - 26.0, r.y, 20.0, r.h),
                    "▼",
                    theme.dim_text,
                    "center",
                );
                if let Some((d, k)) = live.open.filter(|(d, _)| *d == i) {
                    let h = choice_row(n, theme);
                    let full = Rect::new(r.x, r.y + r.h, r.w, h * n.options.len() as f32);
                    let screen = Rect::new(0.0, 0.0, live.size[0], live.size[1]);
                    overlay.push(Draw::Fill {
                        rect: full,
                        color: theme.panel,
                        corner,
                        clip: screen,
                    });
                    for (j, o) in n.options.iter().enumerate() {
                        let row = Rect::new(full.x, full.y + j as f32 * h, full.w, h);
                        if j == k {
                            overlay.push(Draw::Fill {
                                rect: row,
                                color: theme.accent,
                                corner: 0.0,
                                clip: screen,
                            });
                        }
                        overlay.push(Draw::Text {
                            rect: Rect::new(row.x + 10.0, row.y, (row.w - 20.0).max(0.0), h),
                            text: o.clone(),
                            size: f,
                            font: ff.clone(),
                            color: theme.text,
                            align: "left".into(),
                            clip: screen,
                        });
                    }
                    let _ = d;
                    overlay.push(Draw::Frame {
                        rect: full,
                        color: theme.border,
                        width: 1.0,
                        corner,
                        clip: screen,
                    });
                }
            }
            "input" => {
                fill(&mut out, r, theme.field);
                let focused = live.focus == Some(i);
                out.push(Draw::Frame {
                    rect: r,
                    color: if focused { theme.accent } else { theme.border },
                    width: 1.0,
                    corner,
                    clip,
                });
                let inner = Rect::new(r.x + 10.0, r.y, (r.w - 20.0).max(0.0), r.h);
                if n.text.is_empty() && !focused {
                    text(&mut out, inner, &n.placeholder, theme.dim_text, "left");
                } else {
                    let caret = focused && (time * 2.0).fract() < 0.5;
                    let shown = if caret { format!("{}|", n.text) } else { n.text.clone() };
                    text(&mut out, inner, &shown, text_color, "left");
                }
            }
            "list" => {
                fill(&mut out, r, theme.field);
                let rows = n.options.len().clamp(1, 6);
                let h = r.h / rows as f32;
                for (j, o) in n.options.iter().enumerate().take(rows) {
                    let row = Rect::new(r.x, r.y + j as f32 * h, r.w, h);
                    if j == n.selected {
                        out.push(Draw::Fill { rect: row, color: theme.accent, corner: 0.0, clip });
                    }
                    text(
                        &mut out,
                        Rect::new(row.x + 10.0, row.y, (row.w - 20.0).max(0.0), h),
                        o,
                        text_color,
                        "left",
                    );
                }
            }
            "progress" => {
                fill(&mut out, r, theme.field);
                let w = r.w * n.fraction();
                if w > 0.0 {
                    fill(&mut out, Rect::new(r.x, r.y, w, r.h), n.color.unwrap_or(theme.accent));
                }
            }
            _ => {}
        }
        if live.focus == Some(i) && live.keyboard {
            out.push(Draw::Frame {
                rect: r.inset(-3.0),
                color: theme.focus,
                width: 2.0,
                corner: corner + 2.0,
                clip,
            });
        }
    }
    out.extend(overlay);
    out
}

/// Read a screen file and its theme (relative to `project`).
pub fn load(project: &std::path::Path, path: &str) -> Result<Live, String> {
    let text =
        std::fs::read_to_string(project.join(path)).map_err(|_| format!("cannot read {path}"))?;
    let screen = Screen::from_text(&text).map_err(|e| format!("{path}: {e}"))?;
    let theme = if screen.theme.is_empty() {
        Theme::default()
    } else {
        let t = std::fs::read_to_string(project.join(&screen.theme))
            .map_err(|_| format!("cannot read {}", screen.theme))?;
        Theme::from_text(&t).map_err(|e| format!("{}: {e}", screen.theme))?
    };
    let mut live = Live::new(screen, theme);
    live.project = Some(project.to_path_buf());
    Ok(live)
}

/// The screen file a script's widget name means: the name itself when it is a
/// `.sealui` path, else `ui/<name>.sealui`.
pub fn widget_path(widget: &str) -> String {
    if widget.to_ascii_lowercase().ends_with(&format!(".{SCREEN_EXT}")) {
        widget.replace('\\', "/")
    } else {
        format!("ui/{widget}.{SCREEN_EXT}")
    }
}

/// The screens a running game shows, front-most last, plus the shown widgets
/// that have no screen file (those fall back to plain text lines).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Screens {
    pub shown: Vec<(String, Live)>,
    pub missing: BTreeSet<String>,
    /// Screen files that could not be read, as messages for the log.
    pub errors: Vec<String>,
    /// Script texts already copied into the screens, so typed input is not overwritten.
    applied: BTreeMap<String, String>,
    /// Typewriters a script skipped, waiting to report they finished.
    pending_finished: Vec<String>,
    pub size: [f32; 2],
    /// Generation of the built-in dialog currently shown, so a rebuild does not
    /// wipe a click that has not been read yet.
    pub dialog_gen: u64,
}

impl Screens {
    /// Match the shown screens to the widgets scripts want shown, and copy
    /// script texts into the controls with those names.
    pub fn sync(
        &mut self,
        project: Option<&std::path::Path>,
        wanted: &BTreeSet<String>,
        texts: &BTreeMap<String, String>,
    ) {
        self.shown.retain(|(w, _)| wanted.contains(w));
        self.missing.retain(|w| wanted.contains(w));
        for w in wanted {
            // The built-in dialog is installed by the play step, not loaded from a file.
            if w == "__dialog" {
                continue;
            }
            if self.shown.iter().any(|(s, _)| s == w) || self.missing.contains(w) {
                continue;
            }
            let path = widget_path(w);
            let found = project.filter(|p| p.join(&path).is_file());
            match found.map(|p| load(p, &path)) {
                Some(Ok(mut live)) => {
                    if self.size[0] > 0.0 {
                        live.resize(self.size);
                    }
                    if live.focus.is_none() {
                        live.focus = live.focus_order().first().copied();
                        live.keyboard = live.focus.is_some();
                    }
                    self.shown.push((w.clone(), live));
                    // A newly shown screen picks up every text scripts already set.
                    self.applied.clear();
                }
                Some(Err(e)) => {
                    self.errors.push(e);
                    self.missing.insert(w.clone());
                }
                None => {
                    self.missing.insert(w.clone());
                }
            }
        }
        for (element, text) in texts {
            if self.applied.get(element) == Some(text) {
                continue;
            }
            for (_, live) in &mut self.shown {
                if let Some(i) = live.screen.find(element) {
                    let node = &mut live.screen.nodes[i];
                    if node.kind == "image" {
                        let path = text.replace('\\', "/");
                        if !path.is_empty() {
                            node.image = path;
                        }
                    } else {
                        node.text = text.clone();
                        live.revealed.remove(element);
                        live.reveal_done.remove(element);
                        live.reveal_all.remove(element);
                    }
                }
            }
            self.applied.insert(element.clone(), text.clone());
        }
    }

    /// Hide every screen, keeping the view size.
    pub fn clear(&mut self) {
        *self = Screens { size: self.size, ..Default::default() };
    }

    pub fn active(&self) -> bool {
        !self.shown.is_empty()
    }

    /// Whether some shown widget has no screen file and draws as text lines.
    pub fn uses_lines(&self) -> bool {
        !self.missing.is_empty()
    }

    pub fn resize(&mut self, size: [f32; 2]) {
        self.size = size;
        for (_, live) in &mut self.shown {
            live.resize(size);
        }
    }

    /// Restart a typewriter (`skip` false) or jump it to the end (`skip` true).
    /// A skip that had not already finished is reported by the next [`advance`].
    pub fn typewriter(&mut self, element: &str, skip: bool) {
        for (_, live) in &mut self.shown {
            if live.apply_typewriter(element, skip) {
                self.pending_finished.push(element.to_string());
            }
        }
    }

    /// Reveal typewriter characters and note missing string-table keys.
    /// Returns the control names whose typewriter finished this step.
    pub fn advance(&mut self, dt: f32) -> Vec<String> {
        let mut finished = std::mem::take(&mut self.pending_finished);
        for (_, live) in &mut self.shown {
            finished.extend(live.step_typewriters(dt));
        }
        finished
    }

    /// Console lines gathered while resolving string tables.
    pub fn take_warnings(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        for (_, live) in &mut self.shown {
            out.append(&mut live.warnings);
        }
        out
    }

    /// The front-most screen with a control under `p`, else the front-most screen.
    fn under(&mut self, p: [f32; 2]) -> Option<&mut Live> {
        let at = self.shown.iter().rposition(|(_, l)| {
            l.open.is_some() || l.layout().hit(&l.screen, p, |_| true).is_some()
        });
        let i = at.or_else(|| self.shown.len().checked_sub(1))?;
        Some(&mut self.shown[i].1)
    }

    pub fn pointer_move(&mut self, p: [f32; 2]) {
        for (_, live) in &mut self.shown {
            live.pointer_move(p);
        }
    }

    pub fn pointer_down(&mut self, p: [f32; 2]) {
        if let Some(live) = self.under(p) {
            live.pointer_down(p);
        }
    }

    pub fn pointer_up(&mut self, p: [f32; 2]) {
        for (_, live) in &mut self.shown {
            live.pointer_up(p);
        }
    }

    pub fn wheel(&mut self, p: [f32; 2], dy: f32) {
        if let Some(live) = self.under(p) {
            live.wheel(p, dy);
        }
    }

    /// A key for the front-most screen. Returns whether a control used it.
    pub fn key(&mut self, key: &str) -> bool {
        self.shown.last_mut().is_some_and(|(_, live)| live.key(key))
    }

    pub fn type_text(&mut self, text: &str) {
        if let Some((_, live)) = self.shown.last_mut() {
            live.type_text(text);
        }
    }

    /// Whether a text field has focus, so typed keys belong to it.
    pub fn typing(&self) -> bool {
        self.shown
            .last()
            .is_some_and(|(_, l)| l.focus.is_some_and(|i| l.screen.nodes[i].kind == "input"))
    }

    pub fn take_events(&mut self) -> Vec<UiEvent> {
        self.shown.iter_mut().flat_map(|(_, l)| std::mem::take(&mut l.events)).collect()
    }

    pub fn value_of(&self, name: &str) -> Option<f64> {
        self.shown.iter().rev().find_map(|(_, l)| l.value_of(name))
    }

    pub fn text_of(&self, name: &str) -> Option<String> {
        self.shown.iter().rev().find_map(|(_, l)| l.text_of(name))
    }

    /// Every control's value and text by name, for scripts to read.
    pub fn values(&self) -> BTreeMap<String, (f64, String)> {
        let mut out = BTreeMap::new();
        for (_, l) in &self.shown {
            for n in &l.screen.nodes {
                if let (Some(v), Some(t)) = (l.value_of(&n.name), l.text_of(&n.name)) {
                    out.insert(n.name.clone(), (v, t));
                }
            }
        }
        out
    }

    pub fn draws(&self, time: f32) -> Vec<Draw> {
        self.shown.iter().flat_map(|(_, l)| draw(l, time)).collect()
    }

    /// Draws for one shown screen, laid out at `size`. Empty when that screen is not open.
    pub fn draws_named(&self, name: &str, size: [f32; 2], time: f32) -> Vec<Draw> {
        let want = name.trim();
        if want.is_empty() {
            return Vec::new();
        }
        let path = widget_path(want);
        self.shown
            .iter()
            .filter(|(n, _)| n == want || n == &path || widget_path(n) == path)
            .flat_map(|(_, live)| {
                let mut live = live.clone();
                if size[0] > 1.0 && size[1] > 1.0 {
                    live.resize(size);
                }
                draw(&live, time)
            })
            .collect()
    }
}

/// The same paint list drawn `k` times bigger (or smaller), as the designer's preview needs.
pub fn scale_draws(draws: &[Draw], k: f32) -> Vec<Draw> {
    let r = |r: &Rect| Rect::new(r.x * k, r.y * k, r.w * k, r.h * k);
    draws
        .iter()
        .map(|d| match d {
            Draw::Fill { rect, color, corner, clip } => {
                Draw::Fill { rect: r(rect), color: *color, corner: corner * k, clip: r(clip) }
            }
            Draw::Frame { rect, color, width, corner, clip } => Draw::Frame {
                rect: r(rect),
                color: *color,
                width: (width * k).max(1.0),
                corner: corner * k,
                clip: r(clip),
            },
            Draw::Text { rect, text, size, font, color, align, clip } => Draw::Text {
                rect: r(rect),
                text: text.clone(),
                size: (size * k).max(4.0),
                font: font.clone(),
                color: *color,
                align: align.clone(),
                clip: r(clip),
            },
            Draw::Picture { rect, path, tint, crop, clip, slice, slice_mode } => Draw::Picture {
                rect: r(rect),
                path: path.clone(),
                tint: *tint,
                crop: *crop,
                clip: r(clip),
                slice: *slice,
                slice_mode: slice_mode.clone(),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn script_text_replaces_an_image_and_a_new_screen_focuses_its_first_button() {
        let dir = std::env::temp_dir().join(format!("seal-ui-hearts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ui")).unwrap();
        std::fs::write(
            dir.join("ui/Hud.sealui"),
            "seal ui 1\npanel Hud\n  image H1\n    picture sprites/heart.png\n  button Resume\n    text Resume\n",
        )
        .unwrap();
        let mut screens = Screens::default();
        let shown = BTreeSet::from(["Hud".into()]);
        let texts = BTreeMap::from([("H1".into(), "sprites/heart_empty.png".into())]);
        screens.sync(Some(&dir), &shown, &texts);
        let live = &screens.shown[0].1;
        let image = live.screen.find("H1").unwrap();
        assert_eq!(live.screen.nodes[image].image, "sprites/heart_empty.png");
        let focus = live.focus.expect("first button");
        assert_eq!(live.screen.nodes[focus].name, "Resume");
        assert!(live.keyboard);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn text_effects_and_the_string_table_round_trip() {
        let mut screen = Screen { strings: "ui/lines.strings".into(), ..Screen::default() };
        let mut label = Node::new("label", "Coin");
        label.text = "@coin count=3".into();
        label.typewriter = 20.0;
        label.wave = [4.0, 2.0];
        label.shake = [1.0, 3.0];
        screen.nodes.push(label);
        let back = Screen::from_text(&screen.to_text()).unwrap();
        assert_eq!(back.strings, "ui/lines.strings");
        assert_eq!(back.nodes[0].text, "@coin count=3");
        assert_eq!(back.nodes[0].typewriter, 20.0);
        assert_eq!(back.nodes[0].wave, [4.0, 2.0]);
        assert_eq!(back.nodes[0].shake, [1.0, 3.0]);
        let plain = Screen::from_text("seal ui 1\nlabel Title\n  text Hello\n").unwrap();
        assert!(plain.strings.is_empty());
        assert_eq!(plain.nodes[0].typewriter, 0.0);
    }

    #[test]
    fn a_typewriter_finishes_once_and_a_missing_key_warns_once() {
        let dir = std::env::temp_dir().join(format!("seal-ui-type-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ui")).unwrap();
        std::fs::write(dir.join("ui/lines.strings"), "# Seal string table\ncoin = \"x{count}\"\n")
            .unwrap();
        let mut screen = Screen { strings: "ui/lines.strings".into(), ..Screen::default() };
        let mut label = Node::new("label", "Coin");
        label.text = "@coin count=3".into();
        label.typewriter = 1000.0;
        screen.nodes.push(label);
        let mut missing = Node::new("label", "Gone");
        missing.text = "@missing".into();
        screen.nodes.push(missing);
        let mut live = Live::new(screen, Theme::default());
        live.project = Some(dir.clone());
        let mut screens = Screens::default();
        screens.shown.push(("hud".into(), live));
        let finished = screens.advance(0.01);
        assert!(finished.iter().any(|n| n == "Coin"), "{finished:?}");
        let warnings = screens.take_warnings();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].starts_with("warning: "));
        assert!(screens.advance(0.01).is_empty());
        assert!(screens.take_warnings().is_empty());
        let draws = screens.draws(0.0);
        assert!(draws.iter().any(
            |d| matches!(d, Draw::Text { text, .. } if text == "x3" || text.chars().count() == 1)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_inline_picture_is_a_picture_and_a_broken_tag_stays_text() {
        let mut screen = Screen::default();
        let mut label = Node::new("label", "Pay");
        label.text = "Pay [img=sprites/coin.png] now".into();
        label.size = [400.0, 40.0];
        screen.nodes.push(label);
        let mut broken = Node::new("label", "Broken");
        broken.text = "see [img=sprites/coin.png now".into();
        broken.size = [400.0, 40.0];
        screen.nodes.push(broken);
        let live = Live::new(screen, Theme::default());
        let draws = draw(&live, 0.0);
        assert!(draws
            .iter()
            .any(|d| matches!(d, Draw::Picture { path, .. } if path.contains("coin.png"))));
        let letters: String = draws
            .iter()
            .filter_map(|d| match d {
                Draw::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(letters.contains("[img=sprites/coin.png now"), "{letters}");
    }

    #[test]
    fn image_slice_props_load_from_ui_text() {
        let text = "seal ui 1\npanel Root\n  image Panel\n    picture sprites/panel.png\n    slice 8\n    slice_mode tile\n";
        let screen = Screen::from_text(text).unwrap();
        let i = screen.find("Panel").unwrap();
        assert_eq!(screen.nodes[i].slice, [8.0; 4]);
        assert_eq!(screen.nodes[i].slice_mode, "tile");
        let draws = draw(&Live::new(screen.clone(), Theme::default()), 0.0);
        assert!(draws.iter().any(|d| matches!(
            d,
            Draw::Picture { slice, slice_mode, .. } if *slice == [8.0; 4] && slice_mode == "tile"
        )));
    }
}
