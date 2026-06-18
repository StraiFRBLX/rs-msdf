//! Library facade for SVG distance field generation.

mod error;
mod export;
mod geometry;
mod metadata;
mod parser;
mod raster;

pub use error::{Error, Result};
pub use export::{
    MsdfJsonExport, encode_png, write_json_export_file, write_metadata_json_file, write_png_file,
};
pub use metadata::{Bounds, MsdfMetadata};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceFieldMode {
    Sdf,
    Psdf,
    Msdf,
    Mtsdf,
}

impl DistanceFieldMode {
    pub fn channels(self) -> usize {
        match self {
            Self::Sdf | Self::Psdf => 1,
            Self::Msdf => 3,
            Self::Mtsdf => 4,
        }
    }

    pub fn format(self) -> &'static str {
        match self {
            Self::Sdf => "sdf-r8",
            Self::Psdf => "psdf-r8",
            Self::Msdf => "msdf-rgb8",
            Self::Mtsdf => "mtsdf-rgba8",
        }
    }

    pub fn channel_name(self) -> &'static str {
        match self {
            Self::Sdf | Self::Psdf => "r",
            Self::Msdf => "rgb",
            Self::Mtsdf => "rgba",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCorrectionMode {
    Disabled,
    EdgePriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorCorrectionConfig {
    pub mode: ErrorCorrectionMode,
}

impl ErrorCorrectionConfig {
    pub fn disabled() -> Self {
        Self {
            mode: ErrorCorrectionMode::Disabled,
        }
    }

    pub fn edge_priority() -> Self {
        Self {
            mode: ErrorCorrectionMode::EdgePriority,
        }
    }

    pub(crate) fn enabled(self) -> bool {
        self.mode != ErrorCorrectionMode::Disabled
    }
}

impl Default for ErrorCorrectionConfig {
    fn default() -> Self {
        Self::edge_priority()
    }
}

/// Options controlling MSDF texture generation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MsdfOptions {
    pub width: u32,
    pub height: u32,
    pub range_px: f64,
    pub mode: DistanceFieldMode,
    pub error_correction: ErrorCorrectionConfig,
    pub overlap_support: bool,
    pub scanline_sign_correction: bool,
}

impl MsdfOptions {
    /// Creates a new option set for RGB MSDF generation and validates it.
    pub fn new(width: u32, height: u32, range_px: f64) -> Result<Self> {
        let options = Self {
            width,
            height,
            range_px,
            mode: DistanceFieldMode::Msdf,
            error_correction: ErrorCorrectionConfig::default(),
            overlap_support: true,
            scanline_sign_correction: true,
        };
        options.validate()?;
        Ok(options)
    }

    pub fn with_mode(mut self, mode: DistanceFieldMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_error_correction(mut self, error_correction: ErrorCorrectionConfig) -> Self {
        self.error_correction = error_correction;
        self
    }

    pub fn with_overlap_support(mut self, overlap_support: bool) -> Self {
        self.overlap_support = overlap_support;
        self
    }

    pub fn with_scanline_sign_correction(mut self, scanline_sign_correction: bool) -> Self {
        self.scanline_sign_correction = scanline_sign_correction;
        self
    }

    pub fn validate(self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::InvalidOptions(
                "texture dimensions must be greater than zero".to_string(),
            ));
        }

        if !self.range_px.is_finite() || self.range_px <= 0.0 {
            return Err(Error::InvalidOptions(
                "distance range must be a finite positive number".to_string(),
            ));
        }

        Ok(())
    }
}

/// Generated distance field pixels and placement metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct MsdfOutput {
    pub width: u32,
    pub height: u32,
    pub channels: usize,
    /// Interleaved 8-bit pixel data.
    pub pixels: Vec<u8>,
    pub metadata: MsdfMetadata,
}

/// Converts SVG bytes into an 8-bit distance field image.
pub fn generate_from_svg(svg: &[u8], options: MsdfOptions) -> Result<MsdfOutput> {
    options.validate()?;

    let parsed = parser::parse_svg(svg, options)?;
    let rasterized = raster::render_msdf(&parsed.shape, options)?;
    let metadata = MsdfMetadata::new(
        options,
        parsed.svg_bounds,
        rasterized.geometry_bounds,
        rasterized.scale,
        rasterized.translation,
    );

    Ok(MsdfOutput {
        width: options.width,
        height: options.height,
        channels: rasterized.channels,
        pixels: rasterized.pixels,
        metadata,
    })
}

/// Reads an SVG file and converts it into an 8-bit distance field image.
pub fn generate_from_svg_file(path: impl AsRef<Path>, options: MsdfOptions) -> Result<MsdfOutput> {
    let svg = std::fs::read(path)?;
    generate_from_svg(&svg, options)
}

/// Expands either a single SVG path or a glob pattern into sorted SVG paths.
pub fn expand_svg_inputs(input: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let input = input.as_ref();
    let input_string = input.to_string_lossy();
    if !has_glob_metacharacters(&input_string) {
        return Ok(vec![input.to_path_buf()]);
    }

    let mut inputs = Vec::new();
    for entry in glob::glob(&input_string)? {
        let path = entry?;
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        {
            inputs.push(path);
        }
    }
    inputs.sort();

    if inputs.is_empty() {
        return Err(Error::InvalidOptions(format!(
            "input glob `{input_string}` did not match any SVG files"
        )));
    }

    Ok(inputs)
}

/// Generates distance fields for multiple SVG files in parallel.
pub fn generate_from_svg_files(
    paths: &[PathBuf],
    options: MsdfOptions,
) -> Vec<(PathBuf, Result<MsdfOutput>)> {
    paths
        .par_iter()
        .map(|path| (path.clone(), generate_from_svg_file(path, options)))
        .collect()
}

fn has_glob_metacharacters(value: &str) -> bool {
    value.contains(['*', '?', '['])
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use tempfile::tempdir;

    const SIMPLE_SVG: &[u8] = br#"
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
          <path d="M1 1 H9 V9 H1 Z" fill="black"/>
        </svg>
    "#;

    #[test]
    fn validates_options() {
        assert!(MsdfOptions::new(16, 16, 4.0).is_ok());
        assert!(MsdfOptions::new(0, 16, 4.0).is_err());
        assert!(MsdfOptions::new(16, 16, 0.0).is_err());
    }

    #[test]
    fn generates_expected_pixel_count_and_metadata() {
        let output = generate_from_svg(SIMPLE_SVG, MsdfOptions::new(8, 8, 2.0).unwrap()).unwrap();

        assert_eq!(output.pixels.len(), 8 * 8 * 3);
        assert_eq!(output.channels, 3);
        assert_eq!(output.metadata.format, "msdf-rgb8");
        assert_eq!(output.metadata.channels, "rgb");
        assert_eq!(output.metadata.width, 8);
        assert_eq!(output.metadata.height, 8);
        assert_eq!(output.metadata.range_px, 2.0);
        assert!(output.metadata.scale > 0.0);
    }

    #[test]
    fn supports_all_distance_field_modes() {
        for (mode, channels, format) in [
            (DistanceFieldMode::Sdf, 1, "sdf-r8"),
            (DistanceFieldMode::Psdf, 1, "psdf-r8"),
            (DistanceFieldMode::Msdf, 3, "msdf-rgb8"),
            (DistanceFieldMode::Mtsdf, 4, "mtsdf-rgba8"),
        ] {
            let options = MsdfOptions::new(8, 8, 2.0).unwrap().with_mode(mode);
            let output = generate_from_svg(SIMPLE_SVG, options).unwrap();

            assert_eq!(output.channels, channels);
            assert_eq!(output.pixels.len(), 8 * 8 * channels);
            assert_eq!(output.metadata.format, format);
        }
    }

    #[test]
    fn json_export_contains_base64_png_and_metadata() {
        let output = generate_from_svg(SIMPLE_SVG, MsdfOptions::new(8, 8, 2.0).unwrap()).unwrap();
        let export = MsdfJsonExport::from_output(&output).unwrap();

        assert_eq!(export.kind, "rs-msdf");
        assert_eq!(export.version, 3);
        assert_eq!(export.format, "msdf-rgb8");
        assert_eq!(export.encoding, "base64-png");
        assert_eq!(export.channels, "rgb");
        assert_eq!(export.bytes_per_pixel, 3);
        assert_eq!(export.width, output.width);
        assert_eq!(export.height, output.height);
        assert_eq!(export.range_px, output.metadata.range_px);
        assert_eq!(export.svg_bounds, output.metadata.svg_bounds);
        assert_eq!(export.geometry_bounds, output.metadata.geometry_bounds);
        assert_eq!(export.scale, output.metadata.scale);
        assert_eq!(export.translation, output.metadata.translation);

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(export.png_base64.as_bytes())
            .unwrap();
        assert!(decoded.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn public_export_helpers_match_cli_capabilities() {
        let temp = tempdir().unwrap();
        let svg_path = temp.path().join("icon.svg");
        let png_path = temp.path().join("icon.png");
        let metadata_path = temp.path().join("icon.json");
        let export_path = temp.path().join("icon.export.json");
        std::fs::write(&svg_path, SIMPLE_SVG).unwrap();

        let output =
            generate_from_svg_file(&svg_path, MsdfOptions::new(16, 16, 4.0).unwrap()).unwrap();
        let png = encode_png(&output).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

        write_png_file(&png_path, &output).unwrap();
        write_metadata_json_file(&metadata_path, &output.metadata, true).unwrap();
        let export = MsdfJsonExport::from_output(&output).unwrap();
        write_json_export_file(&export_path, &export).unwrap();

        assert!(png_path.exists());
        assert!(metadata_path.exists());
        assert!(export_path.exists());
    }

    #[test]
    fn public_glob_and_batch_helpers_match_cli_bulk_input() {
        let temp = tempdir().unwrap();
        let input_dir = temp.path().join("icons");
        std::fs::create_dir(&input_dir).unwrap();
        std::fs::write(input_dir.join("a.svg"), SIMPLE_SVG).unwrap();
        std::fs::write(input_dir.join("b.svg"), SIMPLE_SVG).unwrap();

        let pattern = format!(
            "{}/{}",
            input_dir.display().to_string().replace('\\', "/"),
            "*.svg"
        );
        let inputs = expand_svg_inputs(pattern).unwrap();
        assert_eq!(inputs.len(), 2);

        let outputs = generate_from_svg_files(&inputs, MsdfOptions::new(8, 8, 2.0).unwrap());
        assert_eq!(outputs.len(), 2);
        assert!(outputs.into_iter().all(|(_, output)| output.is_ok()));
    }

    #[test]
    fn mtsdf_overlap_fixture_keeps_filled_bar_opaque() {
        let output = generate_from_svg(
            include_bytes!("../tests/fixtures/monospace-overlap.svg"),
            MsdfOptions::new(512, 128, 4.0)
                .unwrap()
                .with_mode(DistanceFieldMode::Mtsdf),
        )
        .unwrap();

        let alpha = sample_svg_point(&output, 275.0, 30.5, 3);
        assert!(
            alpha > 128,
            "expected overlap bar alpha to be inside/opaque, got {alpha}"
        );
    }

    #[test]
    fn same_paint_home_icon_has_no_fill_stroke_seam() {
        let output = generate_from_svg(
            include_bytes!("../tests/fixtures/home-same-fill-stroke.svg"),
            MsdfOptions::new(96, 96, 4.0).unwrap(),
        )
        .unwrap();

        let alpha = sample_texture_median_alpha(&output, 80, 83);
        assert!(
            alpha > 240,
            "expected lower-right fill/stroke seam to decode as solid body, got {alpha}"
        );
    }

    fn sample_svg_point(output: &MsdfOutput, x: f64, y: f64, channel: usize) -> u8 {
        let tx = (x * output.metadata.scale + output.metadata.translation[0]).round();
        let ty = (y * output.metadata.scale + output.metadata.translation[1]).round();
        let tx = tx.clamp(0.0, f64::from(output.width - 1)) as usize;
        let ty = ty.clamp(0.0, f64::from(output.height - 1)) as usize;
        let offset = (ty * output.width as usize + tx) * output.channels + channel;
        output.pixels[offset]
    }

    fn sample_texture_median_alpha(output: &MsdfOutput, x: usize, y: usize) -> u8 {
        let offset = (y * output.width as usize + x) * output.channels;
        let mut channels = [
            output.pixels[offset],
            output.pixels[offset + 1],
            output.pixels[offset + 2],
        ];
        channels.sort_unstable();
        channels[1]
    }

}
