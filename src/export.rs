use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::Result;
use crate::metadata::Bounds;
use crate::{Error, MsdfMetadata, MsdfOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonCompression {
    Raw,
    Zstd { level: u32 },
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

    pub fn zstd(level: u32) -> Self {
        Self {
            compression: JsonCompression::Zstd { level },
        }
    }
}

impl Default for JsonExportOptions {
    fn default() -> Self {
        Self::raw()
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
            JsonCompression::Zstd { level } => {
                let filtered = filter_scanlines(
                    &output.pixels,
                    output.width as usize,
                    output.height as usize,
                    output.channels,
                );
                ("base64+zstd+png-filter", zstd_payload(&filtered, level)?)
            }
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

fn zstd_payload(payload: &[u8], level: u32) -> Result<Vec<u8>> {
    oxiarc_zstd::encode_all(payload, level.min(22) as i32)
        .map_err(|error| crate::Error::Compression(error.to_string()))
}

fn filter_scanlines(pixels: &[u8], width: usize, height: usize, bytes_per_pixel: usize) -> Vec<u8> {
    let row_len = width * bytes_per_pixel;
    let mut output = Vec::with_capacity(height * (row_len + 1));
    let zero_row = vec![0; row_len];

    for y in 0..height {
        let row_start = y * row_len;
        let current = &pixels[row_start..row_start + row_len];
        let previous = if y > 0 {
            &pixels[row_start - row_len..row_start]
        } else {
            &zero_row
        };

        let mut best_filter = 0_u8;
        let mut best_row = filter_row(current, previous, bytes_per_pixel, best_filter);
        let mut best_score = filter_score(&best_row);

        for filter in 1..=4 {
            let filtered = filter_row(current, previous, bytes_per_pixel, filter);
            let score = filter_score(&filtered);
            if score < best_score {
                best_filter = filter;
                best_score = score;
                best_row = filtered;
            }
        }

        output.push(best_filter);
        output.extend(best_row);
    }

    output
}

fn filter_row(current: &[u8], previous: &[u8], bytes_per_pixel: usize, filter: u8) -> Vec<u8> {
    current
        .iter()
        .enumerate()
        .map(|(index, &value)| {
            let left = index
                .checked_sub(bytes_per_pixel)
                .map(|left| current[left])
                .unwrap_or(0);
            let up = previous[index];
            let upper_left = index
                .checked_sub(bytes_per_pixel)
                .map(|left| previous[left])
                .unwrap_or(0);
            let predictor = match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth_predictor(left, up, upper_left),
                _ => unreachable!("invalid PNG filter"),
            };
            value.wrapping_sub(predictor)
        })
        .collect()
}

fn filter_score(filtered: &[u8]) -> u64 {
    filtered
        .iter()
        .map(|&value| {
            let signed = i16::from(value as i8);
            signed.unsigned_abs() as u64
        })
        .sum()
}

fn paeth_predictor(left: u8, up: u8, upper_left: u8) -> u8 {
    let left = i16::from(left);
    let up = i16::from(up);
    let upper_left = i16::from(upper_left);
    let estimate = left + up - upper_left;
    let left_distance = (estimate - left).abs();
    let up_distance = (estimate - up).abs();
    let upper_left_distance = (estimate - upper_left).abs();

    if left_distance <= up_distance && left_distance <= upper_left_distance {
        left as u8
    } else if up_distance <= upper_left_distance {
        up as u8
    } else {
        upper_left as u8
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
    std::fs::write(path, serde_json::to_vec(export)?)?;
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
