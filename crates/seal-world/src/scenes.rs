//! Scenes at runtime: scripts open another scene in place of the running
//! one, add a scene's objects next to what is there, or read a scene in the
//! background and switch once it is ready. Objects marked to keep across
//! scenes (and everything under them) stay through a switch.

use crate::Entity;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::mpsc;

/// The scene file a project opens when nothing else is named.
pub const MAIN_SCENE: &str = "scene.seal";

/// A scene being read in the background.
pub struct Loading {
    pub path: String,
    pub additive: bool,
    rx: mpsc::Receiver<Result<Vec<Entity>, String>>,
}

/// Which scenes make up the running play.
#[derive(Default)]
pub struct Scenes {
    /// The scene that is open, project-relative.
    pub open: String,
    /// The scene open when play began; stopping goes back to it.
    pub(crate) started: String,
    /// Added scenes and the ids of the objects each brought in.
    pub added: BTreeMap<String, Vec<String>>,
    /// Objects that stay through `open_scene`.
    pub kept: BTreeSet<String>,
    pub loading: Vec<Loading>,
}

impl Scenes {
    pub fn is_loaded(&self, path: &str) -> bool {
        let p = normal(path);
        (!p.is_empty() && normal(&self.open) == p) || self.added.contains_key(&p)
    }

    /// Every scene in the play: the open one first, then the added ones.
    pub fn loaded(&self) -> Vec<String> {
        let mut out = vec![self.open.clone()];
        out.extend(self.added.keys().cloned());
        out
    }

    pub(crate) fn begin(&mut self) {
        if self.open.trim().is_empty() {
            self.open = MAIN_SCENE.into();
        }
        self.started = self.open.clone();
        self.added.clear();
        self.kept.clear();
        self.loading.clear();
    }

    pub(crate) fn end(&mut self) {
        if !self.started.is_empty() {
            self.open = std::mem::take(&mut self.started);
        }
        self.added.clear();
        self.kept.clear();
        self.loading.clear();
    }

    /// Start reading `path` on another thread.
    pub fn load_async(&mut self, dir: &Path, path: &str, additive: bool) -> Result<(), String> {
        let rel = check(path)?;
        let (tx, rx) = mpsc::channel();
        let dir = dir.to_path_buf();
        let file = rel.clone();
        std::thread::spawn(move || {
            let _ = tx.send(read(&dir, &file));
        });
        self.loading.push(Loading { path: rel, additive, rx });
        Ok(())
    }

    /// Background reads that finished since the last call, in request order.
    pub(crate) fn finished(&mut self) -> Vec<(String, bool, Result<Vec<Entity>, String>)> {
        let mut done = Vec::new();
        self.loading.retain(|l| match l.rx.try_recv() {
            Ok(r) => {
                done.push((l.path.clone(), l.additive, r));
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => {
                done.push((l.path.clone(), l.additive, Err(format!("reading {} stopped", l.path))));
                false
            }
        });
        done
    }
}

/// Forward slashes, no leading `./`.
pub fn normal(path: &str) -> String {
    let p = path.trim().replace('\\', "/");
    p.trim_start_matches("./").to_string()
}

/// A scene path a script may name: a project-relative `.seal` file.
pub fn check(path: &str) -> Result<String, String> {
    let p = normal(path);
    if p.is_empty() {
        return Err("name a scene file to open, like scenes/level2.seal".into());
    }
    if p.starts_with('/') || p.contains(':') || p.split('/').any(|part| part == "..") {
        return Err(format!("{p}: scenes open from inside the project folder"));
    }
    if !p.ends_with(".seal") || p == "project.seal" {
        return Err(format!("{p} is not a scene file (scenes end in .seal)"));
    }
    Ok(p)
}

/// The objects of scene file `path` in project folder `dir`.
pub fn read(dir: &Path, path: &str) -> Result<Vec<Entity>, String> {
    let rel = check(path)?;
    let text = std::fs::read_to_string(dir.join(&rel))
        .map_err(|e| format!("cannot open the scene {rel}: {e}"))?;
    let world = crate::World::from_text(&text).map_err(|e| format!("the scene {rel}: {e}"))?;
    Ok(world.entities)
}

/// `id` and every object under it, through parents.
pub fn with_children(entities: &[Entity], ids: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = ids.clone();
    loop {
        let more: Vec<String> = entities
            .iter()
            .filter(|e| !out.contains(&e.id))
            .filter(|e| e.parent.as_ref().is_some_and(|p| out.contains(p)))
            .map(|e| e.id.clone())
            .collect();
        if more.is_empty() {
            return out;
        }
        out.extend(more);
    }
}

/// Give incoming objects ids nobody in `taken` has (`id@scene`, then numbered),
/// fixing their parents to match. Returns the ids they ended up with.
pub fn fit_ids(incoming: &mut [Entity], taken: &BTreeSet<String>, scene: &str) -> Vec<String> {
    let stem = Path::new(scene).file_stem().and_then(|s| s.to_str()).unwrap_or("scene").to_string();
    let mut used = taken.clone();
    let mut renamed = BTreeMap::new();
    for e in incoming.iter_mut() {
        if used.contains(&e.id) {
            let base = format!("{}@{stem}", e.id);
            let id = std::iter::once(base.clone())
                .chain((2..).map(|n| format!("{base}-{n}")))
                .find(|id| !used.contains(id))
                .unwrap_or(base);
            renamed.insert(e.id.clone(), id.clone());
            e.id = id;
        }
        used.insert(e.id.clone());
    }
    for e in incoming.iter_mut() {
        if let Some(p) = e.parent.as_ref().and_then(|p| renamed.get(p)) {
            e.parent = Some(p.clone());
        }
    }
    incoming.iter().map(|e| e.id.clone()).collect()
}
