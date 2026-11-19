//! The frame: named phases run in a fixed order, systems ordered inside each
//! phase by declared dependencies, and a fixed-step simulation phase that runs
//! zero or more times per frame.
//!
//! A class Tick in the class vocabulary is sugar: [`Schedule::add_tick`]
//! and [`Schedule::add_class_tick`] generate an ordinary system that visits
//! every entity of that kind.
//!
//! Debug builds can shuffle the order of systems that declare no dependency
//! on each other ([`Schedule::shuffle_independent`]), which shakes out
//! systems that silently rely on an order nobody wrote down.

use std::sync::Arc;

use crate::prefab::{PrefabInstance, PrefabLibrary};
use crate::world::{Entity, World};
use crate::Component;

/// Frame phases, in run order. FixedSimulation runs 0..N times per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    Input,
    PreSimulation,
    FixedSimulation,
    PostSimulation,
    Animation,
    PhysicsWriteBack,
    LateUpdate,
    Extract,
}

impl Phase {
    pub const ALL: [Phase; 8] = [
        Phase::Input,
        Phase::PreSimulation,
        Phase::FixedSimulation,
        Phase::PostSimulation,
        Phase::Animation,
        Phase::PhysicsWriteBack,
        Phase::LateUpdate,
        Phase::Extract,
    ];
}

const PHASE_NAMES: [&str; 8] = [
    "Input",
    "PreSimulation",
    "FixedSimulation",
    "PostSimulation",
    "Animation",
    "PhysicsWriteBack",
    "LateUpdate",
    "Extract",
];

/// Frame timing, kept as a world resource by [`Schedule::run_frame`].
#[derive(Clone, Debug, PartialEq)]
pub struct Time {
    /// Scaled seconds this frame (0 while paused).
    pub delta: f64,
    /// Seconds per fixed simulation step.
    pub fixed_step: f64,
    /// Most fixed steps in one frame; extra backlog is dropped, not carried.
    pub max_steps: u32,
    /// Multiplies real time (time dilation). 1 is normal speed.
    pub scale: f64,
    pub paused: bool,
    /// Unsimulated seconds left over after this frame's fixed steps.
    pub accumulator: f64,
    /// accumulator / fixed_step: how far to blend rendered transforms toward the next step.
    pub alpha: f64,
    pub steps_this_frame: u32,
    pub frame: u64,
    pub fixed_frame: u64,
    /// True while FixedSimulation systems run; ticks then see fixed_step as their dt.
    pub in_fixed: bool,
}

impl Default for Time {
    fn default() -> Self {
        Time {
            delta: 0.0,
            fixed_step: 1.0 / 60.0,
            max_steps: 8,
            scale: 1.0,
            paused: false,
            accumulator: 0.0,
            alpha: 0.0,
            steps_this_frame: 0,
            frame: 0,
            fixed_frame: 0,
            in_fixed: false,
        }
    }
}

impl Time {
    /// The dt a system should use right now.
    pub fn dt(&self) -> f64 {
        if self.in_fixed {
            self.fixed_step
        } else {
            self.delta
        }
    }
}

type SystemFn = Box<dyn FnMut(&mut World) + Send>;

struct System {
    name: String,
    /// The name as a trace zone label.
    zone: &'static str,
    phase: Phase,
    after: Vec<String>,
    before: Vec<String>,
    run: SystemFn,
}

/// Systems grouped by phase, in a checked order.
#[derive(Default)]
pub struct Schedule {
    systems: Vec<System>,
    order: Option<Vec<(Phase, Vec<usize>)>>,
    shuffle: Option<u64>,
}

/// Returned by [`Schedule::add_system`] to declare ordering.
pub struct SystemConfig<'a> {
    system: &'a mut System,
}

impl SystemConfig<'_> {
    /// Run after the named system (same or earlier phase).
    pub fn after(self, name: &str) -> Self {
        self.system.after.push(name.to_string());
        self
    }
    /// Run before the named system (same or later phase).
    pub fn before(self, name: &str) -> Self {
        self.system.before.push(name.to_string());
        self
    }
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Schedule {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_system(
        &mut self,
        name: &str,
        phase: Phase,
        run: impl FnMut(&mut World) + Send + 'static,
    ) -> SystemConfig<'_> {
        self.order = None;
        self.systems.push(System {
            name: name.into(),
            zone: seal_trace::intern(name),
            phase,
            after: Vec::new(),
            before: Vec::new(),
            run: Box::new(run),
        });
        SystemConfig { system: self.systems.last_mut().expect("just pushed") }
    }

    /// Tick sugar: call `tick(world, entity, dt)` for every entity with `M`.
    pub fn add_tick<M: Component>(
        &mut self,
        name: &str,
        phase: Phase,
        mut tick: impl FnMut(&mut World, Entity, f64) + Send + 'static,
    ) -> SystemConfig<'_> {
        self.add_system(name, phase, move |world| {
            let dt = world.resource::<Time>().map_or(0.0, Time::dt);
            for entity in world.query::<Entity>().with::<M>().collect(|e| e) {
                if world.contains(entity) {
                    tick(world, entity, dt);
                }
            }
        })
    }

    /// Class Tick in the class vocabulary: call `tick` for every
    /// instance of `class` or of any prefab that extends it.
    pub fn add_class_tick(
        &mut self,
        name: &str,
        phase: Phase,
        library: Arc<PrefabLibrary>,
        class: &str,
        mut tick: impl FnMut(&mut World, Entity, f64) + Send + 'static,
    ) -> SystemConfig<'_> {
        let class = class.to_string();
        let mut cache: Vec<(String, bool)> = Vec::new();
        self.add_system(name, phase, move |world| {
            let dt = world.resource::<Time>().map_or(0.0, Time::dt);
            let instances =
                world.query::<(Entity, &PrefabInstance)>().collect(|(e, p)| (e, p.0.clone()));
            for (entity, prefab) in instances {
                let matches = match cache.iter().find(|(n, _)| *n == prefab) {
                    Some((_, m)) => *m,
                    None => {
                        let m = library.is_a(&prefab, &class);
                        cache.push((prefab, m));
                        m
                    }
                };
                if matches && world.contains(entity) {
                    tick(world, entity, dt);
                }
            }
        })
    }

    /// Shuffle systems that have no declared order between them (debug aid).
    /// The same seed always gives the same order.
    pub fn shuffle_independent(&mut self, seed: Option<u64>) {
        self.shuffle = seed;
        self.order = None;
    }

    /// Check names and dependencies and fix the run order.
    pub fn build(&mut self) -> Result<(), String> {
        let index_of = |name: &str| self.systems.iter().position(|s| s.name == name);
        for (i, s) in self.systems.iter().enumerate() {
            if self.systems[..i].iter().any(|o| o.name == s.name) {
                return Err(format!("two systems are named {}", s.name));
            }
        }
        // Edges inside a phase; across phases the phase order decides, and a
        // dependency that contradicts it is an error.
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for (i, s) in self.systems.iter().enumerate() {
            for (dep, first_is_dep) in
                s.after.iter().map(|d| (d, true)).chain(s.before.iter().map(|d| (d, false)))
            {
                let j =
                    index_of(dep).ok_or(format!("{} depends on unknown system {dep}", s.name))?;
                let (first, second) = if first_is_dep { (j, i) } else { (i, j) };
                let (pf, ps) = (self.systems[first].phase, self.systems[second].phase);
                if pf == ps {
                    edges.push((first, second));
                } else if pf > ps {
                    return Err(format!(
                        "{} must run before {}, but its phase {pf:?} comes after {ps:?}",
                        self.systems[first].name, self.systems[second].name
                    ));
                }
            }
        }
        let mut rng = self.shuffle.unwrap_or(0);
        let mut order = Vec::new();
        for phase in Phase::ALL {
            let members: Vec<usize> =
                (0..self.systems.len()).filter(|&i| self.systems[i].phase == phase).collect();
            let mut indegree: Vec<usize> =
                members.iter().map(|&m| edges.iter().filter(|(_, b)| *b == m).count()).collect();
            let mut done = vec![false; members.len()];
            let mut out = Vec::new();
            while out.len() < members.len() {
                let ready: Vec<usize> =
                    (0..members.len()).filter(|&k| !done[k] && indegree[k] == 0).collect();
                if ready.is_empty() {
                    let stuck: Vec<&str> = (0..members.len())
                        .filter(|&k| !done[k])
                        .map(|k| self.systems[members[k]].name.as_str())
                        .collect();
                    return Err(format!(
                        "systems depend on each other in a loop: {}",
                        stuck.join(", ")
                    ));
                }
                let pick = match self.shuffle {
                    Some(_) => ready[(splitmix(&mut rng) % ready.len() as u64) as usize],
                    None => ready[0],
                };
                done[pick] = true;
                out.push(members[pick]);
                for (a, b) in &edges {
                    if *a == members[pick] {
                        if let Some(k) = members.iter().position(|m| m == b) {
                            indegree[k] -= 1;
                        }
                    }
                }
            }
            order.push((phase, out));
        }
        self.order = Some(order);
        Ok(())
    }

    /// System names in run order for one phase.
    pub fn order(&mut self, phase: Phase) -> Result<Vec<&str>, String> {
        if self.order.is_none() {
            self.build()?;
        }
        let order = self.order.as_ref().expect("built");
        let list = &order.iter().find(|(p, _)| *p == phase).expect("every phase").1;
        Ok(list.iter().map(|&i| self.systems[i].name.as_str()).collect())
    }

    fn run_phase(&mut self, phase: Phase, world: &mut World) {
        let order = self.order.as_ref().expect("built");
        let list = order.iter().find(|(p, _)| *p == phase).expect("every phase").1.clone();
        let _phase = seal_trace::zone!(seal_trace::Channel::CPU, PHASE_NAMES[phase as usize]);
        for i in list {
            let system = &mut self.systems[i];
            let _zone = seal_trace::zone!(seal_trace::Channel::CPU, system.zone);
            (system.run)(world);
        }
    }

    /// Run one frame of `real_dt` seconds. Returns how many fixed steps ran.
    pub fn run_frame(&mut self, world: &mut World, real_dt: f64) -> Result<u32, String> {
        if self.order.is_none() {
            self.build()?;
        }
        if world.resource::<Time>().is_none() {
            world.insert_resource(Time::default());
        }
        {
            let t = world.resource_mut::<Time>().expect("inserted");
            t.delta = if t.paused { 0.0 } else { real_dt.max(0.0) * t.scale };
            t.accumulator += t.delta;
            t.steps_this_frame = 0;
            t.in_fixed = false;
        }
        self.run_phase(Phase::Input, world);
        self.run_phase(Phase::PreSimulation, world);
        loop {
            let t = world.resource_mut::<Time>().expect("time");
            if t.fixed_step <= 0.0 || t.accumulator < t.fixed_step {
                break;
            }
            if t.steps_this_frame >= t.max_steps {
                // Too far behind: drop the backlog instead of spiralling.
                t.accumulator %= t.fixed_step;
                break;
            }
            t.accumulator -= t.fixed_step;
            t.steps_this_frame += 1;
            t.fixed_frame += 1;
            t.in_fixed = true;
            self.run_phase(Phase::FixedSimulation, world);
            world.resource_mut::<Time>().expect("time").in_fixed = false;
        }
        {
            let t = world.resource_mut::<Time>().expect("time");
            t.alpha = if t.fixed_step > 0.0 { t.accumulator / t.fixed_step } else { 0.0 };
        }
        for phase in &Phase::ALL[3..] {
            self.run_phase(*phase, world);
        }
        seal_trace::frame_mark();
        let t = world.resource_mut::<Time>().expect("time");
        t.frame += 1;
        Ok(t.steps_this_frame)
    }
}
