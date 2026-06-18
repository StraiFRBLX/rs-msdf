use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::Result;
use crate::metadata::Bounds;
use crate::{Error, MsdfMetadata, MsdfOutput};

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
    pub png_base64: String,
}

impl MsdfJsonExport {
    pub fn from_output(output: &MsdfOutput) -> Result<Self> {
        let png = encode_png(output)?;
        let png_base64_capacity = base64::encoded_len(png.len(), true).ok_or_else(|| {
            Error::InvalidOptions("encoded PNG is too large for JSON export".to_string())
        })?;
        let mut png_base64 = String::with_capacity(png_base64_capacity);
        base64::engine::general_purpose::STANDARD.encode_string(png, &mut png_base64);

        Ok(Self {
            kind: "rs-msdf",
            version: 3,
            format: output.metadata.format,
            encoding: "base64-png",
            channels: output.metadata.channels,
            bytes_per_pixel: output.channels as u32,
            width: output.width,
            height: output.height,
            range_px: output.metadata.range_px,
            svg_bounds: output.metadata.svg_bounds,
            geometry_bounds: output.metadata.geometry_bounds,
            scale: output.metadata.scale,
            translation: output.metadata.translation,
            png_base64,
        })
    }
}

pub fn encode_png(output: &MsdfOutput) -> Result<Vec<u8>> {
    let mut png_data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_data, output.width, output.height);
        encoder.set_color(color_type(output.channels)?);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&output.pixels)?;
    }
    Ok(png_data)
}

pub fn write_png_file(path: impl AsRef<Path>, output: &MsdfOutput) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, output.width, output.height);
    encoder.set_color(color_type(output.channels)?);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&output.pixels)?;
    Ok(())
}

pub fn write_metadata_json_file(
    path: impl AsRef<Path>,
    metadata: &MsdfMetadata,
    pretty: bool,
) -> Result<()> {
    let json = if pretty {
        serde_json::to_vec_pretty(metadata)?
    } else {
        serde_json::to_vec(metadata)?
    };
    std::fs::write(path, json)?;
    Ok(())
}

pub fn write_json_export_file(path: impl AsRef<Path>, export: &MsdfJsonExport) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer(writer, export)?;
    Ok(())
}

fn color_type(channels: usize) -> Result<png::ColorType> {
    match channels {
        1 => Ok(png::ColorType::Grayscale),
        3 => Ok(png::ColorType::Rgb),
        4 => Ok(png::ColorType::Rgba),
        _ => Err(Error::InvalidOptions(format!(
            "unsupported channel count `{channels}`"
        ))),
    }
}
