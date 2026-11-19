//! The render graph.
//!
//! A frame is a list of passes, each declaring the textures it reads and
//! writes. Compiling the graph:
//! * drops passes whose results nobody uses (culling),
//! * checks that nothing reads a texture before a pass writes it,
//! * works out when each frame-local texture is first and last used, and
//!   lets textures whose lifetimes do not overlap share one device texture
//!   (aliasing), so a frame needs fewer textures.
//!
//! Passes run in the order they were added, so frames are deterministic.

use std::collections::HashMap;

use crate::device::{Device, TextureDesc, TextureId};

/// A texture as the graph sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(u32);

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// Lives for this frame only; the graph picks the device texture.
    Transient(TextureDesc),
    /// A device texture owned outside the graph, such as the window's image.
    Imported(TextureId),
}

struct Resource {
    name: String,
    kind: Kind,
}

type Exec<'a, D> = Box<dyn FnOnce(&mut PassContext<'_, D>) + 'a>;

struct Pass<'a, D> {
    name: String,
    zone: &'static str,
    reads: Vec<ResourceId>,
    writes: Vec<ResourceId>,
    side_effect: bool,
    exec: Exec<'a, D>,
}

/// Declares what a pass touches.
#[derive(Default)]
pub struct PassBuilder {
    reads: Vec<ResourceId>,
    writes: Vec<ResourceId>,
    side_effect: bool,
}

impl PassBuilder {
    pub fn read(&mut self, resource: ResourceId) -> &mut Self {
        if !self.reads.contains(&resource) {
            self.reads.push(resource);
        }
        self
    }

    pub fn write(&mut self, resource: ResourceId) -> &mut Self {
        if !self.writes.contains(&resource) {
            self.writes.push(resource);
        }
        self
    }

    /// Keep this pass even if nothing reads what it writes (read-backs, captures).
    pub fn side_effect(&mut self) -> &mut Self {
        self.side_effect = true;
        self
    }
}

/// What a running pass can reach.
pub struct PassContext<'d, D> {
    pub device: &'d mut D,
    textures: &'d HashMap<ResourceId, TextureId>,
}

impl<D> PassContext<'_, D> {
    /// The device texture behind a resource this pass declared.
    pub fn texture(&self, resource: ResourceId) -> TextureId {
        self.textures[&resource]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    /// A pass reads a frame-local texture no earlier pass wrote.
    ReadBeforeWrite { pass: String, resource: String },
    /// A pass names a resource from another graph.
    UnknownResource { pass: String },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadBeforeWrite { pass, resource } => {
                write!(f, "pass {pass} reads {resource} before any pass writes it")
            }
            Self::UnknownResource { pass } => write!(f, "pass {pass} names an unknown resource"),
        }
    }
}

impl std::error::Error for GraphError {}

/// The result of compiling: which passes run, and which textures share memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledGraph {
    /// Indices of the passes that run, in order.
    pub order: Vec<usize>,
    /// Names of the passes that were culled.
    pub culled: Vec<String>,
    /// Physical slot for each used transient resource. Equal slots share a texture.
    pub slots: Vec<(ResourceId, usize)>,
    /// Description of each physical slot.
    pub slot_descs: Vec<TextureDesc>,
}

/// Device textures kept from frame to frame, so steady frames allocate nothing.
#[derive(Default)]
pub struct TexturePool {
    free: Vec<(TextureDesc, TextureId)>,
}

impl TexturePool {
    fn take<D: Device>(&mut self, device: &mut D, desc: TextureDesc) -> TextureId {
        match self.free.iter().position(|(d, _)| *d == desc) {
            Some(i) => self.free.remove(i).1,
            None => device.create_texture(desc),
        }
    }

    /// Textures waiting for reuse.
    pub fn len(&self) -> usize {
        self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.free.is_empty()
    }

    /// Give every pooled texture back to the device.
    pub fn release<D: Device>(&mut self, device: &mut D) {
        for (_, texture) in self.free.drain(..) {
            device.destroy_texture(texture);
        }
    }
}

pub struct RenderGraph<'a, D> {
    resources: Vec<Resource>,
    passes: Vec<Pass<'a, D>>,
}

impl<D> Default for RenderGraph<'_, D> {
    fn default() -> Self {
        Self { resources: Vec::new(), passes: Vec::new() }
    }
}

impl<'a, D: Device> RenderGraph<'a, D> {
    pub fn new() -> Self {
        Self::default()
    }

    /// A texture that lives for this frame only.
    pub fn create_texture(&mut self, name: &str, desc: TextureDesc) -> ResourceId {
        self.add(name, Kind::Transient(desc))
    }

    /// A texture owned outside the graph. Passes writing it always run.
    pub fn import(&mut self, name: &str, texture: TextureId) -> ResourceId {
        self.add(name, Kind::Imported(texture))
    }

    fn add(&mut self, name: &str, kind: Kind) -> ResourceId {
        self.resources.push(Resource { name: name.into(), kind });
        ResourceId(self.resources.len() as u32 - 1)
    }

    pub fn add_pass(
        &mut self,
        name: &str,
        setup: impl FnOnce(&mut PassBuilder),
        exec: impl FnOnce(&mut PassContext<'_, D>) + 'a,
    ) {
        let mut builder = PassBuilder::default();
        setup(&mut builder);
        self.passes.push(Pass {
            name: name.into(),
            zone: seal_trace::intern(name),
            reads: builder.reads,
            writes: builder.writes,
            side_effect: builder.side_effect,
            exec: Box::new(exec),
        });
    }

    fn imported(&self, r: ResourceId) -> bool {
        matches!(self.resources[r.0 as usize].kind, Kind::Imported(_))
    }

    pub fn compile(&self) -> Result<CompiledGraph, GraphError> {
        let count = self.resources.len() as u32;
        for pass in &self.passes {
            if pass.reads.iter().chain(&pass.writes).any(|r| r.0 >= count) {
                return Err(GraphError::UnknownResource { pass: pass.name.clone() });
            }
        }
        // Walk backwards: a pass is needed when it writes something a later
        // needed pass reads, writes an imported texture, or has a side effect.
        let mut needed_resources: Vec<bool> =
            (0..count).map(|i| self.imported(ResourceId(i))).collect();
        let mut keep = vec![false; self.passes.len()];
        for (index, pass) in self.passes.iter().enumerate().rev() {
            if pass.side_effect || pass.writes.iter().any(|r| needed_resources[r.0 as usize]) {
                keep[index] = true;
                for r in &pass.reads {
                    needed_resources[r.0 as usize] = true;
                }
            }
        }
        let order: Vec<usize> = (0..self.passes.len()).filter(|i| keep[*i]).collect();
        let culled =
            (0..self.passes.len()).filter(|i| !keep[*i]).map(|i| self.passes[i].name.clone());
        // Validate reads and find each transient's first and last use.
        let mut written = vec![false; count as usize];
        let mut span: HashMap<ResourceId, (usize, usize)> = HashMap::new();
        for (step, &index) in order.iter().enumerate() {
            let pass = &self.passes[index];
            for r in &pass.reads {
                if !self.imported(*r) && !written[r.0 as usize] {
                    return Err(GraphError::ReadBeforeWrite {
                        pass: pass.name.clone(),
                        resource: self.resources[r.0 as usize].name.clone(),
                    });
                }
            }
            for r in pass.reads.iter().chain(&pass.writes) {
                if !self.imported(*r) {
                    span.entry(*r).and_modify(|s| s.1 = step).or_insert((step, step));
                }
            }
            for r in &pass.writes {
                written[r.0 as usize] = true;
            }
        }
        // Aliasing: in order of first use, reuse a slot of the same
        // description that is free by then.
        let mut by_first: Vec<(ResourceId, (usize, usize))> = span.into_iter().collect();
        by_first.sort_by_key(|(r, (first, _))| (*first, *r));
        let mut slot_descs: Vec<TextureDesc> = Vec::new();
        let mut slot_free_after: Vec<usize> = Vec::new();
        let mut slots = Vec::new();
        for (r, (first, last)) in by_first {
            let Kind::Transient(desc) = self.resources[r.0 as usize].kind else { unreachable!() };
            let slot = (0..slot_descs.len())
                .find(|s| slot_descs[*s] == desc && slot_free_after[*s] < first)
                .unwrap_or_else(|| {
                    slot_descs.push(desc);
                    slot_free_after.push(0);
                    slot_descs.len() - 1
                });
            slot_free_after[slot] = last;
            slots.push((r, slot));
        }
        slots.sort();
        Ok(CompiledGraph { order, culled: culled.collect(), slots, slot_descs })
    }

    /// Compile and run the frame. Frame-local textures come from `pool` and go back to it.
    pub fn execute(
        self,
        device: &mut D,
        pool: &mut TexturePool,
    ) -> Result<CompiledGraph, GraphError> {
        let compiled = self.compile()?;
        let physical: Vec<TextureId> =
            compiled.slot_descs.iter().map(|desc| pool.take(device, *desc)).collect();
        let mut textures: HashMap<ResourceId, TextureId> = HashMap::new();
        for (index, resource) in self.resources.iter().enumerate() {
            if let Kind::Imported(texture) = resource.kind {
                textures.insert(ResourceId(index as u32), texture);
            }
        }
        for (r, slot) in &compiled.slots {
            textures.insert(*r, physical[*slot]);
        }
        let mut passes: Vec<Option<Pass<'a, D>>> = self.passes.into_iter().map(Some).collect();
        for &index in &compiled.order {
            let pass = passes[index].take().expect("each pass runs once");
            let _zone = seal_trace::Zone::begin(seal_trace::Channel::CPU, pass.zone);
            let mut ctx = PassContext { device, textures: &textures };
            (pass.exec)(&mut ctx);
        }
        for (desc, texture) in compiled.slot_descs.iter().zip(physical) {
            pool.free.push((*desc, texture));
        }
        Ok(compiled)
    }
}
