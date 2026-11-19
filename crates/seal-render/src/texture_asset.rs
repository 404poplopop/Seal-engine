//! Composite texture assets (2D array, volume, cubemap) stored as SealText.

use std::fmt;

pub const TEXTURE_EXT: &str = "texture";
pub const HEADER: &str = "seal texture 1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextureAssetKind {
    #[default]
    Array2d,
    Volume3d,
    Cubemap,
}

impl TextureAssetKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "array" | "array2d" | "2d_array" => Some(Self::Array2d),
            "volume" | "volume3d" | "3d" => Some(Self::Volume3d),
            "cubemap" | "cube" => Some(Self::Cubemap),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Array2d => "array",
            Self::Volume3d => "volume",
            Self::Cubemap => "cubemap",
        }
    }

    pub fn layer_count(self, pictures: usize) -> usize {
        match self {
            Self::Array2d | Self::Volume3d => pictures,
            Self::Cubemap => pictures.max(6),
        }
    }

    pub fn expected_pictures(self) -> Option<usize> {
        match self {
            Self::Cubemap => Some(6),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TextureAssetError {
    Parse(String),
    MissingField(&'static str),
    BadPictureCount { kind: TextureAssetKind, got: usize },
}

impl fmt::Display for TextureAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(s) => write!(f, "{s}"),
            Self::MissingField(k) => write!(f, "missing field {k}"),
            Self::BadPictureCount { kind, got } => {
                let want = kind.expected_pictures().unwrap_or(1);
                write!(f, "{kind:?} needs at least {want} pictures, got {got}")
            }
        }
    }
}

impl std::error::Error for TextureAssetError {}

#[derive(Clone, Debug, PartialEq)]
pub struct TextureAsset {
    pub kind: TextureAssetKind,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub gpu_format: crate::gpu_block_format::GpuBlockFormat,
    pub memory_budget_mb: u32,
    pub streaming: bool,
    pub pictures: Vec<String>,
}

impl Default for TextureAsset {
    fn default() -> Self {
        Self {
            kind: TextureAssetKind::Array2d,
            width: 1,
            height: 1,
            depth: 1,
            gpu_format: crate::gpu_block_format::GpuBlockFormat::Rgba8,
            memory_budget_mb: 128,
            streaming: true,
            pictures: Vec::new(),
        }
    }
}

pub fn is_texture_asset_path(path: &str) -> bool {
    path.trim_end_matches(['/', '\\'])
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(TEXTURE_EXT))
}

impl TextureAsset {
    pub fn layer_count(&self) -> u32 {
        match self.kind {
            TextureAssetKind::Array2d => self.pictures.len().max(1) as u32,
            TextureAssetKind::Volume3d => self.depth.max(1),
            TextureAssetKind::Cubemap => 6,
        }
    }

    pub fn parse(text: &str) -> Result<Self, TextureAssetError> {
        let mut header_ok = false;
        let mut kind = TextureAssetKind::Array2d;
        let mut width = 1u32;
        let mut height = 1u32;
        let mut depth = 1u32;
        let mut gpu_format = crate::gpu_block_format::GpuBlockFormat::Rgba8;
        let mut memory_budget_mb = 128u32;
        let mut streaming = true;
        let mut pictures = Vec::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if !header_ok {
                if line != HEADER {
                    return Err(TextureAssetError::Parse(format!("line {}: bad header", n + 1)));
                }
                header_ok = true;
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(TextureAssetError::Parse(format!(
                    "line {}: expected key = value",
                    n + 1
                )));
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "kind" => {
                    kind = TextureAssetKind::parse(value)
                        .ok_or_else(|| TextureAssetError::Parse(format!("unknown kind {value}")))?;
                }
                "width" => {
                    width = value.parse().map_err(|_| TextureAssetError::Parse("width".into()))?;
                }
                "height" => {
                    height =
                        value.parse().map_err(|_| TextureAssetError::Parse("height".into()))?;
                }
                "depth" => {
                    depth = value.parse().map_err(|_| TextureAssetError::Parse("depth".into()))?;
                }
                "gpu_format" | "format" => {
                    gpu_format =
                        crate::gpu_block_format::GpuBlockFormat::parse(value).ok_or_else(|| {
                            TextureAssetError::Parse(format!("unknown gpu format {value}"))
                        })?;
                }
                "memory_budget_mb" => {
                    memory_budget_mb = value
                        .parse()
                        .map_err(|_| TextureAssetError::Parse("memory_budget_mb".into()))?;
                }
                "streaming" => {
                    streaming = matches!(value, "1" | "true" | "yes");
                }
                "pictures" => {
                    pictures = parse_string_list(value)?;
                }
                _ => {}
            }
        }
        if !header_ok {
            return Err(TextureAssetError::Parse("missing header".into()));
        }
        // Picture counts are checked by `validate_pictures`, so a new empty texture still opens.
        Ok(Self { kind, width, height, depth, gpu_format, memory_budget_mb, streaming, pictures })
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from(HEADER);
        out.push('\n');
        out.push_str(&format!("kind = {}\n", self.kind.name()));
        out.push_str(&format!("width = {}\n", self.width));
        out.push_str(&format!("height = {}\n", self.height));
        if self.kind == TextureAssetKind::Volume3d {
            out.push_str(&format!("depth = {}\n", self.depth));
        }
        out.push_str(&format!("gpu_format = {}\n", self.gpu_format.name()));
        out.push_str(&format!("memory_budget_mb = {}\n", self.memory_budget_mb));
        out.push_str(&format!("streaming = {}\n", if self.streaming { "yes" } else { "no" }));
        out.push_str("pictures = ");
        out.push_str(&string_list(&self.pictures));
        out.push('\n');
        out
    }

    pub fn validate_pictures(&self) -> Result<(), TextureAssetError> {
        if self.pictures.is_empty() {
            return Err(TextureAssetError::MissingField("pictures"));
        }
        if let Some(want) = self.kind.expected_pictures() {
            if self.pictures.len() < want {
                return Err(TextureAssetError::BadPictureCount {
                    kind: self.kind,
                    got: self.pictures.len(),
                });
            }
        }
        Ok(())
    }
}

fn parse_string_list(value: &str) -> Result<Vec<String>, TextureAssetError> {
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        return Err(TextureAssetError::Parse("pictures must be a list".into()));
    }
    let inner = &value[1..value.len() - 1];
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|s| {
            let s = s.trim();
            if (s.starts_with('"') && s.ends_with('"'))
                || (s.starts_with('\'') && s.ends_with('\''))
            {
                Ok(s[1..s.len() - 1].replace("\\\"", "\""))
            } else if s.is_empty() {
                Err(TextureAssetError::Parse("empty picture path".into()))
            } else {
                Ok(s.to_string())
            }
        })
        .collect()
}

fn string_list(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".into();
    }
    let quoted: Vec<String> =
        items.iter().map(|p| format!("\"{}\"", p.replace('"', "\\\""))).collect();
    format!("[{}]", quoted.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_array() {
        let asset = TextureAsset {
            kind: TextureAssetKind::Array2d,
            width: 32,
            height: 16,
            depth: 1,
            gpu_format: crate::gpu_block_format::GpuBlockFormat::Bc1,
            memory_budget_mb: 64,
            streaming: true,
            pictures: vec!["a.png".into(), "b.png".into()],
        };
        let text = asset.to_text();
        let back = TextureAsset::parse(&text).unwrap();
        assert_eq!(back, asset);
    }

    #[test]
    fn round_trip_cubemap() {
        let asset = TextureAsset {
            kind: TextureAssetKind::Cubemap,
            width: 8,
            height: 8,
            depth: 1,
            gpu_format: crate::gpu_block_format::GpuBlockFormat::Rgba8,
            memory_budget_mb: 128,
            streaming: false,
            pictures: (0..6).map(|i| format!("face{i}.png")).collect(),
        };
        let text = asset.to_text();
        let back = TextureAsset::parse(&text).unwrap();
        assert_eq!(back.kind, TextureAssetKind::Cubemap);
        assert_eq!(back.pictures.len(), 6);
    }

    #[test]
    fn damaged_header_rejected() {
        assert!(TextureAsset::parse("not a texture file\n").is_err());
    }

    #[test]
    fn cubemap_needs_six_faces() {
        let asset = TextureAsset::parse(&format!(
            "{HEADER}\nkind = cubemap\nwidth = 4\nheight = 4\npictures = [\"a.png\"]"
        ))
        .expect("a cubemap with missing faces still opens for editing");
        let err = asset.validate_pictures().unwrap_err();
        assert!(matches!(err, TextureAssetError::BadPictureCount { .. }));
    }
}
