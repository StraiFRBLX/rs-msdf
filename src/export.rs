use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::MsdfOutput;
use crate::metadata::Bounds;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MsdfJsonExport {
    pub kind: &'static str,
    pub version: u32,
    pub format: &'static str,
    pub encoding: &'static str,
    pub channels: &'static str,
    pub bytes_per_pixel: u32,
    pub width: u32,
    pub height: u32,
    pub range_px: f64,
    pub svg_bounds: Bounds,
    pub geometry_bounds: Bounds,
    pub scale: f64,
    pub translation: [f64; 2],
    pub data_len: usize,
    pub data: String,
}

impl MsdfJsonExport {
    pub fn from_output(output: &MsdfOutput) -> Self {
        Self {
            kind: "rs-msdf",
            version: 1,
            format: output.metadata.format,
            encoding: "base64",
            channels: output.metadata.channels,
            bytes_per_pixel: output.channels as u32,
            width: output.width,
            height: output.height,
            range_px: output.metadata.range_px,
            svg_bounds: output.metadata.svg_bounds,
            geometry_bounds: output.metadata.geometry_bounds,
            scale: output.metadata.scale,
            translation: output.metadata.translation,
            data_len: output.pixels.len(),
            data: base64::engine::general_purpose::STANDARD.encode(&output.pixels),
        }
    }
}
