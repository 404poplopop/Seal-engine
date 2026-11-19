//! Colour grade for a flat picture: tint, exposure and saturation.
//!
//! White tint, exposure 1 and saturation 1 leave every pixel unchanged.

/// Settings for [`apply`]. Identity values leave the picture byte for byte.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PostGrade {
    pub tint: [f32; 3],
    pub exposure: f32,
    pub saturation: f32,
}

impl Default for PostGrade {
    fn default() -> Self {
        Self { tint: [1.0, 1.0, 1.0], exposure: 1.0, saturation: 1.0 }
    }
}

impl PostGrade {
    pub fn is_identity(self) -> bool {
        self.tint == [1.0, 1.0, 1.0]
            && (self.exposure - 1.0).abs() < 1e-6
            && (self.saturation - 1.0).abs() < 1e-6
    }
}

fn grade_channel(rgb: [f32; 3], grade: PostGrade) -> [f32; 3] {
    let c = [
        rgb[0] * grade.tint[0] * grade.exposure,
        rgb[1] * grade.tint[1] * grade.exposure,
        rgb[2] * grade.tint[2] * grade.exposure,
    ];
    let luma = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    [
        luma + (c[0] - luma) * grade.saturation,
        luma + (c[1] - luma) * grade.saturation,
        luma + (c[2] - luma) * grade.saturation,
    ]
}

/// Grade an RGBA8 picture. Identity settings return the same bytes.
pub fn apply(rgba: &[u8], grade: PostGrade) -> Vec<u8> {
    if grade.is_identity() || rgba.len() < 4 {
        return rgba.to_vec();
    }
    let mut out = rgba.to_vec();
    let pixels = out.len() / 4;
    for i in 0..pixels {
        let p = i * 4;
        let rgb = [out[p] as f32 / 255.0, out[p + 1] as f32 / 255.0, out[p + 2] as f32 / 255.0];
        let next = grade_channel(rgb, grade);
        for channel in 0..3 {
            out[p + channel] = (next[channel].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_grade_keeps_every_byte() {
        let src = [10u8, 20, 30, 255, 1, 2, 3, 128];
        assert_eq!(apply(&src, PostGrade::default()), src);
    }

    #[test]
    fn a_red_tint_drops_the_other_channels() {
        let src = [80u8, 80, 80, 255];
        let graded = apply(&src, PostGrade { tint: [1.0, 0.0, 0.0], ..PostGrade::default() });
        assert!(graded[0] > 0);
        assert_eq!(graded[1], 0);
        assert_eq!(graded[2], 0);
        assert_eq!(graded[3], 255);
    }
}
