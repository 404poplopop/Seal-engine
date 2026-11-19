//! Frustum culling for instanced 3D drawing. The CPU path here is the
//! reference; `seal-render-wgpu` runs the same test as a compute pass and
//! writes indirect draw arguments. Both use the same planes, so they agree
//! except for instances that touch a plane within float rounding.

/// Row-major 4x4 matrix: `clip = m * [x, y, z, 1]`.
pub type Mat4 = [[f32; 4]; 4];

/// A world-space box around one instance, and which mesh it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub mesh: u32,
}

/// Six planes `(a, b, c, d)` with the inside where `a*x + b*y + c*z + d >= 0`:
/// left, right, bottom, top, near, far (depth 0..1, as in wgpu).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    pub planes: [[f32; 4]; 6],
}

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [[0.0; 4]; 4];
    for (r, row) in m.iter_mut().enumerate() {
        for (c, v) in row.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    m
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

/// Right-handed view-projection looking from `eye` at `target` with Z up,
/// vertical field of view `fov_y` in radians, depth 0..1.
pub fn view_projection(
    eye: [f32; 3],
    target: [f32; 3],
    fov_y: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> Mat4 {
    let f = norm(sub(target, eye));
    let up = if f[2].abs() > 0.999 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
    let s = norm(cross(f, up));
    let u = cross(s, f);
    let view = [
        [s[0], s[1], s[2], -dot(s, eye)],
        [u[0], u[1], u[2], -dot(u, eye)],
        [-f[0], -f[1], -f[2], dot(f, eye)],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let t = 1.0 / (fov_y / 2.0).tan();
    let proj = [
        [t / aspect, 0.0, 0.0, 0.0],
        [0.0, t, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), near * far / (near - far)],
        [0.0, 0.0, -1.0, 0.0],
    ];
    mul(&proj, &view)
}

impl Frustum {
    /// Planes of a view-projection (Gribb–Hartmann), normalised.
    pub fn from_matrix(m: &Mat4) -> Frustum {
        let row = |i: usize| m[i];
        let add = |a: [f32; 4], b: [f32; 4], s: f32| {
            [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2], a[3] + s * b[3]]
        };
        let w = row(3);
        let raw = [
            add(w, row(0), 1.0),
            add(w, row(0), -1.0),
            add(w, row(1), 1.0),
            add(w, row(1), -1.0),
            row(2),
            add(w, row(2), -1.0),
        ];
        let mut planes = [[0.0; 4]; 6];
        for (p, r) in planes.iter_mut().zip(raw) {
            let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
            *p = [r[0] / l, r[1] / l, r[2] / l, r[3] / l];
        }
        Frustum { planes }
    }

    /// The smallest signed distance of the box's farthest-inside corner to
    /// any plane. Negative means fully outside one plane (culled).
    pub fn margin(&self, min: [f32; 3], max: [f32; 3]) -> f32 {
        let mut worst = f32::INFINITY;
        for p in &self.planes {
            let x = if p[0] >= 0.0 { max[0] } else { min[0] };
            let y = if p[1] >= 0.0 { max[1] } else { min[1] };
            let z = if p[2] >= 0.0 { max[2] } else { min[2] };
            worst = worst.min(p[0] * x + p[1] * y + p[2] * z + p[3]);
        }
        worst
    }

    pub fn visible(&self, min: [f32; 3], max: [f32; 3]) -> bool {
        self.margin(min, max) >= 0.0
    }
}

/// One mesh's indexed draw, laid out like wgpu's `DrawIndexedIndirectArgs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshDraw {
    pub index_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
}

/// Result of culling: per mesh, the indirect draw (5 words: index count,
/// visible instance count, first index, base vertex, first instance) and the
/// visible instance ids, stored from `first_instance` in one shared list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Culled {
    pub args: Vec<[u32; 5]>,
    pub visible: Vec<u32>,
}

impl Culled {
    /// Visible instance ids of one mesh, sorted (GPU order is arbitrary).
    pub fn mesh_ids(&self, mesh: usize) -> Vec<u32> {
        let a = self.args[mesh];
        let mut v = self.visible[a[4] as usize..(a[4] + a[1]) as usize].to_vec();
        v.sort_unstable();
        v
    }
}

/// Where each mesh's slice of the visible list starts: the number of
/// instances of all earlier meshes (so every mesh fits even if all show).
pub fn mesh_bases(instances: &[Instance], meshes: usize) -> Vec<u32> {
    let mut count = vec![0u32; meshes];
    for i in instances {
        count[i.mesh as usize] += 1;
    }
    let mut base = 0;
    count
        .iter()
        .map(|c| {
            let b = base;
            base += c;
            b
        })
        .collect()
}

/// CPU reference culling.
pub fn cull_cpu(frustum: &Frustum, instances: &[Instance], meshes: &[MeshDraw]) -> Culled {
    let bases = mesh_bases(instances, meshes.len());
    let mut args: Vec<[u32; 5]> = meshes
        .iter()
        .zip(&bases)
        .map(|(m, b)| [m.index_count, 0, m.first_index, m.base_vertex as u32, *b])
        .collect();
    let mut visible = vec![0u32; instances.len()];
    for (id, i) in instances.iter().enumerate() {
        if frustum.visible(i.min, i.max) {
            let a = &mut args[i.mesh as usize];
            visible[(a[4] + a[1]) as usize] = id as u32;
            a[1] += 1;
        }
    }
    Culled { args, visible }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> Frustum {
        Frustum::from_matrix(&view_projection(
            [0.0, -10.0, 2.0],
            [0.0, 0.0, 0.0],
            1.0,
            1.5,
            0.1,
            100.0,
        ))
    }
    fn cube(c: [f32; 3], mesh: u32) -> Instance {
        Instance {
            min: [c[0] - 0.5, c[1] - 0.5, c[2] - 0.5],
            max: [c[0] + 0.5, c[1] + 0.5, c[2] + 0.5],
            mesh,
        }
    }

    #[test]
    fn boxes_in_front_show_and_behind_or_aside_do_not() {
        let f = cam();
        assert!(f.visible(cube([0.0, 0.0, 0.0], 0).min, cube([0.0, 0.0, 0.0], 0).max));
        for c in [[0.0, -20.0, 0.0], [60.0, 0.0, 0.0], [0.0, 0.0, 50.0], [0.0, 200.0, 0.0]] {
            let b = cube(c, 0);
            assert!(!f.visible(b.min, b.max), "{c:?}");
        }
        // Straddling the left edge still counts as visible.
        let edge = 10.0 * (0.5f32).tan() * 1.5;
        let b = cube([-edge, 0.0, 0.0], 0);
        assert!(f.visible(b.min, b.max));
    }

    #[test]
    fn draws_group_visible_instances_by_mesh() {
        let inst = vec![
            cube([0.0, 0.0, 0.0], 1),
            cube([0.0, -20.0, 0.0], 0),
            cube([1.0, 0.0, 0.0], 0),
            cube([-1.0, 1.0, 0.0], 1),
        ];
        let meshes = [
            MeshDraw { index_count: 36, first_index: 0, base_vertex: 0 },
            MeshDraw { index_count: 6, first_index: 36, base_vertex: 24 },
        ];
        let c = cull_cpu(&cam(), &inst, &meshes);
        assert_eq!(c.args[0], [36, 1, 0, 0, 0]);
        assert_eq!(c.args[1], [6, 2, 36, 24, 2]);
        assert_eq!(c.mesh_ids(0), vec![2]);
        assert_eq!(c.mesh_ids(1), vec![0, 3]);
    }
}
