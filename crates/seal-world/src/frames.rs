//! Sprite animations: a `.frames` file holds named clips (idle, run, jump...),
//! each a list of frames at a speed, played once or looped. A frame is any
//! rectangle of any picture, so clips can come from one sheet, several
//! sheets, or separate picture files.
//!
//! File format (text, one item per line, `#` starts a comment):
//!
//! ```text
//! animation idle fps=8 loop=yes
//! frame hero.png 0 0 0.25 1
//! frame hero.png 0.25 0 0.5 1
//! animation jump fps=10 loop=no
//! frame jump.png
//! ```
//!
//! Frame coordinates are u0 v0 u1 v1 from 0 to 1, top left to bottom right;
//! left out, the frame is the whole picture. Picture paths are relative to
//! the `.frames` file's folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

/// File extension of sprite animation files.
pub const EXTENSION: &str = "frames";

/// A named point on one frame, from 0 to 1 across and down the frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Socket {
    pub name: String,
    /// u, v with v pointing down.
    pub at: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Picture path relative to the `.frames` file's folder.
    pub image: String,
    /// u0, v0, u1, v1 with v pointing down.
    pub uv: [f32; 4],
    /// Attachment points, in the order they were written.
    pub sockets: Vec<Socket>,
    /// Point of the cell that sits on the object. 0, 0 is the bottom left.
    pub pivot: [f32; 2],
    /// Nine-slice insets in pixels: left, right, top, bottom.
    pub border: [f32; 4],
    /// Tight outline in cell space, y pointing up.
    pub outline: Vec<[f32; 2]>,
    /// Collider outline in cell space, y pointing up.
    pub physics: Vec<[f32; 2]>,
    /// Name sent to the object's graph when this frame starts. Empty sends nothing.
    pub event: String,
}

impl Frame {
    pub fn whole(image: &str) -> Frame {
        Frame::cell(image, [0.0, 0.0, 1.0, 1.0])
    }

    pub fn cell(image: &str, uv: [f32; 4]) -> Frame {
        Frame {
            image: image.into(),
            uv,
            pivot: [0.5, 0.5],
            border: [0.0; 4],
            outline: Vec::new(),
            physics: Vec::new(),
            sockets: Vec::new(),
            event: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub name: String,
    pub fps: f32,
    pub looping: bool,
    pub frames: Vec<Frame>,
}

impl Clip {
    pub fn new(name: &str) -> Clip {
        Clip { name: name.into(), fps: 8.0, looping: true, frames: Vec::new() }
    }

    /// Seconds one pass through the clip takes.
    pub fn length(&self) -> f32 {
        if self.fps <= 0.0 {
            return 0.0;
        }
        self.frames.len() as f32 / self.fps
    }

    /// How many frames have started by `seconds`, counting past the last
    /// frame. Frame 0 starts at time 0. A clip with no frames stays at 0.
    pub fn index_unwrapped(&self, seconds: f32) -> u64 {
        if self.frames.is_empty() || self.fps <= 0.0 || seconds <= 0.0 {
            return 0;
        }
        (seconds * self.fps + 1e-4).floor() as u64
    }

    /// Frame index `seconds` after the clip started; a clip that does not
    /// loop holds its last frame.
    pub fn frame_at(&self, seconds: f32) -> usize {
        let n = self.frames.len();
        if n <= 1 || self.fps <= 0.0 || seconds <= 0.0 {
            return 0;
        }
        let i = self.index_unwrapped(seconds);
        if self.looping {
            (i % n as u64) as usize
        } else {
            (i as usize).min(n - 1)
        }
    }

    /// Whether a clip that does not loop has reached its end.
    pub fn finished(&self, seconds: f32) -> bool {
        !self.looping && seconds >= self.length()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpriteFrames {
    pub clips: Vec<Clip>,
}

impl SpriteFrames {
    pub fn clip(&self, name: &str) -> Option<&Clip> {
        self.clips.iter().find(|c| c.name == name)
    }

    pub fn clip_mut(&mut self, name: &str) -> Option<&mut Clip> {
        self.clips.iter_mut().find(|c| c.name == name)
    }

    /// The named clip, or the first one when the name is empty or unknown.
    pub fn clip_or_first(&self, name: &str) -> Option<&Clip> {
        self.clip(name).or(self.clips.first())
    }

    pub fn names(&self) -> Vec<String> {
        self.clips.iter().map(|c| c.name.clone()).collect()
    }

    /// A clip name not used yet, starting from `base`.
    pub fn unused_name(&self, base: &str) -> String {
        if self.clip(base).is_none() {
            return base.into();
        }
        (2..).map(|i| format!("{base} {i}")).find(|n| self.clip(n).is_none()).unwrap()
    }

    pub fn parse(text: &str) -> Result<SpriteFrames, String> {
        let mut out = SpriteFrames::default();
        for (number, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let err = |what: &str| format!("line {}: {what}", number + 1);
            let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            match word {
                "animation" => {
                    let mut clip = Clip::new("");
                    let mut name = Vec::new();
                    for part in rest.split_whitespace() {
                        if let Some(v) = part.strip_prefix("fps=") {
                            clip.fps = v.parse().map_err(|_| err("fps is not a number"))?;
                        } else if let Some(v) = part.strip_prefix("loop=") {
                            clip.looping = matches!(v, "yes" | "true" | "1");
                        } else {
                            name.push(part);
                        }
                    }
                    clip.name = name.join(" ");
                    if clip.name.is_empty() {
                        return Err(err("animation needs a name"));
                    }
                    out.clips.push(clip);
                }
                "frame" => {
                    let clip =
                        out.clips.last_mut().ok_or_else(|| err("frame before any animation"))?;
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    let (keys, rest_parts): (Vec<&str>, Vec<&str>) =
                        parts.into_iter().partition(|p| p.contains('='));
                    let (image, nums) = split_image(&rest_parts);
                    if image.is_empty() {
                        return Err(err("frame needs a picture"));
                    }
                    let uv = match nums.len() {
                        0 => [0.0, 0.0, 1.0, 1.0],
                        4 => {
                            let mut uv = [0.0; 4];
                            for (slot, n) in uv.iter_mut().zip(nums) {
                                *slot =
                                    n.parse().map_err(|_| err("frame corner is not a number"))?;
                            }
                            uv
                        }
                        _ => return Err(err("frame needs a picture and 0 or 4 numbers")),
                    };
                    let mut frame = Frame::cell(&image, uv);
                    for key in keys {
                        let (name, value) = key.split_once('=').unwrap();
                        match name {
                            "pivot" => frame.pivot = pair2(value).map_err(err)?,
                            "border" => frame.border = pair4(value).map_err(err)?,
                            "outline" => frame.outline = pair_list(value).map_err(err)?,
                            "phys" => frame.physics = pair_list(value).map_err(err)?,
                            _ => return Err(err(&format!("unknown frame field `{name}`"))),
                        }
                    }
                    clip.frames.push(frame);
                }
                "event" => {
                    let clip =
                        out.clips.last_mut().ok_or_else(|| err("event before any animation"))?;
                    let frame =
                        clip.frames.last_mut().ok_or_else(|| err("event before any frame"))?;
                    let name = rest.trim();
                    if name.is_empty() {
                        return Err(err("event needs a name"));
                    }
                    if name.chars().any(|c| c.is_control()) {
                        return Err(err("event name has a control character"));
                    }
                    if !frame.event.is_empty() {
                        return Err(err("this frame already has an event"));
                    }
                    frame.event = name.to_string();
                }
                "socket" => {
                    let clip =
                        out.clips.last_mut().ok_or_else(|| err("socket before any animation"))?;
                    let frame =
                        clip.frames.last_mut().ok_or_else(|| err("socket before any frame"))?;
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if parts.len() != 3 {
                        return Err(err("socket needs a name and two numbers"));
                    }
                    if !socket_name(parts[0]) {
                        return Err(err("socket name must be letters, digits, _ or -"));
                    }
                    if frame.sockets.iter().any(|s| s.name == parts[0]) {
                        return Err(err("that socket is already on this frame"));
                    }
                    let u =
                        parts[1].parse::<f32>().map_err(|_| err("socket point is not a number"))?;
                    let v =
                        parts[2].parse::<f32>().map_err(|_| err("socket point is not a number"))?;
                    if !u.is_finite() || !v.is_finite() {
                        return Err(err("socket point is not a number"));
                    }
                    frame.sockets.push(Socket { name: parts[0].into(), at: [u, v] });
                }
                _ => return Err(err(&format!("unknown word `{word}`"))),
            }
        }
        Ok(out)
    }

    pub fn to_text(&self) -> String {
        let mut s = String::from("# Seal sprite animations\n");
        for clip in &self.clips {
            s.push_str(&format!(
                "animation {} fps={} loop={}\n",
                clip.name,
                num(clip.fps),
                if clip.looping { "yes" } else { "no" }
            ));
            for f in &clip.frames {
                if f.uv == [0.0, 0.0, 1.0, 1.0] {
                    s.push_str(&format!("frame {}", f.image));
                } else {
                    s.push_str(&format!(
                        "frame {} {} {} {} {}",
                        f.image,
                        num(f.uv[0]),
                        num(f.uv[1]),
                        num(f.uv[2]),
                        num(f.uv[3])
                    ));
                }
                if f.pivot != [0.5, 0.5] {
                    s.push_str(&format!(" pivot={},{}", num(f.pivot[0]), num(f.pivot[1])));
                }
                if f.border != [0.0; 4] {
                    s.push_str(&format!(
                        " border={},{},{},{}",
                        num(f.border[0]),
                        num(f.border[1]),
                        num(f.border[2]),
                        num(f.border[3])
                    ));
                }
                if !f.outline.is_empty() {
                    s.push_str(" outline=");
                    s.push_str(&points_text(&f.outline));
                }
                if !f.physics.is_empty() {
                    s.push_str(" phys=");
                    s.push_str(&points_text(&f.physics));
                }
                s.push('\n');
                if !f.event.is_empty() {
                    s.push_str("event ");
                    s.push_str(&f.event);
                    s.push('\n');
                }
                for socket in &f.sockets {
                    s.push_str(&format!(
                        "socket {} {} {}\n",
                        socket.name,
                        num(socket.at[0]),
                        num(socket.at[1])
                    ));
                }
            }
        }
        s
    }

    pub fn load(path: &Path) -> Result<SpriteFrames, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        SpriteFrames::parse(&text)
    }

    /// Writes through a temporary file so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("frames.tmp");
        std::fs::write(&tmp, self.to_text())?;
        std::fs::rename(&tmp, path)?;
        invalidate(path);
        Ok(())
    }
}

fn socket_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Picture names may contain spaces: the image is everything before the
/// trailing numbers.
fn split_image<'a>(parts: &[&'a str]) -> (String, Vec<&'a str>) {
    let mut cut = parts.len();
    while cut > 0 && parts.len() - cut < 4 && parts[cut - 1].parse::<f32>().is_ok() {
        cut -= 1;
    }
    if parts.len() - cut != 4 {
        cut = parts.len();
    }
    // A picture named only by digits keeps its name.
    if cut == 0 {
        cut = parts.len().min(1);
    }
    (parts[..cut].join(" "), parts[cut..].to_vec())
}

fn pair2(text: &str) -> Result<[f32; 2], &'static str> {
    let v = numbers(text, 2)?;
    Ok([v[0], v[1]])
}

fn pair4(text: &str) -> Result<[f32; 4], &'static str> {
    let v = numbers(text, 4)?;
    Ok([v[0], v[1], v[2], v[3]])
}

fn pair_list(text: &str) -> Result<Vec<[f32; 2]>, &'static str> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.split(';')
        .map(|part| {
            let v = numbers(part, 2)?;
            Ok([v[0], v[1]])
        })
        .collect()
}

fn numbers(text: &str, count: usize) -> Result<Vec<f32>, &'static str> {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != count || parts.iter().any(|p| p.is_empty()) {
        return Err("frame field is not a list of numbers");
    }
    parts
        .iter()
        .map(|p| p.parse::<f32>().map_err(|_| "frame field is not a list of numbers"))
        .collect::<Result<Vec<f32>, _>>()
        .and_then(|v| {
            if v.iter().all(|n| n.is_finite()) {
                Ok(v)
            } else {
                Err("frame field is not a list of numbers")
            }
        })
}

fn points_text(points: &[[f32; 2]]) -> String {
    points.iter().map(|p| format!("{},{}", num(p[0]), num(p[1]))).collect::<Vec<_>>().join(";")
}

fn num(v: f32) -> String {
    let s = format!("{:.6}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// Frames from a sheet split into `cols` x `rows` equal cells, read left to
/// right then top to bottom, from cell `first` for `count` cells.
pub fn grid_frames(image: &str, cols: u32, rows: u32, first: u32, count: u32) -> Vec<Frame> {
    let (cols, rows) = (cols.max(1), rows.max(1));
    let total = cols * rows;
    (first..(first + count).min(total))
        .map(|i| {
            let (c, r) = ((i % cols) as f32, (i / cols) as f32);
            let (w, h) = (1.0 / cols as f32, 1.0 / rows as f32);
            Frame::cell(image, [c * w, r * h, (c + 1.0) * w, (r + 1.0) * h])
        })
        .collect()
}

/// Frames from a sheet cut where fully clear columns or rows separate the
/// pictures (`rgba` is width x height x 4 bytes). Cells are read top to
/// bottom, then left to right inside a row band. Every frame gets the size of
/// the largest picture (centred across, standing on its bottom edge), so a
/// thin in-between pose is not stretched when it is drawn.
pub fn auto_frames(image: &str, rgba: &[u8], width: u32, height: u32) -> Vec<Frame> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return Vec::new();
    }
    let solid = |x: usize, y: usize| rgba[(y * w + x) * 4 + 3] > 8;
    let bands = runs((0..h).map(|y| (0..w).any(|x| solid(x, y))));
    let mut boxes = Vec::new();
    for (y0, y1) in bands {
        let cols = runs((0..w).map(|x| (y0..y1).any(|y| solid(x, y))));
        for (x0, x1) in cols {
            // Tighten to the picture inside the band.
            let ys: Vec<usize> = (y0..y1).filter(|&y| (x0..x1).any(|x| solid(x, y))).collect();
            boxes.push((x0, ys[0], x1, ys[ys.len() - 1] + 1));
        }
    }
    let mw = boxes.iter().map(|b| b.2 - b.0).max().unwrap_or(0);
    let mh = boxes.iter().map(|b| b.3 - b.1).max().unwrap_or(0);
    boxes
        .into_iter()
        .map(|(x0, _, x1, b)| {
            let left = ((x0 + x1) / 2).saturating_sub(mw / 2).min(w - mw);
            let top = b.saturating_sub(mh).min(h - mh);
            Frame::cell(
                image,
                [
                    left as f32 / w as f32,
                    top as f32 / h as f32,
                    (left + mw) as f32 / w as f32,
                    (top + mh) as f32 / h as f32,
                ],
            )
        })
        .collect()
}

fn runs(filled: impl Iterator<Item = bool>) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    let mut i = 0;
    for on in filled {
        match (on, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, i));
                start = None;
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(s) = start {
        out.push((s, i));
    }
    out
}

type Cache = Mutex<HashMap<PathBuf, (Option<SystemTime>, Option<Arc<SpriteFrames>>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The file at `path`, read again only when it changed on disk.
pub fn load_cached(path: &Path) -> Option<Arc<SpriteFrames>> {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let mut map = cache().lock().ok()?;
    if let Some((seen, frames)) = map.get(path) {
        if *seen == mtime && mtime.is_some() {
            return frames.clone();
        }
    }
    let frames = SpriteFrames::load(path).ok().map(Arc::new);
    map.insert(path.to_path_buf(), (mtime, frames.clone()));
    frames
}

/// Forget a cached file so the next read sees a fresh save at once.
pub fn invalidate(path: &Path) {
    if let Ok(mut map) = cache().lock() {
        map.remove(path);
    }
}

/// Pivot and nine-slice of the frame a sprite is showing.
///
/// `None` when `sprite` is not an animation file. The border is in picture
/// pixels (left, right, top, bottom). Pivot is 0, 0 at the bottom left.
pub fn frame_look(
    sprite: &str,
    animation: &str,
    frame_override: Option<u32>,
    anim_start: f32,
    seconds: f32,
    project: Option<&Path>,
) -> Option<([f32; 2], [f32; 4])> {
    if !is_frames_path(sprite) {
        return None;
    }
    let raw = Path::new(sprite);
    let full = match project {
        Some(root) if raw.is_relative() => root.join(raw),
        _ => raw.to_path_buf(),
    };
    let lib = load_cached(&full)?;
    let clip = lib.clip_or_first(animation)?;
    let idx = match frame_override {
        Some(f) => (f as usize).min(clip.frames.len().saturating_sub(1)),
        None => clip.frame_at(if seconds > 0.0 { seconds - anim_start } else { 0.0 }),
    };
    let frame = clip.frames.get(idx)?;
    Some((frame.pivot, frame.border))
}

/// Whether a sprite path names an animation file rather than a picture.
pub fn is_frames_path(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}
