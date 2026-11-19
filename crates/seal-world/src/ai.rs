//! AI movement for play (16b): objects given orders by scripts walk along
//! grid paths around solid objects. Orders are kinematic: the walker's
//! position is set each step, which the physics step then takes over.

use crate::nav::NavGrid;
use crate::Entity;

/// An order in world terms (object ids, points), as scripts give it.
#[derive(Clone, Debug, PartialEq)]
pub enum Goal {
    MoveTo { target: [f32; 3], speed: f32 },
    Chase { target: String, speed: f32 },
    Patrol { a: [f32; 3], b: [f32; 3], speed: f32 },
    Stop,
}

/// One object following an order.
#[derive(Clone, Debug, PartialEq)]
pub struct Agent {
    pub id: String,
    pub goal: Goal,
    /// Remaining path points on the ground plane (x, z).
    path: Vec<(f32, f32)>,
    /// Patrol leg: heading to `b` when true.
    to_b: bool,
    /// Seconds until a chase plans again.
    replan: f32,
    /// Where the current path ends, to notice a moved chase target.
    planned_for: Option<(usize, usize)>,
}

/// Grid cell size for walking paths, in units.
pub const NAV_CELL: f32 = 0.5;
/// How often a chase re-plans, in seconds.
const CHASE_REPLAN: f32 = 0.25;

#[derive(Default)]
pub struct Ai {
    pub agents: Vec<Agent>,
    /// Objects whose move-to finished (or that are next to their chase target).
    pub arrived: std::collections::BTreeSet<String>,
}

fn half_extents(e: &Entity) -> [f32; 3] {
    [e.scale[0].abs() * 0.5, e.scale[1].abs() * 0.5, e.scale[2].abs() * 0.5]
}

/// Ground grid for `walker`: cells covered (with the walker's half width as
/// clearance) by solid, still, visible objects whose height overlaps the
/// walker's. Floors under the walker do not overlap, so they do not block.
/// The grid covers every object and the points in `also`, plus a margin.
pub fn grid_for(entities: &[Entity], walker: &Entity, also: &[[f32; 3]]) -> NavGrid {
    let wh = half_extents(walker);
    let (wy0, wy1) = (walker.translation[1] - wh[1], walker.translation[1] + wh[1]);
    let clearance = wh[0].max(wh[2]).min(2.0);
    let obstacles: Vec<([f32; 3], [f32; 3])> = entities
        .iter()
        .filter(|e| e.id != walker.id && e.shape.is_some() && !e.dynamic && !e.hidden)
        .filter_map(|e| {
            let h = half_extents(e);
            let (y0, y1) = (e.translation[1] - h[1], e.translation[1] + h[1]);
            // Small tolerance so a floor touching the walker's feet is ground, not wall.
            (y1 > wy0 + 0.05 && y0 < wy1 - 0.05).then(|| {
                (
                    [e.translation[0] - h[0], 0.0, e.translation[2] - h[2]],
                    [e.translation[0] + h[0], 0.0, e.translation[2] + h[2]],
                )
            })
        })
        .collect();
    let mut lo = [walker.translation[0], walker.translation[2]];
    let mut hi = lo;
    for p in entities.iter().map(|e| e.translation).chain(also.iter().copied()) {
        lo = [lo[0].min(p[0]), lo[1].min(p[2])];
        hi = [hi[0].max(p[0]), hi[1].max(p[2])];
    }
    let margin = 4.0;
    let origin = (lo[0] - margin, lo[1] - margin);
    let w = (((hi[0] - lo[0] + 2.0 * margin) / NAV_CELL).ceil() as usize).clamp(1, 512);
    let h = (((hi[1] - lo[1] + 2.0 * margin) / NAV_CELL).ceil() as usize).clamp(1, 512);
    let mut grid = NavGrid::new(w, h, NAV_CELL, origin);
    for (min, max) in obstacles {
        for y in 0..h {
            for x in 0..w {
                let (cx, cz) = grid.centre_of((x, y));
                if cx > min[0] - clearance
                    && cx < max[0] + clearance
                    && cz > min[2] - clearance
                    && cz < max[2] + clearance
                {
                    grid.set_blocked(x, y, true);
                }
            }
        }
    }
    grid
}

impl Ai {
    pub fn order(&mut self, id: &str, goal: Goal) {
        self.agents.retain(|a| a.id != id);
        self.arrived.remove(id);
        if goal != Goal::Stop {
            self.agents.push(Agent {
                id: id.to_string(),
                goal,
                path: Vec::new(),
                to_b: false,
                replan: 0.0,
                planned_for: None,
            });
        }
    }

    /// Moves every agent one step of `dt` seconds.
    pub fn step(&mut self, dt: f32, entities: &mut [Entity], log: &mut Vec<String>) {
        let mut done = Vec::new();
        for agent in &mut self.agents {
            let Some(me) = entities.iter().position(|e| e.id == agent.id) else {
                done.push(agent.id.clone());
                continue;
            };
            let (dest, speed) = match &agent.goal {
                Goal::MoveTo { target, speed } => (*target, *speed),
                Goal::Patrol { a, b, speed } => (if agent.to_b { *b } else { *a }, *speed),
                Goal::Chase { target, speed } => match entities.iter().find(|e| &e.id == target) {
                    Some(t) => (t.translation, *speed),
                    None => {
                        done.push(agent.id.clone());
                        continue;
                    }
                },
                Goal::Stop => {
                    done.push(agent.id.clone());
                    continue;
                }
            };
            let grid = grid_for(entities, &entities[me], &[dest]);
            let here = (entities[me].translation[0], entities[me].translation[2]);
            let goal_cell = grid.cell_of((dest[0], dest[2]));
            agent.replan -= dt;
            let chase = matches!(agent.goal, Goal::Chase { .. });
            if agent.path.is_empty()
                || (chase && (agent.replan <= 0.0 || agent.planned_for != goal_cell))
            {
                agent.replan = CHASE_REPLAN;
                agent.planned_for = goal_cell;
                match grid.find_path(here, (dest[0], dest[2])) {
                    Some(mut p) => {
                        // Skip the cell we stand in, and end on the exact point.
                        p.remove(0);
                        if !chase {
                            p.pop();
                            p.push((dest[0], dest[2]));
                        }
                        agent.path = p;
                    }
                    None => {
                        log.push(format!(
                            "{} has no path to {:.1}, {:.1}",
                            entities[me].name, dest[0], dest[2]
                        ));
                        done.push(agent.id.clone());
                        continue;
                    }
                }
            }
            if chase {
                let d = ((dest[0] - here.0).powi(2) + (dest[2] - here.1).powi(2)).sqrt();
                if d <= NAV_CELL * 2.0 + 0.5 {
                    self.arrived.insert(agent.id.clone());
                    agent.path.clear();
                    continue;
                }
                self.arrived.remove(&agent.id);
            }
            // Walk along the path, using up this step's distance.
            let mut budget = speed.max(0.0) * dt;
            let e = &mut entities[me];
            while budget > 0.0 {
                let Some(&(px, pz)) = agent.path.first() else { break };
                let (dx, dz) = (px - e.translation[0], pz - e.translation[2]);
                let d = (dx * dx + dz * dz).sqrt();
                if d <= budget {
                    e.translation[0] = px;
                    e.translation[2] = pz;
                    budget -= d;
                    agent.path.remove(0);
                } else {
                    e.translation[0] += dx / d * budget;
                    e.translation[2] += dz / d * budget;
                    budget = 0.0;
                }
            }
            e.velocity[0] = 0.0;
            e.velocity[2] = 0.0;
            if agent.path.is_empty() {
                match agent.goal {
                    Goal::MoveTo { .. } => {
                        self.arrived.insert(agent.id.clone());
                        done.push(agent.id.clone());
                    }
                    Goal::Patrol { .. } => agent.to_b = !agent.to_b,
                    _ => {}
                }
            }
        }
        self.agents.retain(|a| !done.contains(&a.id));
    }
}
