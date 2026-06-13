use crate::metadata::Bounds;

const EPSILON: f64 = 1.0e-9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub(crate) fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn lerp(self, other: Self, t: f64) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum EdgeColor {
    Red,
    Green,
    Blue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Segment {
    Line {
        p0: Point,
        p1: Point,
        color: EdgeColor,
    },
    Quad {
        p0: Point,
        p1: Point,
        p2: Point,
        color: EdgeColor,
    },
    Cubic {
        p0: Point,
        p1: Point,
        p2: Point,
        p3: Point,
        color: EdgeColor,
    },
}

impl Segment {
    pub(crate) fn start(&self) -> Point {
        match self {
            Segment::Line { p0, .. } | Segment::Quad { p0, .. } | Segment::Cubic { p0, .. } => *p0,
        }
    }

    pub(crate) fn color(&self) -> EdgeColor {
        match self {
            Segment::Line { color, .. }
            | Segment::Quad { color, .. }
            | Segment::Cubic { color, .. } => *color,
        }
    }

    pub(crate) fn set_color(&mut self, color: EdgeColor) {
        match self {
            Segment::Line {
                color: edge_color, ..
            }
            | Segment::Quad {
                color: edge_color, ..
            }
            | Segment::Cubic {
                color: edge_color, ..
            } => *edge_color = color,
        }
    }

    fn point_at(&self, t: f64) -> Point {
        match *self {
            Segment::Line { p0, p1, .. } => p0.lerp(p1, t),
            Segment::Quad { p0, p1, p2, .. } => {
                let a = p0.lerp(p1, t);
                let b = p1.lerp(p2, t);
                a.lerp(b, t)
            }
            Segment::Cubic { p0, p1, p2, p3, .. } => {
                let a = p0.lerp(p1, t);
                let b = p1.lerp(p2, t);
                let c = p2.lerp(p3, t);
                let d = a.lerp(b, t);
                let e = b.lerp(c, t);
                d.lerp(e, t)
            }
        }
    }

    pub(crate) fn distance_to(&self, p: Point) -> f64 {
        match *self {
            Segment::Line { p0, p1, .. } => distance_to_line(p, p0, p1),
            Segment::Quad { .. } | Segment::Cubic { .. } => distance_to_curve(self, p),
        }
    }

    pub(crate) fn add_bounds(&self, bounds: &mut BoundsBuilder) {
        match *self {
            Segment::Line { p0, p1, .. } => {
                bounds.add(p0);
                bounds.add(p1);
            }
            Segment::Quad { p0, p1, p2, .. } => {
                bounds.add(p0);
                bounds.add(p1);
                bounds.add(p2);
            }
            Segment::Cubic { p0, p1, p2, p3, .. } => {
                bounds.add(p0);
                bounds.add(p1);
                bounds.add(p2);
                bounds.add(p3);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Contour {
    pub segments: Vec<Segment>,
    pub fill_rule: FillRule,
}

impl Contour {
    pub(crate) fn signed_area(&self) -> f64 {
        let points = self.sampled_points();
        if points.len() < 3 {
            return 0.0;
        }

        points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
            .map(|(a, b)| a.x * b.y - b.x * a.y)
            .sum::<f64>()
            * 0.5
    }

    fn sampled_points(&self) -> Vec<Point> {
        let mut points = Vec::new();
        for segment in &self.segments {
            let steps = match segment {
                Segment::Line { .. } => 1,
                Segment::Quad { .. } => 12,
                Segment::Cubic { .. } => 20,
            };

            if points.is_empty() {
                points.push(segment.start());
            }

            for i in 1..=steps {
                points.push(segment.point_at(i as f64 / steps as f64));
            }
        }
        points
    }

    pub(crate) fn contains(&self, p: Point) -> bool {
        let points = self.sampled_points();
        if points.len() < 3 {
            return false;
        }

        let mut inside = false;
        let mut previous = *points.last().unwrap();
        for current in points {
            let crosses = (current.y > p.y) != (previous.y > p.y);
            if crosses {
                let x = (previous.x - current.x) * (p.y - current.y)
                    / (previous.y - current.y + EPSILON)
                    + current.x;
                if p.x < x {
                    inside = !inside;
                }
            }
            previous = current;
        }
        inside
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shape {
    pub contours: Vec<Contour>,
}

impl Shape {
    pub(crate) fn bounds(&self) -> Option<Bounds> {
        let mut builder = BoundsBuilder::default();
        for contour in &self.contours {
            for segment in &contour.segments {
                segment.add_bounds(&mut builder);
            }
        }
        builder.finish()
    }

    pub(crate) fn contains(&self, p: Point) -> bool {
        let mut even_odd_inside = false;
        let mut winding = 0_i32;

        for contour in &self.contours {
            if !contour.contains(p) {
                continue;
            }

            match contour.fill_rule {
                FillRule::EvenOdd => even_odd_inside = !even_odd_inside,
                FillRule::NonZero => {
                    winding += if contour.signed_area() >= 0.0 { 1 } else { -1 };
                }
            }
        }

        even_odd_inside || winding != 0
    }

    pub(crate) fn color_edges(&mut self) {
        let palette = [EdgeColor::Red, EdgeColor::Green, EdgeColor::Blue];
        let mut index = 0;
        for contour in &mut self.contours {
            for segment in &mut contour.segments {
                segment.set_color(palette[index % palette.len()]);
                index += 1;
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct BoundsBuilder {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    has_point: bool,
}

impl BoundsBuilder {
    pub(crate) fn add(&mut self, p: Point) {
        if self.has_point {
            self.min_x = self.min_x.min(p.x);
            self.min_y = self.min_y.min(p.y);
            self.max_x = self.max_x.max(p.x);
            self.max_y = self.max_y.max(p.y);
        } else {
            self.min_x = p.x;
            self.min_y = p.y;
            self.max_x = p.x;
            self.max_y = p.y;
            self.has_point = true;
        }
    }

    pub(crate) fn finish(self) -> Option<Bounds> {
        self.has_point.then_some(Bounds {
            min_x: self.min_x,
            min_y: self.min_y,
            max_x: self.max_x,
            max_y: self.max_y,
        })
    }
}

fn distance_to_line(p: Point, a: Point, b: Point) -> f64 {
    let ab = Point::new(b.x - a.x, b.y - a.y);
    let ap = Point::new(p.x - a.x, p.y - a.y);
    let len_sq = ab.x * ab.x + ab.y * ab.y;
    if len_sq <= EPSILON {
        return distance(p, a);
    }

    let t = ((ap.x * ab.x + ap.y * ab.y) / len_sq).clamp(0.0, 1.0);
    distance(p, Point::new(a.x + ab.x * t, a.y + ab.y * t))
}

fn distance_to_curve(segment: &Segment, p: Point) -> f64 {
    let mut best_t = 0.0;
    let mut best = f64::INFINITY;

    for i in 0..=32 {
        let t = i as f64 / 32.0;
        let candidate = distance(segment.point_at(t), p);
        if candidate < best {
            best = candidate;
            best_t = t;
        }
    }

    let mut low = (best_t - 1.0 / 32.0).max(0.0);
    let mut high = (best_t + 1.0 / 32.0).min(1.0);
    for _ in 0..24 {
        let left = low + (high - low) / 3.0;
        let right = high - (high - low) / 3.0;
        if distance(segment.point_at(left), p) < distance(segment.point_at(right), p) {
            high = right;
        } else {
            low = left;
        }
    }

    distance(segment.point_at((low + high) * 0.5), p)
}

fn distance(a: Point, b: Point) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_distance_projects_to_segment() {
        let line = Segment::Line {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(10.0, 0.0),
            color: EdgeColor::Red,
        };

        assert!((line.distance_to(Point::new(5.0, 3.0)) - 3.0).abs() < 1.0e-6);
        assert!((line.distance_to(Point::new(13.0, 4.0)) - 5.0).abs() < 1.0e-6);
    }

    #[test]
    fn contour_contains_points() {
        let contour = Contour {
            fill_rule: FillRule::NonZero,
            segments: vec![
                Segment::Line {
                    p0: Point::new(0.0, 0.0),
                    p1: Point::new(10.0, 0.0),
                    color: EdgeColor::Red,
                },
                Segment::Line {
                    p0: Point::new(10.0, 0.0),
                    p1: Point::new(10.0, 10.0),
                    color: EdgeColor::Green,
                },
                Segment::Line {
                    p0: Point::new(10.0, 10.0),
                    p1: Point::new(0.0, 10.0),
                    color: EdgeColor::Blue,
                },
                Segment::Line {
                    p0: Point::new(0.0, 10.0),
                    p1: Point::new(0.0, 0.0),
                    color: EdgeColor::Red,
                },
            ],
        };

        assert!(contour.contains(Point::new(5.0, 5.0)));
        assert!(!contour.contains(Point::new(15.0, 5.0)));
    }
}
