use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::MsdfOutput;
use crate::Result;
use crate::metadata::Bounds;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonCompression {
    Raw,
    Zstd { level: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonExportOptions {
    pub compression: JsonCompression,
}

impl JsonExportOptions {
    pub fn raw() -> Self {
        Self {
            compression: JsonCompression::Raw,
        }
    }

    pub fn zstd(level: i32) -> Self {
        Self {
            compression: JsonCompression::Zstd { level },
        }
    }
}

impl Default for JsonExportOptions {
    fn default() -> Self {
        Self::zstd(10)
    }
}

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
    pub uncompressed_data_len: usize,
    pub data: String,
}

impl MsdfJsonExport {
    pub fn from_output(output: &MsdfOutput) -> Self {
        Self::from_encoded_payload(output, "base64", output.pixels.clone())
    }

    pub fn from_output_with_options(
        output: &MsdfOutput,
        options: JsonExportOptions,
    ) -> Result<Self> {
        let (encoding, payload) = match options.compression {
            JsonCompression::Raw => ("base64", output.pixels.clone()),
            JsonCompression::Zstd { level } => (
                "base64+zstd",
                zstd::stream::encode_all(output.pixels.as_slice(), level)?,
            ),
        };

        Ok(Self::from_encoded_payload(output, encoding, payload))
    }

    fn from_encoded_payload(output: &MsdfOutput, encoding: &'static str, payload: Vec<u8>) -> Self {
        Self {
            kind: "rs-msdf",
            version: 2,
            format: output.metadata.format,
            encoding,
            channels: output.metadata.channels,
            bytes_per_pixel: output.channels as u32,
            width: output.width,
            height: output.height,
            range_px: output.metadata.range_px,
            svg_bounds: output.metadata.svg_bounds,
            geometry_bounds: output.metadata.geometry_bounds,
            scale: output.metadata.scale,
            translation: output.metadata.translation,
            data_len: payload.len(),
            uncompressed_data_len: output.pixels.len(),
            data: base64::engine::general_purpose::STANDARD.encode(payload),
        }
    }
}
