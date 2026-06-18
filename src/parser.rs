use crate::error::{Error, Result};
use crate::geometry::{Contour, EdgeColor, FillRule, Point, Segment, Shape};
use crate::metadata::Bounds;
use crate::MsdfOptions;
use std::collections::{HashMap, HashSet};

const TRACE_SCALE: u32 = 8;
const MIN_TRACE_RESOLUTION: u32 = 512;
const MAX_TRACE_RESOLUTION: u32 = 2048;
const ALPHA_THRESHOLD: u8 = 128;

pub(crate) struct ParsedSvg {
    pub shape: Shape,
    pub svg_bounds: Bounds,
}

pub(crate) fn parse_svg(svg: &[u8], options: MsdfOptions) -> Result<ParsedSvg> {
    preflight_svg(svg)?;

    let mut usvg_options = usvg::Options::default();
    usvg_options.fontdb_mut().load_system_fonts();
    usvg_options.style_sheet = Some("* { color: #000000; }".to_string());

    let tree = usvg::Tree::from_data(svg, &usvg_options)?;
    reject_unsupported(&tree)?;

    let size = tree.size();
    let svg_width = f64::from(size.width());
    let svg_height = f64::from(size.height());
    let svg_bounds = Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: svg_width,
        max_y: svg_height,
    };

    let trace_resolution = trace_resolution(options);
    let scale = (f64::from(trace_resolution) / svg_width.max(svg_height)).max(1.0);
    let trace_width = (svg_width * scale).ceil().max(1.0) as u32;
    let trace_height = (svg_height * scale).ceil().max(1.0) as u32;

    let mut pixmap = tiny_skia::Pixmap::new(trace_width, trace_height).ok_or_else(|| {
        Error::InvalidOptions("trace resolution is too large for the rasterizer".to_string())
    })?;
    let transform = tiny_skia::Transform::from_scale(scale as f32, scale as f32);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mask = AlphaMask::from_pixmap(&pixmap);
    let tolerance = simplify_tolerance(svg_width, svg_height, options);
    let mut shape = trace_mask(&mask, scale, tolerance)?;
    if shape.contours.is_empty() {
        return Err(Error::EmptyGeometry);
    }

    shape.normalize();
    shape.color_edges();
    Ok(ParsedSvg { shape, svg_bounds })
}

fn preflight_svg(svg: &[u8]) -> Result<()> {
    let xml = std::str::from_utf8(svg)
        .map_err(|_| Error::UnsupportedSvg("SVG input must be valid UTF-8 XML data".to_string()))?;
    let document = roxmltree::Document::parse(xml)?;

    for node in document.descendants().filter(|node| node.is_element()) {
        if node.tag_name().name() == "image" {
            return Err(Error::UnsupportedSvg(
                "embedded raster images are outside alpha-mask icon scope".to_string(),
            ));
        }
    }

    Ok(())
}

fn reject_unsupported(tree: &usvg::Tree) -> Result<()> {
    if tree.has_text_nodes() {
        return Err(Error::UnsupportedSvg(
            "text nodes remain after parsing; convert text to paths before generating an MSDF"
                .to_string(),
        ));
    }

    if group_contains_image(tree.root()) {
        return Err(Error::UnsupportedSvg(
            "embedded raster images are outside alpha-mask icon scope".to_string(),
        ));
    }

    Ok(())
}

fn group_contains_image(group: &usvg::Group) -> bool {
    group.children().iter().any(|node| match node {
        usvg::Node::Group(group) => group_contains_image(group),
        usvg::Node::Image(_) => true,
        _ => false,
    })
}

fn trace_resolution(options: MsdfOptions) -> u32 {
    options
        .width
        .max(options.height)
        .saturating_mul(TRACE_SCALE)
        .clamp(MIN_TRACE_RESOLUTION, MAX_TRACE_RESOLUTION)
}

fn simplify_tolerance(svg_width: f64, svg_height: f64, options: MsdfOptions) -> f64 {
    let units_per_output_pixel =
        (svg_width / f64::from(options.width)).max(svg_height / f64::from(options.height));
    0.15 * units_per_output_pixel
}

#[derive(Debug)]
struct AlphaMask {
    width: u32,
    height: u32,
    filled: Vec<bool>,
}

impl AlphaMask {
    fn from_pixmap(pixmap: &tiny_skia::Pixmap) -> Self {
        let filled = pixmap
            .pixels()
            .iter()
            .map(|pixel| pixel.alpha() >= ALPHA_THRESHOLD)
            .collect();
        Self {
            width: pixmap.width(),
            height: pixmap.height(),
            filled,
        }
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return false;
        }
        self.filled[y as usize * self.width as usize + x as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GridPoint {
    x: i32,
    y: i32,
}

impl GridPoint {
    fn to_svg(self, scale: f64) -> Point {
        Point::new(f64::from(self.x) / scale, f64::from(self.y) / scale)
    }
}

type Edge = (GridPoint, GridPoint);

fn trace_mask(mask: &AlphaMask, scale: f64, tolerance: f64) -> Result<Shape> {
    let mut edges = Vec::new();
    for y in 0..mask.height as i32 {
        for x in 0..mask.width as i32 {
            if !mask.contains(x, y) {
                continue;
            }

            if !mask.contains(x, y - 1) {
                edges.push((GridPoint { x, y }, GridPoint { x: x + 1, y }));
            }
            if !mask.contains(x + 1, y) {
                edges.push((GridPoint { x: x + 1, y }, GridPoint { x: x + 1, y: y + 1 }));
            }
            if !mask.contains(x, y + 1) {
                edges.push((GridPoint { x: x + 1, y: y + 1 }, GridPoint { x, y: y + 1 }));
            }
            if !mask.contains(x - 1, y) {
                edges.push((GridPoint { x, y: y + 1 }, GridPoint { x, y }));
            }
        }
    }

    let loops = connect_edges(edges);
    let mut contours = Vec::new();
    for contour in loops {
        let simplified = simplify_closed_polyline(&contour, tolerance * scale);
        if simplified.len() < 3 {
            continue;
        }

        let points: Vec<Point> = simplified
            .into_iter()
            .map(|point| point.to_svg(scale))
            .collect();
        let segments = points
            .iter()
            .copied()
            .zip(points.iter().copied().cycle().skip(1))
            .take(points.len())
            .filter(|(p0, p1)| p0 != p1)
            .map(|(p0, p1)| Segment::Line {
                p0,
                p1,
                color: EdgeColor::WHITE,
                is_boundary: true,
            })
            .collect::<Vec<_>>();

        if !segments.is_empty() {
            contours.push(Contour {
                segments,
                fill_rule: FillRule::EvenOdd,
                is_boundary: true,
            });
        }
    }

    Ok(Shape { contours })
}

fn connect_edges(edges: Vec<Edge>) -> Vec<Vec<GridPoint>> {
    let mut outgoing: HashMap<GridPoint, Vec<GridPoint>> = HashMap::new();
    for (from, to) in edges {
        outgoing.entry(from).or_default().push(to);
    }
    for targets in outgoing.values_mut() {
        targets.sort_by_key(|point| (point.y, point.x));
    }

    let mut used = HashSet::new();
    let mut contours = Vec::new();
    let starts = outgoing.keys().copied().collect::<Vec<_>>();

    for start in starts {
        let Some(&first_next) = outgoing.get(&start).and_then(|targets| targets.first()) else {
            continue;
        };
        if used.contains(&(start, first_next)) {
            continue;
        }

        let mut contour = vec![start];
        let mut current = start;
        while let Some(next) = next_unused_edge(current, &outgoing, &used) {
            used.insert((current, next));
            if next == start {
                break;
            }
            contour.push(next);
            current = next;
        }

        if contour.len() >= 3 {
            contours.push(contour);
        }
    }

    contours
}

fn next_unused_edge(
    current: GridPoint,
    outgoing: &HashMap<GridPoint, Vec<GridPoint>>,
    used: &HashSet<Edge>,
) -> Option<GridPoint> {
    outgoing
        .get(&current)?
        .iter()
        .copied()
        .find(|next| !used.contains(&(current, *next)))
}

fn simplify_closed_polyline(points: &[GridPoint], tolerance: f64) -> Vec<GridPoint> {
    if points.len() <= 3 {
        return points.to_vec();
    }

    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() / 2] = true;
    rdp(points, 0, points.len() / 2, tolerance, &mut keep);
    rdp(points, points.len() / 2, points.len() - 1, tolerance, &mut keep);
    rdp_wrapped(points, points.len() - 1, 0, tolerance, &mut keep);

    points
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, point)| keep[index].then_some(point))
        .collect()
}

fn rdp(points: &[GridPoint], start: usize, end: usize, tolerance: f64, keep: &mut [bool]) {
    if end <= start + 1 {
        return;
    }

    let (index, distance) = max_distance(points, start, end);
    if distance > tolerance {
        keep[index] = true;
        rdp(points, start, index, tolerance, keep);
        rdp(points, index, end, tolerance, keep);
    }
}

fn rdp_wrapped(points: &[GridPoint], start: usize, end: usize, tolerance: f64, keep: &mut [bool]) {
    let mut wrapped = points[start..].to_vec();
    wrapped.extend_from_slice(&points[..=end]);
    let mut wrapped_keep = vec![false; wrapped.len()];
    wrapped_keep[0] = true;
    wrapped_keep[wrapped.len() - 1] = true;
    rdp(&wrapped, 0, wrapped.len() - 1, tolerance, &mut wrapped_keep);

    for (wrapped_index, should_keep) in wrapped_keep.into_iter().enumerate() {
        if !should_keep {
            continue;
        }
        let index = (start + wrapped_index) % points.len();
        keep[index] = true;
    }
}

fn max_distance(points: &[GridPoint], start: usize, end: usize) -> (usize, f64) {
    let a = points[start];
    let b = points[end];
    let mut best_index = start;
    let mut best_distance = 0.0;

    for (index, point) in points.iter().enumerate().take(end).skip(start + 1) {
        let distance = point_line_distance(*point, a, b);
        if distance > best_distance {
            best_distance = distance;
            best_index = index;
        }
    }

    (best_index, best_distance)
}

fn point_line_distance(point: GridPoint, a: GridPoint, b: GridPoint) -> f64 {
    let px = f64::from(point.x);
    let py = f64::from(point.y);
    let ax = f64::from(a.x);
    let ay = f64::from(a.y);
    let bx = f64::from(b.x);
    let by = f64::from(b.y);
    let dx = bx - ax;
    let dy = by - ay;

    if dx == 0.0 && dy == 0.0 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }

    ((dy * px - dx * py + bx * ay - by * ax).abs()) / (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_shape_from_rendered_alpha() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12">
              <rect x="1" y="1" width="10" height="10" fill="black"/>
            </svg>
        "#;

        let parsed = parse_svg(svg, MsdfOptions::new(32, 32, 4.0).unwrap()).unwrap();
        assert!(!parsed.shape.contours.is_empty());
        assert_eq!(parsed.svg_bounds.max_x, 12.0);
    }

    #[test]
    fn rejects_images() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
              <image href="data:image/png;base64,iVBORw0KGgo=" width="10" height="10"/>
            </svg>
        "#;

        assert!(matches!(
            parse_svg(svg, MsdfOptions::new(16, 16, 4.0).unwrap()),
            Err(Error::UnsupportedSvg(_))
        ));
    }

    #[test]
    fn filters_are_render_truth_instead_of_parse_rejections() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
              <filter id="blur"><feGaussianBlur stdDeviation="0.1"/></filter>
              <rect x="1" y="1" width="8" height="8" filter="url(#blur)"/>
            </svg>
        "#;

        let parsed = parse_svg(svg, MsdfOptions::new(32, 32, 4.0).unwrap()).unwrap();
        assert!(!parsed.shape.contours.is_empty());
    }

    #[test]
    fn strokes_are_traced_from_visible_coverage() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
              <path d="M1 5 H9" fill="none" stroke="black" stroke-width="2"/>
            </svg>
        "#;

        let parsed = parse_svg(svg, MsdfOptions::new(64, 64, 4.0).unwrap()).unwrap();
        assert!(!parsed.shape.contours.is_empty());
        assert!(parsed.shape.bounds().unwrap().height() > 0.0);
    }

    #[test]
    fn preserves_holes_from_alpha_silhouette() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
              <path d="M1 1 H9 V9 H1 Z M3 3 H7 V7 H3 Z" fill="black" fill-rule="evenodd"/>
            </svg>
        "#;

        let parsed = parse_svg(svg, MsdfOptions::new(64, 64, 4.0).unwrap()).unwrap();

        assert!(parsed.shape.contains(Point::new(2.0, 2.0)));
        assert!(!parsed.shape.contains(Point::new(5.0, 5.0)));
    }
}
