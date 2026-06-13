use serde::{Deserialize, Serialize};

use crate::MsdfOptions;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    pub(crate) fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    pub(crate) fn height(self) -> f64 {
        self.max_y - self.min_y
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MsdfMetadata {
    pub format: &'static str,
    pub width: u32,
    pub height: u32,
    pub range_px: f64,
    pub svg_bounds: Bounds,
    pub geometry_bounds: Bounds,
    pub scale: f64,
    pub translation: [f64; 2],
}

impl MsdfMetadata {
    pub(crate) fn new(
        options: MsdfOptions,
        svg_bounds: Bounds,
        geometry_bounds: Bounds,
        scale: f64,
        translation: [f64; 2],
    ) -> Self {
        Self {
            format: "msdf-rgb8",
            width: options.width,
            height: options.height,
            range_px: options.range_px,
            svg_bounds,
            geometry_bounds,
            scale,
            translation,
        }
    }
}
