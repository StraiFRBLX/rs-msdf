//! Library facade for SVG to RGB MSDF generation.

mod error;
mod geometry;
mod metadata;
mod parser;
mod raster;

pub use error::{Error, Result};
pub use metadata::{Bounds, MsdfMetadata};

/// Options controlling MSDF texture generation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MsdfOptions {
    pub width: u32,
    pub height: u32,
    pub range_px: f64,
}

impl MsdfOptions {
    /// Creates a new option set and validates it.
    pub fn new(width: u32, height: u32, range_px: f64) -> Result<Self> {
        let options = Self {
            width,
            height,
            range_px,
        };
        options.validate()?;
        Ok(options)
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

/// Generated MSDF pixels and placement metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct MsdfOutput {
    pub width: u32,
    pub height: u32,
    /// Interleaved 8-bit RGB MSDF pixels.
    pub rgb_pixels: Vec<u8>,
    pub metadata: MsdfMetadata,
}

/// Converts SVG bytes into an 8-bit RGB MSDF image.
pub fn generate_from_svg(svg: &[u8], options: MsdfOptions) -> Result<MsdfOutput> {
    options.validate()?;

    let parsed = parser::parse_svg(svg)?;
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
        rgb_pixels: rasterized.rgb_pixels,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(output.rgb_pixels.len(), 8 * 8 * 3);
        assert_eq!(output.metadata.format, "msdf-rgb8");
        assert_eq!(output.metadata.width, 8);
        assert_eq!(output.metadata.height, 8);
        assert_eq!(output.metadata.range_px, 2.0);
        assert!(output.metadata.scale > 0.0);
    }
}
