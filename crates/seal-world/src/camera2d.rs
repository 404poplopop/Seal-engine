//! The 2D play camera: follows an object with smoothing, keeps a drag-margin
//! dead zone, stays inside its limits, shakes on request and snaps to the
//! sprite pixel grid.

use seal_scene::Camera2d;

/// One running shake: how far it throws the view, and its time left of total.
#[derive(Clone, Debug, PartialEq)]
struct Shake {
    strength: f32,
    left: f32,
    total: f32,
}

/// Moving camera state during play. `pos` is `None` until the first step,
/// when the view starts right on its target instead of sliding in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rig {
    pub pos: Option<[f32; 2]>,
    shakes: Vec<Shake>,
    time: f32,
}

/// What the view should show this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    /// Scene point at the middle of the view, before limits and shake.
    pub center: [f32; 2],
    pub zoom: f32,
    /// Left, right, bottom, top, when the camera has limits.
    pub limits: Option<[f32; 4]>,
    /// Sprite pixels per unit to snap to; 0 for none.
    pub pixel_snap: f32,
}

impl Rig {
    /// Start a shake that throws the view up to `strength` units and fades out
    /// over `seconds`. Shakes add up while they overlap.
    pub fn shake(&mut self, strength: f32, seconds: f32) {
        if strength > 0.0 && seconds > 0.0 && strength.is_finite() && seconds.is_finite() {
            self.shakes.push(Shake {
                strength: strength.min(100.0),
                left: seconds,
                total: seconds,
            });
        }
    }

    /// Move toward `target` by one play step and age the shakes.
    ///
    /// Drag margins are ignored here. Play passes the view size to
    /// [`Rig::step_in_view`].
    pub fn step(&mut self, target: [f32; 2], cam: &Camera2d, dt: f32) {
        self.step_in_view(target, cam, [0.0, 0.0], dt);
    }

    /// Move toward `target`, holding still while it stays inside the drag
    /// margins. `view_half` is half the view width and height in scene units.
    /// Margins of zero follow `target` the same way [`Rig::step`] does.
    pub fn step_in_view(&mut self, target: [f32; 2], cam: &Camera2d, view_half: [f32; 2], dt: f32) {
        let dt = dt.max(0.0);
        self.time += dt;
        let current = *self.pos.get_or_insert(target);
        let goal = crate::cam_drag::desired_center(current, target, cam.drag, view_half);
        let follow = if cam.smoothing <= 0.0 { 1.0 } else { 1.0 - (-dt / cam.smoothing).exp() };
        let pos = self.pos.as_mut().unwrap();
        for i in 0..2 {
            pos[i] += (goal[i] - pos[i]) * follow;
        }
        for s in &mut self.shakes {
            s.left -= dt;
        }
        self.shakes.retain(|s| s.left > 0.0);
    }

    /// Age shakes only, for play without a 2D camera object.
    pub fn step_shakes(&mut self, dt: f32) {
        self.time += dt.max(0.0);
        for s in &mut self.shakes {
            s.left -= dt.max(0.0);
        }
        self.shakes.retain(|s| s.left > 0.0);
    }

    /// The view for `cam`, when the rig has started.
    pub fn view(&self, cam: &Camera2d) -> Option<View> {
        let center = self.pos?;
        let limits = (cam.limits != [0.0; 4]).then_some(cam.limits);
        Some(View {
            center,
            zoom: cam.zoom.clamp(0.05, 50.0),
            limits,
            pixel_snap: cam.pixel_snap.max(0.0),
        })
    }

    /// How far the shakes push the view right now, in scene units. The wobble is
    /// worked out from play time, so the same play shakes the same way every run.
    pub fn shake_offset(&self) -> [f32; 2] {
        let t = self.time;
        let amount: f32 = self.shakes.iter().map(|s| s.strength * (s.left / s.total)).sum();
        if amount == 0.0 {
            return [0.0; 2];
        }
        let x = (t * 47.0).sin() * 0.6 + (t * 89.0 + 1.3).sin() * 0.4;
        let y = (t * 53.0 + 0.7).cos() * 0.6 + (t * 97.0 + 2.1).sin() * 0.4;
        [x * amount, y * amount]
    }

    pub fn clear(&mut self) {
        *self = Rig::default();
    }
}

/// Keep a view of `half` (half width, half height) inside `limits`. When the
/// limits are narrower than the view, the view centres on them.
pub fn clamp_center(center: [f32; 2], half: [f32; 2], limits: Option<[f32; 4]>) -> [f32; 2] {
    let Some([l, r, b, t]) = limits else { return center };
    let fit = |c: f32, lo: f32, hi: f32, h: f32| {
        if hi - lo <= 2.0 * h {
            (lo + hi) * 0.5
        } else {
            c.clamp(lo + h, hi - h)
        }
    };
    [fit(center[0], l, r, half[0]), fit(center[1], b, t, half[1])]
}

/// Round a view centre onto the sprite pixel grid (`pixels` per unit).
pub fn snap_center(center: [f32; 2], pixels: f32) -> [f32; 2] {
    if pixels <= 0.0 {
        return center;
    }
    center.map(|c| (c * pixels).round() / pixels)
}

/// Screen pixels per scene unit adjusted so each sprite pixel covers a whole
/// number of screen pixels (at least one).
pub fn snap_scale(screen_per_unit: f32, pixels: f32) -> f32 {
    if pixels <= 0.0 {
        return screen_per_unit;
    }
    (screen_per_unit / pixels).round().max(1.0) * pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothing_eases_in_and_zero_snaps() {
        let mut cam = Camera2d { on: true, smoothing: 0.5, ..Default::default() };
        let mut rig = Rig::default();
        rig.step([0.0, 0.0], &cam, 0.016);
        rig.step([10.0, 0.0], &cam, 0.5);
        let x = rig.pos.unwrap()[0];
        assert!((x - 10.0 * (1.0 - (-1.0f32).exp())).abs() < 1e-3, "{x}");
        cam.smoothing = 0.0;
        rig.step([10.0, 4.0], &cam, 0.016);
        assert_eq!(rig.pos, Some([10.0, 4.0]));
    }

    #[test]
    fn limits_hold_the_view_inside() {
        let lim = Some([0.0, 20.0, 0.0, 10.0]);
        assert_eq!(clamp_center([1.0, 5.0], [4.0, 3.0], lim), [4.0, 5.0]);
        assert_eq!(clamp_center([30.0, -2.0], [4.0, 3.0], lim), [16.0, 3.0]);
        assert_eq!(clamp_center([3.0, 3.0], [12.0, 3.0], lim), [10.0, 3.0]);
        assert_eq!(clamp_center([3.0, 3.0], [12.0, 3.0], None), [3.0, 3.0]);
    }

    #[test]
    fn shakes_fade_out_and_repeat_exactly() {
        let mut a = Rig::default();
        a.shake(0.5, 0.4);
        let mut b = a.clone();
        a.step_shakes(0.1);
        b.step_shakes(0.1);
        let o = a.shake_offset();
        assert_eq!(o, b.shake_offset());
        assert!(o[0].abs() <= 0.5 && o[1].abs() <= 0.5 && (o[0] != 0.0 || o[1] != 0.0));
        a.step_shakes(0.4);
        assert_eq!(a.shake_offset(), [0.0, 0.0]);
    }

    #[test]
    fn drag_margins_hold_inside_and_move_by_the_overshoot() {
        let half = [5.0, 4.0];
        let mut cam = Camera2d {
            on: true,
            smoothing: 0.0,
            drag: [0.2, 0.2, 0.25, 0.25],
            ..Default::default()
        };
        let mut rig = Rig::default();
        rig.step_in_view([0.0, 0.0], &cam, half, 0.016);
        rig.step_in_view([1.0, 0.5], &cam, half, 0.016);
        assert_eq!(rig.pos, Some([0.0, 0.0]), "inside the dead zone the camera stays");
        // View is 10 wide. 0.2 of that is 2, so a target at 4 is 2 past the edge.
        rig.step_in_view([4.0, 0.0], &cam, half, 0.016);
        assert_eq!(rig.pos, Some([2.0, 0.0]));
        // View is 8 tall. 0.25 of that is 2. A target at -5 is 3 past the bottom edge.
        rig.step_in_view([2.0, -5.0], &cam, half, 0.016);
        assert_eq!(rig.pos, Some([2.0, -3.0]));

        cam.smoothing = 0.5;
        cam.drag = [0.2, 0.2, 0.0, 0.0];
        let follow = 1.0 - (-1.0f32).exp();
        rig.step_in_view([10.0, -3.0], &cam, half, 0.5);
        let x = rig.pos.unwrap()[0];
        // Slack is still 2, so the centre wants to be at 8, and smoothing covers part of that.
        assert!((x - (2.0 + 6.0 * follow)).abs() < 1e-3, "{x}");

        cam.drag = [0.0; 4];
        let mut plain = Rig::default();
        let mut margined = Rig::default();
        plain.step([0.0, 0.0], &cam, 0.016);
        margined.step_in_view([0.0, 0.0], &cam, half, 0.016);
        plain.step([10.0, 4.0], &cam, 0.5);
        margined.step_in_view([10.0, 4.0], &cam, half, 0.5);
        assert_eq!(plain.pos, margined.pos, "zero margins follow the target as before");
    }

    #[test]
    fn pixel_snap_rounds_to_the_grid() {
        assert_eq!(snap_center([1.03, -0.49], 16.0), [1.0, -0.5]);
        assert_eq!(snap_scale(40.0, 16.0), 48.0);
        assert_eq!(snap_scale(5.0, 16.0), 16.0);
        assert_eq!(snap_scale(40.0, 0.0), 40.0);
    }
}
