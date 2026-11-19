//! Rendering core: a render graph that orders, culls and aliases passes, and
//! the device interface every backend implements.
//!
//! The software device (`soft`) draws on the CPU with exactly specified
//! arithmetic, so a frame gives the same pixels on every machine. Tests and
//! headless tools use it; GPU backends implement the same [`Device`].
#![forbid(unsafe_code)]

pub mod clip2d;
pub mod cull;
pub mod device;
pub mod gi;
pub mod gpu_block_format;
pub mod ktx2_io;
pub mod texture_encode;
#[cfg(not(all(windows, target_env = "gnu")))]
mod texture_encode_intel;
pub mod graph;
pub mod hdr_output;
pub mod picture;
mod picture_blocks;
mod picture_mesh;
mod picture_pack;
mod picture_svg;
pub mod probes;
pub mod shadow;
pub mod soft;
pub mod texture_asset;
pub mod texture_load;
pub mod texture_stream;

pub use device::{
    BlendMode, ColourSpace, Device, Instance, QuadBatch, TextureDesc, TextureFilter, TextureFormat,
    TextureId, TextureShape, TextureWrap,
};
pub use gpu_block_format::GpuBlockFormat;
pub use ktx2_io::{read_ktx2, write_ktx2, write_ktx2_from_rgba, Ktx2Image};
pub use texture_encode::{export_block_format, psnr_rgba, EncodeQuality};
pub use texture_asset::{TextureAsset, TextureAssetKind};
pub use graph::{CompiledGraph, GraphError, PassBuilder, PassContext, RenderGraph, ResourceId};
pub use soft::SoftDevice;
