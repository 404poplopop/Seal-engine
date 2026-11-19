//! HDR window choice and brightness, shared by every backend.
//!
//! A value of 1 in the scene is diffuse white. On an HDR surface that white is
//! drawn at the paper-white level. Values above 1 are brighter than paper white
//! and are kept. On an SDR surface they are clipped to 1, and paper white is
//! ignored, so an existing frame does not change.

/// HDR pixel layouts a window surface can offer, most precise first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdrBuffer {
    /// 16-bit float per channel. Holds values above 1.
    Rgba16Float,
    /// 10-bit colour. The display clips values above 1.
    Rgb10a2,
}

/// Which window runner to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerChoice {
    /// The ordinary window. `reason` is why the HDR window was not used.
    Standard { reason: &'static str },
    /// A window whose surface uses `format`.
    Hdr { format: HdrBuffer },
}

/// How displayed numbers relate to nits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputSpace {
    /// 1 is the brightest the surface can store. Paper white is not applied.
    Sdr,
    /// 1 is 203 nits before the paper-white scale.
    Hdr,
    /// Extended linear: 1 is 80 nits. Paper white sits above 1.
    ExtendedLinear,
}

/// Reference white used when the surface is not extended linear.
pub const REFERENCE_WHITE_NITS: f32 = 203.0;

/// What 1 means on an extended-linear surface.
pub const EXTENDED_LINEAR_WHITE_NITS: f32 = 80.0;

/// First HDR layout in `offered`, or none.
pub fn first_hdr(offered: &[HdrBuffer]) -> Option<HdrBuffer> {
    const ORDER: [HdrBuffer; 2] = [HdrBuffer::Rgba16Float, HdrBuffer::Rgb10a2];
    ORDER.into_iter().find(|candidate| offered.contains(candidate))
}

/// Paper white is stored from 80 to 400 nits.
pub fn clamp_paper_white_nits(value: f32) -> f32 {
    value.clamp(80.0, 400.0)
}

/// `extended_linear` matters only when `hdr` is set.
pub fn output_space(hdr: bool, extended_linear: bool) -> OutputSpace {
    if !hdr {
        OutputSpace::Sdr
    } else if extended_linear {
        OutputSpace::ExtendedLinear
    } else {
        OutputSpace::Hdr
    }
}

/// Scale that places scene white (1) on paper white. SDR stays at 1.
pub fn sdr_gain(space: OutputSpace, paper_white_nits: f32) -> f32 {
    let paper = clamp_paper_white_nits(paper_white_nits);
    match space {
        OutputSpace::Sdr => 1.0,
        OutputSpace::Hdr => paper / REFERENCE_WHITE_NITS,
        OutputSpace::ExtendedLinear => paper / EXTENDED_LINEAR_WHITE_NITS,
    }
}

/// One channel after the HDR or SDR rule.
///
/// On HDR, values above 1 stay above paper white. On SDR they become 1.
pub fn displayed_channel(linear: f32, space: OutputSpace, paper_white_nits: f32) -> f32 {
    let linear = if linear.is_nan() { 0.0 } else { linear };
    match space {
        OutputSpace::Sdr => linear.clamp(0.0, 1.0),
        OutputSpace::Hdr | OutputSpace::ExtendedLinear => {
            linear.max(0.0) * sdr_gain(space, paper_white_nits)
        }
    }
}

/// [`displayed_channel`] for red, green and blue.
pub fn displayed_rgb(
    linear: [f32; 3],
    space: OutputSpace,
    paper_white_nits: f32,
) -> [f32; 3] {
    linear.map(|channel| displayed_channel(channel, space, paper_white_nits))
}

/// Pick the game window. HDR is used only when it was asked for, the device
/// started, and the surface offered an HDR layout.
pub fn choose_runner(want_hdr: bool, offered: &[HdrBuffer], device_ready: bool) -> RunnerChoice {
    if !want_hdr {
        return RunnerChoice::Standard { reason: "hdr off" };
    }
    if !device_ready {
        return RunnerChoice::Standard { reason: "device failed" };
    }
    match first_hdr(offered) {
        Some(format) => RunnerChoice::Hdr { format },
        None => RunnerChoice::Standard { reason: "no hdr format" },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_stays_standard_when_hdr_is_off() {
        let choice = choose_runner(false, &[HdrBuffer::Rgba16Float], true);
        assert_eq!(choice, RunnerChoice::Standard { reason: "hdr off" });
    }

    #[test]
    fn runner_stays_standard_when_the_device_fails() {
        let choice = choose_runner(true, &[HdrBuffer::Rgba16Float], false);
        assert_eq!(choice, RunnerChoice::Standard { reason: "device failed" });
    }

    #[test]
    fn runner_stays_standard_when_no_hdr_format_is_offered() {
        let choice = choose_runner(true, &[], true);
        assert_eq!(choice, RunnerChoice::Standard { reason: "no hdr format" });
    }

    #[test]
    fn runner_prefers_the_float_format() {
        let choice = choose_runner(true, &[HdrBuffer::Rgb10a2, HdrBuffer::Rgba16Float], true);
        assert_eq!(choice, RunnerChoice::Hdr { format: HdrBuffer::Rgba16Float });
        let only = choose_runner(true, &[HdrBuffer::Rgb10a2], true);
        assert_eq!(only, RunnerChoice::Hdr { format: HdrBuffer::Rgb10a2 });
    }

    #[test]
    fn sdr_clips_and_ignores_paper_white() {
        let space = output_space(false, true);
        assert_eq!(space, OutputSpace::Sdr);
        assert_eq!(sdr_gain(space, 400.0), 1.0);
        assert_eq!(displayed_channel(0.25, space, 400.0), 0.25);
        assert_eq!(displayed_channel(1.0, space, 80.0), 1.0);
        assert_eq!(displayed_channel(2.5, space, 203.0), 1.0);
        assert_eq!(displayed_channel(-0.4, space, 203.0), 0.0);
        assert_eq!(displayed_channel(f32::NAN, space, 203.0), 0.0);
    }

    #[test]
    fn hdr_keeps_values_above_one_and_places_white_on_paper_white() {
        let space = output_space(true, false);
        assert_eq!(space, OutputSpace::Hdr);
        assert_eq!(displayed_channel(1.0, space, 203.0), 1.0);
        assert_eq!(displayed_channel(2.5, space, 203.0), 2.5);
        let bright = displayed_channel(1.0, space, 406.0);
        assert!((bright - 400.0 / REFERENCE_WHITE_NITS).abs() < 1e-5);
        assert!(displayed_channel(2.0, space, 400.0) > bright);
    }

    #[test]
    fn extended_linear_puts_paper_white_above_one() {
        let space = output_space(true, true);
        assert_eq!(space, OutputSpace::ExtendedLinear);
        let white = displayed_channel(1.0, space, 203.0);
        assert!((white - 203.0 / EXTENDED_LINEAR_WHITE_NITS).abs() < 1e-5);
        assert!(white > 1.0);
        let face = displayed_channel(2.0, space, 203.0);
        assert!((face - white * 2.0).abs() < 1e-4);
        let rgb = displayed_rgb([1.0, 2.0, 0.0], space, 203.0);
        assert_eq!(rgb[0], white);
        assert_eq!(rgb[1], face);
        assert_eq!(rgb[2], 0.0);
    }

    #[test]
    fn paper_white_outside_the_range_is_clamped() {
        assert_eq!(clamp_paper_white_nits(10.0), 80.0);
        assert_eq!(clamp_paper_white_nits(900.0), 400.0);
        assert_eq!(clamp_paper_white_nits(203.0), 203.0);
    }
}
