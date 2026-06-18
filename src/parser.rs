use crate::error::{Error, Result};
use crate::geometry::{Contour, EdgeColor, FillRule, Point, Segment, Shape};
use crate::metadata::Bounds;

pub(crate) struct ParsedSvg {
    pub shape: Shape,
    pub svg_bounds: Bounds,
}

pub(crate) fn parse_svg(svg: &[u8]) -> Result<ParsedSvg> {
    preflight_svg(svg)?;

    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(svg, &options)?;

    reject_unsupported(&tree)?;

    let size = tree.size();
    let svg_bounds = Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: f64::from(size.width()),
        max_y: f64::from(size.height()),
    };

    let mut contours = Vec::new();
    collect_group(tree.root(), &mut contours)?;

    let mut shape = Shape { contours };
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
        let tag = node.tag_name().name();
        match tag {
            "clipPath" => {
                return Err(Error::UnsupportedSvg(
                    "clip paths cannot be converted into precise contours in v1".to_string(),
                ));
            }
            "mask" => {
                return Err(Error::UnsupportedSvg(
                    "masks cannot be converted into precise contours in v1".to_string(),
                ));
            }
            "filter" => {
                return Err(Error::UnsupportedSvg(
                    "filters cannot be converted into precise contours in v1".to_string(),
                ));
            }
            "pattern" => {
                return Err(Error::UnsupportedSvg(
                    "patterns cannot be converted into precise contours in v1".to_string(),
                ));
            }
            "image" => {
                return Err(Error::UnsupportedSvg(
                    "embedded raster images cannot be converted into an MSDF contour".to_string(),
                ));
            }
            _ => {}
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

    if !tree.clip_paths().is_empty() {
        return Err(Error::UnsupportedSvg(
            "clip paths cannot be converted into precise contours in v1".to_string(),
        ));
    }

    if !tree.masks().is_empty() {
        return Err(Error::UnsupportedSvg(
            "masks cannot be converted into precise contours in v1".to_string(),
        ));
    }

    if !tree.filters().is_empty() {
        return Err(Error::UnsupportedSvg(
            "filters cannot be converted into precise contours in v1".to_string(),
        ));
    }

    if !tree.patterns().is_empty() {
        return Err(Error::UnsupportedSvg(
            "patterns cannot be converted into precise contours in v1".to_string(),
        ));
    }

    Ok(())
}

fn collect_group(group: &usvg::Group, contours: &mut Vec<Contour>) -> Result<()> {
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => collect_group(group, contours)?,
            usvg::Node::Path(path) => collect_path(path, contours)?,
            usvg::Node::Image(_) => {
                return Err(Error::UnsupportedSvg(
                    "embedded raster images cannot be converted into an MSDF contour".to_string(),
                ));
            }
            usvg::Node::Text(_) => {
                return Err(Error::UnsupportedSvg(
                    "text nodes must be converted to paths before generating an MSDF".to_string(),
                ));
            }
        }
    }

    Ok(())
}

fn collect_path(path: &usvg::Path, contours: &mut Vec<Contour>) -> Result<()> {
    if path.fill().is_some_and(|fill| fill.opacity().get() > 0.0) {
        let fill_rule = path
            .fill()
            .map(|fill| convert_fill_rule(fill.rule()))
            .unwrap();
        collect_path_data(path.data(), path.abs_transform(), fill_rule, contours)?;
    }

    if let Some(stroke) = path
        .stroke()
        .filter(|stroke| stroke.opacity().get() > 0.0 && stroke.width().get() > 0.0)
    {
        let stroke_path = path
            .data()
            .stroke(&stroke.to_tiny_skia(), 1.0)
            .ok_or_else(|| {
                Error::UnsupportedSvg(
                    "this stroke could not be expanded into outline contours".to_string(),
                )
            })?;
        collect_path_data(
            &stroke_path,
            path.abs_transform(),
            FillRule::NonZero,
            contours,
        )?;
    }

    Ok(())
}

fn collect_path_data(
    path: &tiny_skia_path::Path,
    transform: usvg::Transform,
    fill_rule: FillRule,
    contours: &mut Vec<Contour>,
) -> Result<()> {
    let mut current = None;
    let mut start = None;
    let mut segments = Vec::new();

    for segment in path.segments() {
        match segment {
            tiny_skia_path::PathSegment::MoveTo(p) => {
                finish_contour(&mut segments, fill_rule, contours);
                let p = transform_point(transform, p.x, p.y);
                current = Some(p);
                start = Some(p);
            }
            tiny_skia_path::PathSegment::LineTo(p) => {
                let p0 = current.ok_or_else(|| {
                    Error::UnsupportedSvg("path segment appears before MoveTo".to_string())
                })?;
                let p1 = transform_point(transform, p.x, p.y);
                if p0 != p1 {
                    segments.push(Segment::Line {
                        p0,
                        p1,
                        color: EdgeColor::WHITE,
                    });
                }
                current = Some(p1);
            }
            tiny_skia_path::PathSegment::QuadTo(p1, p2) => {
                let p0 = current.ok_or_else(|| {
                    Error::UnsupportedSvg("path segment appears before MoveTo".to_string())
                })?;
                let p1 = transform_point(transform, p1.x, p1.y);
                let p2 = transform_point(transform, p2.x, p2.y);
                segments.push(Segment::Quad {
                    p0,
                    p1,
                    p2,
                    color: EdgeColor::WHITE,
                });
                current = Some(p2);
            }
            tiny_skia_path::PathSegment::CubicTo(p1, p2, p3) => {
                let p0 = current.ok_or_else(|| {
                    Error::UnsupportedSvg("path segment appears before MoveTo".to_string())
                })?;
                let p1 = transform_point(transform, p1.x, p1.y);
                let p2 = transform_point(transform, p2.x, p2.y);
                let p3 = transform_point(transform, p3.x, p3.y);
                segments.push(Segment::Cubic {
                    p0,
                    p1,
                    p2,
                    p3,
                    color: EdgeColor::WHITE,
                });
                current = Some(p3);
            }
            tiny_skia_path::PathSegment::Close => {
                if let (Some(p0), Some(p1)) = (current, start)
                    && p0 != p1
                {
                    segments.push(Segment::Line {
                        p0,
                        p1,
                        color: EdgeColor::WHITE,
                    });
                }
                current = start;
                finish_contour(&mut segments, fill_rule, contours);
            }
        }
    }

    finish_contour(&mut segments, fill_rule, contours);
    Ok(())
}

fn finish_contour(segments: &mut Vec<Segment>, fill_rule: FillRule, contours: &mut Vec<Contour>) {
    if !segments.is_empty() {
        contours.push(Contour {
            segments: std::mem::take(segments),
            fill_rule,
        });
    }
}

fn convert_fill_rule(fill_rule: usvg::FillRule) -> FillRule {
    match fill_rule {
        usvg::FillRule::NonZero => FillRule::NonZero,
        usvg::FillRule::EvenOdd => FillRule::EvenOdd,
    }
}

fn transform_point(transform: usvg::Transform, x: f32, y: f32) -> Point {
    let mut p = tiny_skia_path::Point::from_xy(x, y);
    transform.map_point(&mut p);
    Point::new(f64::from(p.x), f64::from(p.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_shape_as_path() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12">
              <rect x="1" y="1" width="10" height="10" fill="black"/>
            </svg>
        "#;

        let parsed = parse_svg(svg).unwrap();
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

        assert!(matches!(parse_svg(svg), Err(Error::UnsupportedSvg(_))));
    }

    #[test]
    fn rejects_filters() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
              <filter id="blur"><feGaussianBlur stdDeviation="1"/></filter>
              <rect x="1" y="1" width="8" height="8" filter="url(#blur)"/>
            </svg>
        "#;

        assert!(matches!(parse_svg(svg), Err(Error::UnsupportedSvg(_))));
    }

    #[test]
    fn applies_path_transforms() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20">
              <path d="M0 0 H4 V4 H0 Z" fill="black" transform="translate(6 7) scale(2)"/>
            </svg>
        "#;

        let parsed = parse_svg(svg).unwrap();
        let bounds = parsed.shape.bounds().unwrap();

        assert_eq!(bounds.min_x, 6.0);
        assert_eq!(bounds.min_y, 7.0);
        assert_eq!(bounds.max_x, 14.0);
        assert_eq!(bounds.max_y, 15.0);
    }

    #[test]
    fn expands_visible_strokes() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
              <path d="M1 5 H9" fill="none" stroke="black" stroke-width="2"/>
            </svg>
        "#;

        let parsed = parse_svg(svg).unwrap();
        assert!(!parsed.shape.contours.is_empty());
        assert!(parsed.shape.bounds().unwrap().height() > 0.0);
    }

    #[test]
    fn preserves_non_zero_hole_winding() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
              <path d="M1 1 H9 V9 H1 Z M3 3 V7 H7 V3 Z" fill="black" fill-rule="nonzero"/>
            </svg>
        "#;

        let parsed = parse_svg(svg).unwrap();

        assert!(parsed.shape.contains(Point::new(2.0, 2.0)));
        assert!(!parsed.shape.contains(Point::new(5.0, 5.0)));
    }

    #[test]
    fn preserves_even_odd_holes() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
              <path d="M1 1 H9 V9 H1 Z M3 3 H7 V7 H3 Z" fill="black" fill-rule="evenodd"/>
            </svg>
        "#;

        let parsed = parse_svg(svg).unwrap();

        assert!(parsed.shape.contains(Point::new(2.0, 2.0)));
        assert!(!parsed.shape.contains(Point::new(5.0, 5.0)));
    }

    #[test]
    fn supports_text_when_usvg_resolves_it_to_outlines() {
        let svg = br#"
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">
              <text x="2" y="14" font-family="Arial, sans-serif" font-size="14">A</text>
            </svg>
        "#;

        match parse_svg(svg) {
            Ok(parsed) => assert!(!parsed.shape.contours.is_empty()),
            Err(Error::UnsupportedSvg(message)) => {
                assert!(message.contains("text nodes"));
            }
            Err(error) => panic!("unexpected text parsing error: {error}"),
        }
    }
}
