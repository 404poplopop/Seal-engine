//! Project and play settings for the 2D physics stepper. Defaults match behaviour
//! before these settings existed.

use rapier2d::prelude::IntegrationParameters;

/// Tunables read from project settings (or defaults). All values are validated on load.
#[derive(Clone, Debug, PartialEq)]
pub struct Physics2DSettings {
    /// Fixed simulation rate in hertz.
    pub fixed_hz: f32,
    /// Most fixed steps allowed per rendered frame.
    pub max_substeps: u32,
    pub velocity_iterations: u32,
    pub position_iterations: u32,
    /// Scene gravity multiplier applied with `SceneSettings.gravity` (1 leaves scene gravity as today).
    pub gravity_scale: f32,
    /// Draw dynamic bodies partway between the last and next fixed step.
    pub render_interpolation: bool,
    pub sleeping: bool,
    pub sleep_linear_threshold: f32,
    pub sleep_angular_threshold: f32,
}

impl Default for Physics2DSettings {
    fn default() -> Self {
        let integration = IntegrationParameters::default();
        Self {
            fixed_hz: 60.0,
            max_substeps: 5,
            velocity_iterations: integration.num_solver_iterations as u32,
            position_iterations: 0,
            gravity_scale: 1.0,
            render_interpolation: false,
            sleeping: true,
            sleep_linear_threshold: 0.05,
            sleep_angular_threshold: 0.05,
        }
    }
}

impl Physics2DSettings {
    pub fn fixed_step(&self) -> f32 {
        if self.fixed_hz.is_finite() && self.fixed_hz > 0.0 {
            1.0 / self.fixed_hz
        } else {
            1.0 / 60.0
        }
    }

    pub fn apply_integration(&self, params: &mut IntegrationParameters) {
        let total = self
            .velocity_iterations
            .saturating_add(self.position_iterations)
            .max(1);
        params.num_solver_iterations = total as usize;
    }

    pub fn from_project_settings(settings: &std::collections::BTreeMap<String, String>) -> Self {
        let d = Self::default();
        let f32 = |id: &str, fallback: f32| {
            settings
                .get(id)
                .and_then(|t| t.parse().ok())
                .filter(|v: &f32| v.is_finite())
                .unwrap_or(fallback)
        };
        let u32 = |id: &str, fallback: u32| {
            settings
                .get(id)
                .and_then(|t| t.parse().ok())
                .filter(|v: &u32| *v > 0 && *v < 1000)
                .unwrap_or(fallback)
        };
        let on = |id: &str, fallback: bool| match settings.get(id).map(String::as_str) {
            Some("true") => true,
            Some("false") => false,
            _ => fallback,
        };
        Self {
            fixed_hz: f32("physics2d.fixed_hz", d.fixed_hz),
            max_substeps: u32("physics2d.max_substeps", d.max_substeps),
            velocity_iterations: u32("physics2d.velocity_iterations", d.velocity_iterations),
            position_iterations: u32("physics2d.position_iterations", d.position_iterations),
            gravity_scale: f32("physics2d.gravity_scale", d.gravity_scale),
            render_interpolation: on("physics2d.render_interpolation", d.render_interpolation),
            sleeping: on("physics2d.sleeping", d.sleeping),
            sleep_linear_threshold: f32("physics2d.sleep_linear_threshold", d.sleep_linear_threshold),
            sleep_angular_threshold: f32("physics2d.sleep_angular_threshold", d.sleep_angular_threshold),
        }
    }
}

/// Counters and timings from the last frame of play, for the profiler panel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Physics2DProfiler {
    pub bodies: usize,
    pub colliders: usize,
    pub contacts: usize,
    pub joints: usize,
    pub step_ms: f32,
    pub step_history: Vec<f32>,
}

impl Physics2DProfiler {
    pub const HISTORY: usize = 120;

    pub fn record_step(&mut self, ms: f32) {
        self.step_ms = ms;
        self.step_history.push(ms);
        if self.step_history.len() > Self::HISTORY {
            let drop = self.step_history.len() - Self::HISTORY;
            self.step_history.drain(0..drop);
        }
    }
}