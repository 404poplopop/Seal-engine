//! Sprite atlases: Export Game packs the game's small pictures onto a few
//! large pages, so the game loads a handful of textures instead of one per file
//! and draws sprites from the same page together.
//!
//! `atlas.txt` in the packaged data folder lists the pages and where each
//! picture sits on them:
//!
//! ```text
//! seal atlas 1
//! page atlas/page0.png 1024 512
//! sprite 0 0 0 24 24 sprites/panel.png
//! ```
//!
//! A `sprite` line is `page x y width height path`. The original pictures stay
//! in the package too, for anything that reads their pixels.

use crate::error::CoreError;
use std::path::Path;

/// Index file written beside the packaged project.
pub const INDEX: &str = "atlas.txt";
/// Folder holding the page pictures.
pub const FOLDER: &str = "atlas";
/// Largest page side, in pixels.
pub const PAGE_SIZE: u32 = 2048;
/// Pictures larger than this on either side keep their own texture.
pub const MAX_PICTURE: u32 = 512;
/// Empty pixels between pictures, so neighbours never bleed into each other.
const PADDING: u32 = 2;

/// Where one picture sits on an atlas page.
///
/// `x`, `y`, `w`, `h` are the texels written on the page. `source_w` and
/// `source_h` are the picture's size before trimming and scaling, which is the
/// size it still draws at. `origin_x` and `origin_y` are where those texels sit
/// inside that picture. `content_w` and `content_h` are the packed region in
/// those same picture pixels. `turned` means the texels were rotated a quarter
/// turn clockwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub path: String,
    pub page: usize,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub source_w: u32,
    pub source_h: u32,
    pub origin_x: u32,
    pub origin_y: u32,
    pub content_w: u32,
    pub content_h: u32,
    pub turned: bool,
}

impl Placed {
    /// A picture packed whole, with no trim, scale, or turn.
    pub fn whole(path: impl Into<String>, page: usize, x: u32, y: u32, w: u32, h: u32) -> Self {
        Self {
            path: path.into(),
            page,
            x,
            y,
            w,
            h,
            source_w: w,
            source_h: h,
            origin_x: 0,
            origin_y: 0,
            content_w: w,
            content_h: h,
            turned: false,
        }
    }

    /// True when the index line can stay in the original six-field form.
    pub fn legacy_line(&self) -> bool {
        !self.turned
            && self.origin_x == 0
            && self.origin_y == 0
            && self.source_w == self.w
            && self.source_h == self.h
            && self.content_w == self.w
            && self.content_h == self.h
    }

    /// Trim or a quarter turn: drawing has to put the texels back.
    pub fn needs_layout(&self) -> bool {
        self.turned
            || self.origin_x != 0
            || self.origin_y != 0
            || self.content_w != self.source_w
            || self.content_h != self.source_h
    }
}

/// How Export packs sprite pages for one preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtlasOptions {
    /// Pack the opaque bounds and remember where they sat in the picture.
    pub tight: bool,
    /// Turn a picture a quarter turn when that is the only way it fits.
    pub rotate: bool,
    /// Largest page side, in pixels. A power of two.
    pub page_size: u32,
    /// Packed texel size as a percent of the picture. 100 is full size, 50 is half.
    pub scale_percent: u32,
}

impl Default for AtlasOptions {
    fn default() -> Self {
        Self { tight: false, rotate: false, page_size: PAGE_SIZE, scale_percent: 100 }
    }
}

impl AtlasOptions {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// One picture on a shelf, after an optional quarter turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShelfSpot {
    pub page: usize,
    pub x: u32,
    pub y: u32,
    pub turned: bool,
}

/// Shelf packing that may turn pictures, with the space each page actually used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShelfPack {
    pub spots: Vec<Option<ShelfSpot>>,
    /// Width and height filled on each page, before rounding up.
    pub used: Vec<(u32, u32)>,
    /// Texture size of each page. Never larger than the page limit.
    pub pages: Vec<(u32, u32)>,
}

/// A packed atlas: page sizes and every placed picture.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Atlas {
    /// Page picture paths (relative to the data folder), their sizes, and
    /// whether the page is sampled nearest (`true`) or linear (`false`).
    pub pages: Vec<(String, u32, u32, bool)>,
    pub sprites: Vec<Placed>,
}

impl Atlas {
    pub fn to_text(&self) -> String {
        let legacy = self.sprites.iter().all(Placed::legacy_line);
        let mut out =
            if legacy { String::from("seal atlas 1\n") } else { String::from("seal atlas 2\n") };
        for (path, w, h, nearest) in &self.pages {
            let filter = if *nearest { "nearest" } else { "linear" };
            out += &format!("page {path} {w} {h} {filter}\n");
        }
        for s in &self.sprites {
            if legacy {
                out += &format!("sprite {} {} {} {} {} {}\n", s.page, s.x, s.y, s.w, s.h, s.path);
            } else {
                let turn = u8::from(s.turned);
                out += &format!(
                    "sprite {} {} {} {} {} {} {} {} {} {} {} {turn} {}\n",
                    s.page,
                    s.x,
                    s.y,
                    s.w,
                    s.h,
                    s.source_w,
                    s.source_h,
                    s.origin_x,
                    s.origin_y,
                    s.content_w,
                    s.content_h,
                    s.path
                );
            }
        }
        out
    }

    pub fn parse(text: &str) -> Option<Atlas> {
        let mut lines = text.lines();
        let version = match lines.next()?.trim() {
            "seal atlas 1" => 1,
            "seal atlas 2" => 2,
            _ => return None,
        };
        let mut atlas = Atlas::default();
        for line in lines {
            let mut parts = line.splitn(2, ' ');
            match (parts.next()?, parts.next()) {
                ("page", Some(rest)) => {
                    let mut f = rest.rsplitn(4, ' ');
                    let tail = f.next()?;
                    let (nearest, h, w, path) = if tail == "nearest" || tail == "linear" {
                        let h = f.next()?.parse().ok()?;
                        let w = f.next()?.parse().ok()?;
                        (tail != "linear", h, w, f.next()?.to_string())
                    } else {
                        let h = tail.parse().ok()?;
                        let w = f.next()?.parse().ok()?;
                        (true, h, w, f.next()?.to_string())
                    };
                    if w == 0 || h == 0 {
                        return None;
                    }
                    atlas.pages.push((path, w, h, nearest));
                }
                ("sprite", Some(rest)) => {
                    let placed = if version == 1 {
                        let f: Vec<&str> = rest.splitn(6, ' ').collect();
                        if f.len() != 6 {
                            return None;
                        }
                        let n = |i: usize| f[i].parse::<u32>().ok();
                        Placed::whole(f[5], n(0)? as usize, n(1)?, n(2)?, n(3)?, n(4)?)
                    } else {
                        let f: Vec<&str> = rest.splitn(13, ' ').collect();
                        if f.len() != 13 {
                            return None;
                        }
                        let n = |i: usize| f[i].parse::<u32>().ok();
                        let turn = n(11)?;
                        if turn > 1 || f[12].is_empty() {
                            return None;
                        }
                        Placed {
                            page: n(0)? as usize,
                            x: n(1)?,
                            y: n(2)?,
                            w: n(3)?,
                            h: n(4)?,
                            source_w: n(5)?,
                            source_h: n(6)?,
                            origin_x: n(7)?,
                            origin_y: n(8)?,
                            content_w: n(9)?,
                            content_h: n(10)?,
                            turned: turn == 1,
                            path: f[12].to_string(),
                        }
                    };
                    if placed.page >= atlas.pages.len()
                        || placed.w == 0
                        || placed.h == 0
                        || placed.source_w == 0
                        || placed.source_h == 0
                        || placed.content_w == 0
                        || placed.content_h == 0
                        || placed.origin_x.saturating_add(placed.content_w) > placed.source_w
                        || placed.origin_y.saturating_add(placed.content_h) > placed.source_h
                    {
                        return None;
                    }
                    atlas.sprites.push(placed);
                }
                ("", None) => {}
                _ => return None,
            }
        }
        Some(atlas)
    }

    /// The atlas written in `data`, when there is one.
    pub fn load(data: &Path) -> Option<Atlas> {
        Atlas::parse(&std::fs::read_to_string(data.join(INDEX)).ok()?)
    }
}

/// Page index, x and y of one packed picture.
pub type Spot = (usize, u32, u32);

/// Shelf packing: tallest pictures first, left to right in rows, opening a new
/// page when one is full. Returns placements (without paths) in input order
/// and each page's used size, rounded up to a power of two.
pub fn pack(sizes: &[(u32, u32)], page_size: u32) -> (Vec<Spot>, Vec<(u32, u32)>) {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(sizes[i].1), std::cmp::Reverse(sizes[i].0), i));
    let mut spots = vec![(0, 0, 0); sizes.len()];
    let mut pages: Vec<(u32, u32)> = Vec::new();
    let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
    for i in order {
        let (w, h) = (sizes[i].0 + PADDING, sizes[i].1 + PADDING);
        if pages.is_empty() {
            pages.push((0, 0));
        }
        if x + w > page_size {
            x = 0;
            y += row;
            row = 0;
        }
        if y + h > page_size {
            pages.push((0, 0));
            (x, y, row) = (0, 0, 0);
        }
        let page = pages.len() - 1;
        spots[i] = (page, x, y);
        pages[page].0 = pages[page].0.max(x + w);
        pages[page].1 = pages[page].1.max(y + h);
        x += w;
        row = row.max(h);
    }
    let pages =
        pages.into_iter().map(|(w, h)| (w.next_power_of_two(), h.next_power_of_two())).collect();
    (spots, pages)
}

fn pictures(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if dir == root && entry.file_name() == FOLDER {
                continue;
            }
            pictures(root, &path, out)?;
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}

/// Pack every PNG picture in `data` up to [`MAX_PICTURE`] on a side into atlas
/// pages and write `atlas.txt`. Returns how many pictures were packed; with
/// none, nothing is written.
pub fn build(data: &Path) -> Result<usize, CoreError> {
    build_with_page_size(data, PAGE_SIZE)
}

pub fn build_with_page_size(data: &Path, page_size: u32) -> Result<usize, CoreError> {
    let page_size = if page_size == 0 { PAGE_SIZE } else { page_size };
    build_with(data, &AtlasOptions { page_size, ..AtlasOptions::default() })
}

/// Pack like [`build`], using the export preset's atlas options.
///
/// Default options take the same path as [`build`], so an existing package does
/// not change. Any other option trims, turns, limits the page, or scales.
pub fn build_with(data: &Path, options: &AtlasOptions) -> Result<usize, CoreError> {
    if options.is_default() {
        return build_default(data);
    }
    build_configured(data, options)
}

fn build_default(data: &Path) -> Result<usize, CoreError> {
    let mut paths = Vec::new();
    pictures(data, data, &mut paths)?;
    let mut images = Vec::new();
    for rel in paths {
        if let Some((rgba, w, h)) = crate::asset_editor::decode_png(&data.join(&rel)) {
            if w > 0 && h > 0 && w <= MAX_PICTURE && h <= MAX_PICTURE {
                images.push((rel, rgba, w, h));
            }
        }
    }
    if images.is_empty() {
        return Ok(0);
    }
    // Nearest and linear pictures cannot share a page: a page has one sampler.
    let mut groups: [Vec<_>; 2] = [Vec::new(), Vec::new()];
    for image in images {
        let nearest = crate::import_presets::picture_look(&data.join(&image.0)).nearest;
        groups[usize::from(!nearest)].push(image);
    }
    let mut atlas = Atlas::default();
    let mut canvases: Vec<Vec<u8>> = Vec::new();
    for (nearest, group) in groups.into_iter().enumerate() {
        let nearest = nearest == 0;
        if group.is_empty() {
            continue;
        }
        let sizes: Vec<(u32, u32)> = group.iter().map(|i| (i.2, i.3)).collect();
        let (spots, page_sizes) = pack(&sizes, PAGE_SIZE);
        let base = atlas.pages.len();
        for (n, (w, h)) in page_sizes.iter().enumerate() {
            atlas.pages.push((format!("{FOLDER}/page{}.png", base + n), *w, *h, nearest));
            canvases.push(vec![0u8; (*w * *h * 4) as usize]);
        }
        for ((rel, rgba, w, h), (page, x, y)) in group.into_iter().zip(spots) {
            let page = base + page;
            let page_w = atlas.pages[page].1;
            for row in 0..h {
                let from = (row * w * 4) as usize;
                let to = (((y + row) * page_w + x) * 4) as usize;
                canvases[page][to..to + (w * 4) as usize]
                    .copy_from_slice(&rgba[from..from + (w * 4) as usize]);
            }
            atlas.sprites.push(Placed::whole(rel, page, x, y, w, h));
        }
    }
    std::fs::create_dir_all(data.join(FOLDER))?;
    for ((path, w, h, _), pixels) in atlas.pages.iter().zip(&canvases) {
        write_png(&data.join(path), pixels, *w, *h)?;
    }
    crate::persist::atomic_write(&data.join(INDEX), atlas.to_text().as_bytes())?;
    Ok(atlas.sprites.len())
}

struct Prepared {
    path: String,
    pixels: Vec<u8>,
    pack_w: u32,
    pack_h: u32,
    source_w: u32,
    source_h: u32,
    origin_x: u32,
    origin_y: u32,
    content_w: u32,
    content_h: u32,
}

fn build_configured(data: &Path, options: &AtlasOptions) -> Result<usize, CoreError> {
    let page_size = if options.page_size >= 64 { options.page_size } else { PAGE_SIZE };
    let scale = if (1..100).contains(&options.scale_percent) { options.scale_percent } else { 100 };
    let mut paths = Vec::new();
    pictures(data, data, &mut paths)?;
    let mut images = Vec::new();
    for rel in paths {
        if let Some((rgba, w, h)) = crate::asset_editor::decode_png(&data.join(&rel)) {
            if w > 0 && h > 0 && w <= MAX_PICTURE && h <= MAX_PICTURE {
                images.push((rel, rgba, w, h));
            }
        }
    }
    if images.is_empty() {
        return Ok(0);
    }
    let mut groups: [Vec<_>; 2] = [Vec::new(), Vec::new()];
    for image in images {
        let nearest = crate::import_presets::picture_look(&data.join(&image.0)).nearest;
        groups[usize::from(!nearest)].push(image);
    }
    let mut atlas = Atlas::default();
    let mut canvases: Vec<Vec<u8>> = Vec::new();
    for (nearest, group) in groups.into_iter().enumerate() {
        let nearest = nearest == 0;
        if group.is_empty() {
            continue;
        }
        let mut prepared = Vec::new();
        for (rel, rgba, w, h) in group {
            let (origin_x, origin_y, content_w, content_h) = if options.tight {
                opaque_bounds(&rgba, w, h).unwrap_or((0, 0, w, h))
            } else {
                (0, 0, w, h)
            };
            let cropped = crop(&rgba, w, origin_x, origin_y, content_w, content_h);
            let (pixels, pack_w, pack_h) = scale_image(&cropped, content_w, content_h, scale);
            let fits = |aw: u32, ah: u32| aw + PADDING <= page_size && ah + PADDING <= page_size;
            let placeable = if options.rotate {
                fits(pack_w, pack_h) || fits(pack_h, pack_w)
            } else {
                fits(pack_w, pack_h)
            };
            if !placeable {
                continue;
            }
            prepared.push(Prepared {
                path: rel,
                pixels,
                pack_w,
                pack_h,
                source_w: w,
                source_h: h,
                origin_x,
                origin_y,
                content_w,
                content_h,
            });
        }
        if prepared.is_empty() {
            continue;
        }
        let sizes: Vec<(u32, u32)> =
            prepared.iter().map(|item| (item.pack_w, item.pack_h)).collect();
        type TurnedSpot = Option<(usize, u32, u32, bool)>;
        let (spots, page_sizes): (Vec<TurnedSpot>, Vec<(u32, u32)>) = if options.rotate {
            let shelf = pack_turning(&sizes, page_size);
            let spots = shelf
                .spots
                .into_iter()
                .map(|spot| spot.map(|spot| (spot.page, spot.x, spot.y, spot.turned)))
                .collect();
            (spots, shelf.pages)
        } else {
            let (spots, pages) = pack(&sizes, page_size);
            let pages = pages
                .into_iter()
                .map(|(w, h)| (w.min(page_size).max(1), h.min(page_size).max(1)))
                .collect();
            let spots = spots.into_iter().map(|(page, x, y)| Some((page, x, y, false))).collect();
            (spots, pages)
        };
        let base = atlas.pages.len();
        for (n, (w, h)) in page_sizes.iter().enumerate() {
            atlas.pages.push((format!("{FOLDER}/page{}.png", base + n), *w, *h, nearest));
            canvases.push(vec![0u8; (*w as usize) * (*h as usize) * 4]);
        }
        for (item, spot) in prepared.into_iter().zip(spots) {
            let Some((page, x, y, turned)) = spot else {
                continue;
            };
            let page = base + page;
            if page >= atlas.pages.len() {
                continue;
            }
            let page_w = atlas.pages[page].1;
            blit(
                &mut canvases[page],
                page_w,
                x,
                y,
                &item.pixels,
                item.pack_w,
                item.pack_h,
                turned,
            )?;
            let (w, h) =
                if turned { (item.pack_h, item.pack_w) } else { (item.pack_w, item.pack_h) };
            atlas.sprites.push(Placed {
                path: item.path,
                page,
                x,
                y,
                w,
                h,
                source_w: item.source_w,
                source_h: item.source_h,
                origin_x: item.origin_x,
                origin_y: item.origin_y,
                content_w: item.content_w,
                content_h: item.content_h,
                turned,
            });
        }
    }
    if atlas.sprites.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(data.join(FOLDER))?;
    for ((path, w, h, _), pixels) in atlas.pages.iter().zip(&canvases) {
        write_png(&data.join(path), pixels, *w, *h)?;
    }
    crate::persist::atomic_write(&data.join(INDEX), atlas.to_text().as_bytes())?;
    Ok(atlas.sprites.len())
}

/// Shelf packing that turns a picture a quarter turn when that is what fits
/// in the remaining row, or on the page at all. Pages stay within `page_size`.
pub fn pack_turning(sizes: &[(u32, u32)], page_size: u32) -> ShelfPack {
    let page_size = page_size.max(1);
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(sizes[i].1), std::cmp::Reverse(sizes[i].0), i));
    let mut spots = vec![None; sizes.len()];
    let mut used: Vec<(u32, u32)> = Vec::new();
    let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
    for i in order {
        let (w, h) = (sizes[i].0.saturating_add(PADDING), sizes[i].1.saturating_add(PADDING));
        let (rw, rh) = (sizes[i].1.saturating_add(PADDING), sizes[i].0.saturating_add(PADDING));
        if used.is_empty() {
            used.push((0, 0));
        }
        let mut turned = fit_turn(x, y, w, h, rw, rh, page_size);
        if turned.is_none() && x > 0 {
            x = 0;
            y = y.saturating_add(row);
            row = 0;
            turned = fit_turn(x, y, w, h, rw, rh, page_size);
        }
        if turned.is_none() && used.last() != Some(&(0, 0)) {
            used.push((0, 0));
            x = 0;
            y = 0;
            row = 0;
            turned = fit_turn(0, 0, w, h, rw, rh, page_size);
        }
        let Some(turned) = turned else {
            continue;
        };
        let (ww, hh) = if turned { (rw, rh) } else { (w, h) };
        let page = used.len() - 1;
        spots[i] = Some(ShelfSpot { page, x, y, turned });
        used[page].0 = used[page].0.max(x.saturating_add(ww));
        used[page].1 = used[page].1.max(y.saturating_add(hh));
        x = x.saturating_add(ww);
        row = row.max(hh);
    }
    if used.last() == Some(&(0, 0)) {
        used.pop();
    }
    let pages = used
        .iter()
        .map(|(w, h)| {
            let side = |n: u32| n.max(1).next_power_of_two().min(page_size).max(1);
            (side(*w), side(*h))
        })
        .collect();
    ShelfPack { spots, used, pages }
}

/// `Some(true)` when only the turned rectangle fits. Upright wins when both fit.
fn fit_turn(x: u32, y: u32, w: u32, h: u32, rw: u32, rh: u32, page: u32) -> Option<bool> {
    let fits = |px: u32, py: u32, pw: u32, ph: u32| {
        pw > 0
            && ph > 0
            && pw <= page
            && ph <= page
            && px.saturating_add(pw) <= page
            && py.saturating_add(ph) <= page
    };
    match (fits(x, y, w, h), fits(x, y, rw, rh)) {
        (true, _) => Some(false),
        (false, true) => Some(true),
        (false, false) => None,
    }
}

fn opaque_bounds(rgba: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            let alpha = rgba.get(((y * w + x) * 4 + 3) as usize).copied().unwrap_or(0);
            if alpha > 0 {
                any = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + 1);
                max_y = max_y.max(y + 1);
            }
        }
    }
    any.then_some((min_x, min_y, max_x - min_x, max_y - min_y))
}

fn crop(rgba: &[u8], w: u32, x: u32, y: u32, cw: u32, ch: u32) -> Vec<u8> {
    if x == 0 && y == 0 && cw == w && ch == (rgba.len() / 4 / w.max(1) as usize) as u32 {
        return rgba.to_vec();
    }
    let mut out = vec![0u8; (cw as usize) * (ch as usize) * 4];
    for row in 0..ch {
        let from = (((y + row) * w + x) * 4) as usize;
        let to = (row * cw * 4) as usize;
        let n = (cw * 4) as usize;
        if let Some(src) = rgba.get(from..from + n) {
            out[to..to + n].copy_from_slice(src);
        }
    }
    out
}

fn scale_image(src: &[u8], w: u32, h: u32, percent: u32) -> (Vec<u8>, u32, u32) {
    if percent >= 100 || w == 0 || h == 0 {
        return (src.to_vec(), w, h);
    }
    let nw = ((w as u64 * percent as u64) / 100).max(1) as u32;
    let nh = ((h as u64 * percent as u64) / 100).max(1) as u32;
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    for dy in 0..nh {
        let y0 = (dy as u64 * h as u64) / nh as u64;
        let y1 = (((dy as u64 + 1) * h as u64) / nh as u64).max(y0 + 1).min(h as u64);
        for dx in 0..nw {
            let x0 = (dx as u64 * w as u64) / nw as u64;
            let x1 = (((dx as u64 + 1) * w as u64) / nw as u64).max(x0 + 1).min(w as u64);
            let mut acc = [0u32; 4];
            let mut count = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y as u32 * w + x as u32) * 4) as usize;
                    for (channel, sum) in acc.iter_mut().enumerate() {
                        *sum += src.get(i + channel).copied().unwrap_or(0) as u32;
                    }
                    count += 1;
                }
            }
            let o = ((dy * nw + dx) * 4) as usize;
            let count = count.max(1);
            for channel in 0..4 {
                out[o + channel] = (acc[channel] / count) as u8;
            }
        }
    }
    (out, nw, nh)
}

#[allow(clippy::too_many_arguments)]
fn blit(
    canvas: &mut [u8],
    page_w: u32,
    x: u32,
    y: u32,
    src: &[u8],
    sw: u32,
    sh: u32,
    turned: bool,
) -> Result<(), CoreError> {
    let copy = |canvas: &mut [u8], dx: u32, dy: u32, from: usize| -> Result<(), CoreError> {
        let to = ((dy as usize) * (page_w as usize) + dx as usize) * 4;
        let src = src.get(from..from + 4).ok_or_else(|| CoreError::new("atlas sprite is short"))?;
        let dest = canvas
            .get_mut(to..to + 4)
            .ok_or_else(|| CoreError::new("atlas sprite falls off the page"))?;
        dest.copy_from_slice(src);
        Ok(())
    };
    if !turned {
        for row in 0..sh {
            for col in 0..sw {
                copy(canvas, x + col, y + row, ((row * sw + col) * 4) as usize)?;
            }
        }
    } else {
        for sy in 0..sh {
            for sx in 0..sw {
                let dx = x + (sh - 1 - sy);
                let dy = y + sx;
                copy(canvas, dx, dy, ((sy * sw + sx) * 4) as usize)?;
            }
        }
    }
    Ok(())
}

/// The packed region inside the original picture, as fractions from the top left.
pub fn content_frame(sprite: &Placed) -> [f32; 4] {
    let sw = sprite.source_w.max(1) as f32;
    let sh = sprite.source_h.max(1) as f32;
    [
        sprite.origin_x as f32 / sw,
        sprite.origin_y as f32 / sh,
        (sprite.origin_x + sprite.content_w) as f32 / sw,
        (sprite.origin_y + sprite.content_h) as f32 / sh,
    ]
}

/// Where the written texels sit on a page of `page_w` by `page_h`.
pub fn page_uv(sprite: &Placed, page_w: u32, page_h: u32) -> [f32; 4] {
    let pw = page_w.max(1) as f32;
    let ph = page_h.max(1) as f32;
    [
        sprite.x as f32 / pw,
        sprite.y as f32 / ph,
        (sprite.x + sprite.w) as f32 / pw,
        (sprite.y + sprite.h) as f32 / ph,
    ]
}

/// Page UV for one point of the original picture (`u` and `v` run 0 to 1, v down).
/// A turned sprite is sampled back to the picture's own orientation.
pub fn picture_corner_uv(sprite: &Placed, page_w: u32, page_h: u32, u: f32, v: f32) -> [f32; 2] {
    let frame = content_frame(sprite);
    let cu = (u - frame[0]) / (frame[2] - frame[0]).max(1e-6);
    let cv = (v - frame[1]) / (frame[3] - frame[1]).max(1e-6);
    let (du, dv) = if sprite.turned { (1.0 - cv, cu) } else { (cu, cv) };
    let rect = page_uv(sprite, page_w, page_h);
    [rect[0] + du * (rect[2] - rect[0]), rect[1] + dv * (rect[3] - rect[1])]
}

/// Page texel holding an original-picture pixel. `None` when that pixel was
/// trimmed away. Scaling maps several picture pixels onto one texel.
pub fn page_texel(sprite: &Placed, lx: u32, ly: u32) -> Option<(u32, u32)> {
    if lx >= sprite.source_w || ly >= sprite.source_h {
        return None;
    }
    if lx < sprite.origin_x || ly < sprite.origin_y {
        return None;
    }
    let cx = lx - sprite.origin_x;
    let cy = ly - sprite.origin_y;
    if cx >= sprite.content_w
        || cy >= sprite.content_h
        || sprite.content_w == 0
        || sprite.content_h == 0
    {
        return None;
    }
    let (dw, dh) = if sprite.turned { (sprite.h, sprite.w) } else { (sprite.w, sprite.h) };
    if dw == 0 || dh == 0 {
        return None;
    }
    let sx = ((cx as u64 * dw as u64) / sprite.content_w as u64).min(dw as u64 - 1) as u32;
    let sy = ((cy as u64 * dh as u64) / sprite.content_h as u64).min(dh as u64 - 1) as u32;
    let (dx, dy) = if sprite.turned { (dh - 1 - sy, sx) } else { (sx, sy) };
    Some((sprite.x + dx, sprite.y + dy))
}

fn write_png(path: &Path, rgba: &[u8], w: u32, h: u32) -> Result<(), CoreError> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer =
        encoder.write_header().map_err(|e| CoreError::new(format!("atlas page: {e}")))?;
    writer.write_image_data(rgba).map_err(|e| CoreError::new(format!("atlas page: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelf_packing_never_overlaps_and_opens_pages_when_full() {
        let sizes = vec![(30, 10), (50, 40), (20, 20), (60, 60), (10, 50)];
        let (spots, pages) = pack(&sizes, 100);
        for a in 0..sizes.len() {
            let (pa, xa, ya) = spots[a];
            assert!(xa + sizes[a].0 <= 100 && ya + sizes[a].1 <= 100);
            for b in a + 1..sizes.len() {
                let (pb, xb, yb) = spots[b];
                let apart = pa != pb
                    || xa + sizes[a].0 <= xb
                    || xb + sizes[b].0 <= xa
                    || ya + sizes[a].1 <= yb
                    || yb + sizes[b].1 <= ya;
                assert!(apart, "{a} and {b} overlap: {spots:?}");
            }
        }
        assert!(pages.len() >= 2, "{pages:?}");
        assert!(pages.iter().all(|(w, h)| w.is_power_of_two() && h.is_power_of_two()));
    }

    #[test]
    fn index_text_round_trips_and_rejects_nonsense() {
        let atlas = Atlas {
            pages: vec![("atlas/page0.png".into(), 64, 32, true)],
            sprites: vec![Placed::whole("my sprites/a b.png", 0, 2, 3, 8, 9)],
        };
        assert_eq!(Atlas::parse(&atlas.to_text()), Some(atlas));
        assert_eq!(Atlas::parse("seal atlas 1\nsprite 0 0 0 1 1 a.png\n"), None);
        assert_eq!(Atlas::parse("hello"), None);
        let old = "seal atlas 1\npage atlas/page0.png 64 32\nsprite 0 0 0 8 9 a.png\n";
        let parsed = Atlas::parse(old).unwrap();
        assert!(parsed.pages[0].3, "a page written before filters samples nearest");
    }

    #[test]
    fn linear_and_nearest_pictures_land_on_different_pages() {
        let dir = std::env::temp_dir().join(format!(
            "seal-atlas-filter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sprites")).unwrap();
        let png = |name: &str| {
            let path = dir.join(name);
            let file = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
            let mut encoder = png::Encoder::new(file, 4, 4);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&[255; 64]).unwrap();
        };
        png("sprites/hard.png");
        png("sprites/soft.png");
        let hard = crate::import_presets::FileImportState {
            settings: crate::import_presets::ImportSettings::Picture(
                crate::import_presets::PictureSettings {
                    filter: "nearest".into(),
                    ..Default::default()
                },
            ),
            customized: true,
            platform_overrides: Default::default(),
        };
        let soft = crate::import_presets::FileImportState {
            settings: crate::import_presets::ImportSettings::Picture(
                crate::import_presets::PictureSettings {
                    filter: "linear".into(),
                    ..Default::default()
                },
            ),
            customized: true,
            platform_overrides: Default::default(),
        };
        crate::import_presets::save_file_state(&dir, "sprites/hard.png", &hard).unwrap();
        crate::import_presets::save_file_state(&dir, "sprites/soft.png", &soft).unwrap();
        assert!(build(&dir).unwrap() >= 2);
        let atlas = Atlas::load(&dir).unwrap();
        let page =
            |name: &str| atlas.sprites.iter().find(|s| s.path == name).map(|s| s.page).unwrap();
        assert_ne!(page("sprites/hard.png"), page("sprites/soft.png"));
        assert!(atlas.pages[page("sprites/hard.png")].3);
        assert!(!atlas.pages[page("sprites/soft.png")].3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
