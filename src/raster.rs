use rayon::prelude::*;

use crate::error::{Error, Result};
use crate::geometry::{EdgeColor, Point, Shape};
use crate::metadata::Bounds;
use crate::MsdfOptions;

pub(crate) struct RasterizedMsdf {
    pub rgb_pixels: Vec<u8>,
    pub geometry_bounds: Bounds,
    pub scale: f64,
    pub translation: [f64; 2],
}

pub(crate) fn render_msdf(shape: &Shape, options: MsdfOptions) -> Result<RasterizedMsdf> {
    let geometry_bounds = shape.bounds().ok_or(Error::EmptyGeometry)?;
    if geometry_bounds.width() <= 0.0 || geometry_bounds.height() <= 0.0 {
        return Err(Error::EmptyGeometry);
    }

    let fit_width = (f64::from(options.width) - options.range_px * 2.0).max(1.0);
    let fit_height = (f64::from(options.height) - options.range_px * 2.0).max(1.0);
    let scale = (fit_width / geometry_bounds.width()).min(fit_height / geometry_bounds.height());
    let fitted_width = geometry_bounds.width() * scale;
    let fitted_height = geometry_bounds.height() * scale;
    let translation = [
        (f64::from(options.width) - fitted_width) * 0.5 - geometry_bounds.min_x * scale,
        (f64::from(options.height) - fitted_height) * 0.5 - geometry_bounds.min_y * scale,
    ];

    let pixel_count = options.width as usize * options.height as usize;
    let mut rgb_pixels = vec![0_u8; pixel_count * 3];

    rgb_pixels
        .par_chunks_mut(3)
        .enumerate()
        .for_each(|(index, pixel)| {
            let x = index as u32 % options.width;
            let y = index as u32 / options.width;
            let texture_point = Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let shape_point = Point::new(
                (texture_point.x - translation[0]) / scale,
                (texture_point.y - translation[1]) / scale,
            );

            let sign = if shape.contains(shape_point) {
                1.0
            } else {
                -1.0
            };
            let distances = channel_distances(shape, shape_point, sign);

            pixel[0] = encode_distance(distances[0] * scale, options.range_px);
            pixel[1] = encode_distance(distances[1] * scale, options.range_px);
            pixel[2] = encode_distance(distances[2] * scale, options.range_px);
        });

    Ok(RasterizedMsdf {
        rgb_pixels,
        geometry_bounds,
        scale,
        translation,
    })
}

fn channel_distances(shape: &Shape, p: Point, sign: f64) -> [f64; 3] {
    let mut distances = [f64::INFINITY; 3];

    for contour in &shape.contours {
        for segment in &contour.segments {
            let distance = segment.distance_to(p) * sign;
            match segment.color() {
                EdgeColor::Red => distances[0] = choose_closer(distances[0], distance),
                EdgeColor::Green => distances[1] = choose_closer(distances[1], distance),
                EdgeColor::Blue => distances[2] = choose_closer(distances[2], distance),
            }
        }
    }

    let fallback = distances
        .iter()
        .copied()
        .filter(|d| d.is_finite())
        .min_by(|a, b| a.abs().total_cmp(&b.abs()))
        .unwrap_or(0.0);

    for distance in &mut distances {
        if !distance.is_finite() {
            *distance = fallback;
        }
    }

    distances
}

fn choose_closer(current: f64, candidate: f64) -> f64 {
    if candidate.abs() < current.abs() {
        candidate
    } else {
        current
    }
}

fn encode_distance(distance_px: f64, range_px: f64) -> u8 {
    let normalized = (distance_px / range_px * 0.5 + 0.5).clamp(0.0, 1.0);
    (normalized * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_signed_distance_midpoint() {
        assert_eq!(encode_distance(0.0, 4.0), 128);
        assert_eq!(encode_distance(-4.0, 4.0), 0);
        assert_eq!(encode_distance(4.0, 4.0), 255);
    }
}
