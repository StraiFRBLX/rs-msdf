use crate::metadata::Bounds;

const EPSILON: f64 = 1.0e-9;
const CUBIC_SEARCH_STARTS: usize = 4;
const CUBIC_SEARCH_STEPS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub(crate) fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub(crate) fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    pub(crate) fn cross(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }

    pub(crate) fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    fn lerp(self, other: Self, t: f64) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }

    pub(crate) fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }

    pub(crate) fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }

    pub(crate) fn scale(self, scale: f64) -> Self {
        Self::new(self.x * scale, self.y * scale)
    }

    fn orthonormal(self) -> Self {
        let normalized = self.normalize();
        Self::new(normalized.y, -normalized.x)
    }

    pub(crate) fn normalize(self) -> Self {
        let length = self.length();
        if length <= EPSILON {
            Self::new(0.0, 0.0)
        } else {
            Self::new(self.x / length, self.y / length)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SignedDistance {
    pub distance: f64,
    pub dot: f64,
}

impl SignedDistance {
    pub(crate) fn new(distance: f64, dot: f64) -> Self {
        Self { distance, dot }
    }

    pub(crate) fn is_closer_than(self, other: Self) -> bool {
        self.distance.abs() < other.distance.abs()
            || (self.distance.abs() == other.distance.abs() && self.dot < other.dot)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EdgeColor(u8);

impl EdgeColor {
    pub(crate) const RED: Self = Self(0b001);
    pub(crate) const GREEN: Self = Self(0b010);
    pub(crate) const BLUE: Self = Self(0b100);
    pub(crate) const YELLOW: Self = Self(0b011);
    pub(crate) const MAGENTA: Self = Self(0b101);
    pub(crate) const CYAN: Self = Self(0b110);
    pub(crate) const WHITE: Self = Self(0b111);

    pub(crate) fn has_red(self) -> bool {
        self.0 & Self::RED.0 != 0
    }

    pub(crate) fn has_green(self) -> bool {
        self.0 & Self::GREEN.0 != 0
    }

    pub(crate) fn has_blue(self) -> bool {
        self.0 & Self::BLUE.0 != 0
    }

    fn and(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    fn xor(self, other: Self) -> Self {
        Self(self.0 ^ other.0)
    }

    pub(crate) fn bits(self) -> u8 {
        self.0
    }
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

    pub(crate) fn point_at(&self, t: f64) -> Point {
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

    pub(crate) fn tangent_at(&self, t: f64) -> Point {
        match *self {
            Segment::Line { p0, p1, .. } => Point::new(p1.x - p0.x, p1.y - p0.y),
            Segment::Quad { p0, p1, p2, .. } => {
                let a = Point::new(p1.x - p0.x, p1.y - p0.y);
                let b = Point::new(p2.x - p1.x, p2.y - p1.y);
                Point::new(2.0 * (a.x + (b.x - a.x) * t), 2.0 * (a.y + (b.y - a.y) * t))
            }
            Segment::Cubic { p0, p1, p2, p3, .. } => {
                let a = p0.lerp(p1, t);
                let b = p1.lerp(p2, t);
                let c = p2.lerp(p3, t);
                let d = Point::new(b.x - a.x, b.y - a.y);
                let e = Point::new(c.x - b.x, c.y - b.y);
                Point::new(3.0 * (d.x + (e.x - d.x) * t), 3.0 * (d.y + (e.y - d.y) * t))
            }
        }
    }

    pub(crate) fn direction_start(&self) -> Point {
        self.tangent_at(0.0).normalize()
    }

    pub(crate) fn direction_end(&self) -> Point {
        self.tangent_at(1.0).normalize()
    }

    pub(crate) fn signed_distance_to(&self, p: Point, fallback_sign: f64) -> (SignedDistance, f64) {
        match *self {
            Segment::Line { p0, p1, .. } => signed_distance_line(p, p0, p1),
            Segment::Quad { p0, p1, p2, .. } => signed_distance_quad(p, p0, p1, p2),
            Segment::Cubic { p0, p1, p2, p3, .. } => {
                signed_distance_cubic(p, p0, p1, p2, p3, fallback_sign)
            }
        }
    }

    pub(crate) fn pseudo_distance(&self, distance: SignedDistance, p: Point, param: f64) -> f64 {
        let mut result = distance.distance;
        if param < 0.0 {
            let dir = self.direction_start();
            let ep = Point::new(p.x - self.start().x, p.y - self.start().y);
            if let Some(perpendicular) = perpendicular_distance(ep, Point::new(-dir.x, -dir.y)) {
                result = choose_smaller(result, -perpendicular);
            }
        } else if param > 1.0 {
            let end = self.end();
            let dir = self.direction_end();
            let ep = Point::new(p.x - end.x, p.y - end.y);
            if let Some(perpendicular) = perpendicular_distance(ep, dir) {
                result = choose_smaller(result, perpendicular);
            }
        }
        result
    }

    pub(crate) fn end(&self) -> Point {
        match self {
            Segment::Line { p1, .. } => *p1,
            Segment::Quad { p2, .. } => *p2,
            Segment::Cubic { p3, .. } => *p3,
        }
    }

    #[allow(dead_code)]
    fn reversed(&self) -> Self {
        match *self {
            Segment::Line { p0, p1, color } => Segment::Line {
                p0: p1,
                p1: p0,
                color,
            },
            Segment::Quad { p0, p1, p2, color } => Segment::Quad {
                p0: p2,
                p1,
                p2: p0,
                color,
            },
            Segment::Cubic {
                p0,
                p1,
                p2,
                p3,
                color,
            } => Segment::Cubic {
                p0: p3,
                p1: p2,
                p2: p1,
                p3: p0,
                color,
            },
        }
    }

    fn split_in_thirds(&self) -> Vec<Self> {
        [
            self.split_range(0.0, 1.0 / 3.0),
            self.split_range(1.0 / 3.0, 2.0 / 3.0),
            self.split_range(2.0 / 3.0, 1.0),
        ]
        .into()
    }

    fn split_range(&self, start: f64, end: f64) -> Self {
        match *self {
            Segment::Line { color, .. } => Segment::Line {
                p0: self.point_at(start),
                p1: self.point_at(end),
                color,
            },
            Segment::Quad { color, .. } => {
                let p0 = self.point_at(start);
                let p2 = self.point_at(end);
                let midpoint_t = (start + end) * 0.5;
                let midpoint = self.point_at(midpoint_t);
                Segment::Quad {
                    p0,
                    p1: Point::new(
                        2.0 * midpoint.x - 0.5 * (p0.x + p2.x),
                        2.0 * midpoint.y - 0.5 * (p0.y + p2.y),
                    ),
                    p2,
                    color,
                }
            }
            Segment::Cubic { color, .. } => {
                let p0 = self.point_at(start);
                let p3 = self.point_at(end);
                let span = end - start;
                let t0 = self.tangent_at(start);
                let t1 = self.tangent_at(end);
                Segment::Cubic {
                    p0,
                    p1: Point::new(p0.x + t0.x * span / 3.0, p0.y + t0.y * span / 3.0),
                    p2: Point::new(p3.x - t1.x * span / 3.0, p3.y - t1.y * span / 3.0),
                    p3,
                    color,
                }
            }
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
                bounds.add(p2);
                add_quadratic_extrema(bounds, p0, p1, p2);
            }
            Segment::Cubic { p0, p1, p2, p3, .. } => {
                bounds.add(p0);
                bounds.add(p3);
                add_cubic_extrema(bounds, p0, p1, p2, p3);
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

    pub(crate) fn winding(&self) -> i32 {
        if self.signed_area() >= 0.0 { 1 } else { -1 }
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

    #[allow(dead_code)]
    fn reverse(&mut self) {
        self.segments = self.segments.iter().rev().map(Segment::reversed).collect();
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
        let mut seed = 0_u64;
        for contour in &mut self.contours {
            color_contour_edges(contour, &mut seed);
        }
    }

    pub(crate) fn normalize(&mut self) {
        for contour in &mut self.contours {
            if contour.segments.len() == 1 {
                let segment = contour.segments.remove(0);
                contour.segments.extend(segment.split_in_thirds());
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn orient_contours(&mut self) {
        for contour in &mut self.contours {
            if contour.signed_area() < 0.0 {
                contour.reverse();
            }
        }
    }
}

fn add_quadratic_extrema(bounds: &mut BoundsBuilder, p0: Point, p1: Point, p2: Point) {
    for t in [
        quadratic_extremum(p0.x, p1.x, p2.x),
        quadratic_extremum(p0.y, p1.y, p2.y),
    ]
    .into_iter()
    .flatten()
    {
        if t > 0.0 && t < 1.0 {
            bounds.add(quad_point(p0, p1, p2, t));
        }
    }
}

fn quadratic_extremum(a: f64, b: f64, c: f64) -> Option<f64> {
    let denominator = a - 2.0 * b + c;
    (denominator.abs() > EPSILON).then_some((a - b) / denominator)
}

fn add_cubic_extrema(bounds: &mut BoundsBuilder, p0: Point, p1: Point, p2: Point, p3: Point) {
    for (a, b, c, d) in [(p0.x, p1.x, p2.x, p3.x), (p0.y, p1.y, p2.y, p3.y)] {
        let mut solutions = [0.0; 3];
        let count = solve_quadratic(
            &mut solutions,
            -a + 3.0 * b - 3.0 * c + d,
            2.0 * (a - 2.0 * b + c),
            b - a,
        );
        for &t in &solutions[..count.max(0) as usize] {
            if t > 0.0 && t < 1.0 {
                bounds.add(cubic_point(p0, p1, p2, p3, t));
            }
        }
    }
}

fn quad_point(p0: Point, p1: Point, p2: Point, t: f64) -> Point {
    let a = p0.lerp(p1, t);
    let b = p1.lerp(p2, t);
    a.lerp(b, t)
}

fn cubic_point(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let a = p0.lerp(p1, t);
    let b = p1.lerp(p2, t);
    let c = p2.lerp(p3, t);
    let d = a.lerp(b, t);
    let e = b.lerp(c, t);
    d.lerp(e, t)
}

fn color_contour_edges(contour: &mut Contour, seed: &mut u64) {
    let edge_count = contour.segments.len();
    if edge_count == 0 {
        return;
    }

    let corners: Vec<_> = (0..edge_count)
        .filter(|&index| {
            let previous = contour.segments[(index + edge_count - 1) % edge_count].direction_end();
            let next = contour.segments[index].direction_start();
            is_corner(previous, next)
        })
        .collect();

    if corners.is_empty() {
        let color = switch_color(init_color(seed), seed, None);
        contour
            .segments
            .iter_mut()
            .for_each(|segment| segment.set_color(color));
        return;
    }

    if corners.len() == 1 {
        color_teardrop(contour, corners[0], seed);
        return;
    }

    let corner_count = corners.len();
    let start = corners[0];
    let initial = switch_color(init_color(seed), seed, None);
    let mut color = initial;
    let mut spline = 0;
    for i in 0..edge_count {
        let index = (start + i) % edge_count;
        if spline + 1 < corner_count && corners[spline + 1] == index {
            spline += 1;
            let banned = (spline == corner_count - 1).then_some(initial);
            color = switch_color(color, seed, banned);
        }
        contour.segments[index].set_color(color);
    }
}

fn color_teardrop(contour: &mut Contour, corner: usize, seed: &mut u64) {
    let mut color = switch_color(init_color(seed), seed, None);
    let colors = [color, EdgeColor::WHITE, {
        color = switch_color(color, seed, None);
        color
    }];
    let edge_count = contour.segments.len();

    if edge_count >= 3 {
        for i in 0..edge_count {
            contour.segments[(corner + i) % edge_count]
                .set_color(colors[(1 + symmetrical_trichotomy(i, edge_count)) as usize]);
        }
    } else {
        let mut segments = Vec::with_capacity(edge_count * 3);
        for i in 0..edge_count {
            let mut thirds = contour.segments[(corner + i) % edge_count].split_in_thirds();
            for (j, segment) in thirds.iter_mut().enumerate() {
                segment.set_color(colors[(i * 3 + j).min(2)]);
            }
            segments.extend(thirds);
        }
        contour.segments = segments;
    }
}

fn symmetrical_trichotomy(position: usize, count: usize) -> i32 {
    (3.0 + 2.875 * position as f64 / (count - 1) as f64 - 1.4375 + 0.5) as i32 - 3
}

fn init_color(seed: &mut u64) -> EdgeColor {
    [EdgeColor::CYAN, EdgeColor::MAGENTA, EdgeColor::YELLOW][seed_extract3(seed) as usize]
}

fn switch_color(color: EdgeColor, seed: &mut u64, banned: Option<EdgeColor>) -> EdgeColor {
    if let Some(banned) = banned {
        let combined = color.and(banned);
        if combined == EdgeColor::RED || combined == EdgeColor::GREEN || combined == EdgeColor::BLUE
        {
            return combined.xor(EdgeColor::WHITE);
        }
    }

    let shifted = color.0 << (1 + seed_extract2(seed));
    EdgeColor((shifted | (shifted >> 3)) & EdgeColor::WHITE.0)
}

fn seed_extract2(seed: &mut u64) -> u64 {
    let value = *seed & 1;
    *seed >>= 1;
    value
}

fn seed_extract3(seed: &mut u64) -> u64 {
    let value = *seed % 3;
    *seed /= 3;
    value
}

fn is_corner(previous: Point, next: Point) -> bool {
    previous.dot(next) <= 0.0 || previous.cross(next).abs() > 3.0_f64.sin()
}

fn perpendicular_distance(ep: Point, edge_dir: Point) -> Option<f64> {
    (ep.dot(edge_dir) > 0.0).then_some(ep.cross(edge_dir))
}

fn choose_smaller(current: f64, candidate: f64) -> f64 {
    if candidate.abs() <= current.abs() {
        candidate
    } else {
        current
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

fn signed_distance_line(p: Point, a: Point, b: Point) -> (SignedDistance, f64) {
    let aq = p.sub(a);
    let ab = b.sub(a);
    let len_sq = ab.dot(ab);
    if len_sq <= EPSILON {
        return (SignedDistance::new(distance(p, a), 1.0), 0.0);
    }

    let param = aq.dot(ab) / len_sq;
    let endpoint = if param > 0.5 { b } else { a };
    let eq = endpoint.sub(p);
    let endpoint_distance = eq.length();

    if param > 0.0 && param < 1.0 {
        let ortho_distance = ab.orthonormal().dot(aq);
        if ortho_distance.abs() < endpoint_distance {
            return (SignedDistance::new(ortho_distance, 0.0), param);
        }
    }

    let signed = non_zero_sign(aq.cross(ab)) * endpoint_distance;
    let dot = ab.normalize().dot(eq.normalize()).abs();
    (SignedDistance::new(signed, dot), param)
}

fn signed_distance_quad(p: Point, p0: Point, p1: Point, p2: Point) -> (SignedDistance, f64) {
    let qa = p0.sub(p);
    let ab = p1.sub(p0);
    let br = p2.sub(p1).sub(ab);
    let mut solutions = [0.0; 3];
    let solution_count = solve_cubic(
        &mut solutions,
        br.dot(br),
        3.0 * ab.dot(br),
        2.0 * ab.dot(ab) + qa.dot(br),
        qa.dot(ab),
    );

    let mut ep_dir = p1.sub(p0);
    let mut min_distance = non_zero_sign(ep_dir.cross(qa)) * qa.length();
    let mut param = -qa.dot(ep_dir) / ep_dir.dot(ep_dir);

    let b_distance = p2.sub(p).length();
    if b_distance < min_distance.abs() {
        ep_dir = p2.sub(p1);
        min_distance = non_zero_sign(ep_dir.cross(p2.sub(p))) * b_distance;
        param = p.sub(p1).dot(ep_dir) / ep_dir.dot(ep_dir);
    }

    for &t in &solutions[..solution_count.max(0) as usize] {
        if t > 0.0 && t < 1.0 {
            let qe = qa.add(ab.scale(2.0 * t)).add(br.scale(t * t));
            let distance = qe.length();
            if distance <= min_distance.abs() {
                min_distance = non_zero_sign(ab.add(br.scale(t)).cross(qe)) * distance;
                param = t;
            }
        }
    }

    if (0.0..=1.0).contains(&param) {
        return (SignedDistance::new(min_distance, 0.0), param);
    }

    let dot = if param < 0.5 {
        p1.sub(p0).normalize().dot(qa.normalize()).abs()
    } else {
        p2.sub(p1).normalize().dot(p2.sub(p).normalize()).abs()
    };
    (SignedDistance::new(min_distance, dot), param)
}

fn signed_distance_cubic(
    p: Point,
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    fallback_sign: f64,
) -> (SignedDistance, f64) {
    let qa = p0.sub(p);
    let ab = p1.sub(p0);
    let br = p2.sub(p1).sub(ab);
    let as_ = p3.sub(p2).sub(p2.sub(p1)).sub(br);

    let mut ep_dir = p1.sub(p0);
    let mut min_distance = non_zero_sign(ep_dir.cross(qa)) * qa.length();
    let mut param = if ep_dir.dot(ep_dir) <= EPSILON {
        0.0
    } else {
        -qa.dot(ep_dir) / ep_dir.dot(ep_dir)
    };

    let b_distance = p3.sub(p).length();
    if b_distance < min_distance.abs() {
        ep_dir = p3.sub(p2);
        min_distance = non_zero_sign(ep_dir.cross(p3.sub(p))) * b_distance;
        param = if ep_dir.dot(ep_dir) <= EPSILON {
            1.0
        } else {
            ep_dir.sub(p3.sub(p)).dot(ep_dir) / ep_dir.dot(ep_dir)
        };
    }

    for i in 0..=CUBIC_SEARCH_STARTS {
        let mut t = i as f64 / CUBIC_SEARCH_STARTS as f64;
        let qe = cubic_relative(qa, ab, br, as_, t);
        let d1 = cubic_derivative(ab, br, as_, t);
        let d2 = cubic_second_derivative(br, as_, t);
        let denom = d1.dot(d1) + qe.dot(d2);
        if denom.abs() <= EPSILON {
            continue;
        }

        let mut improved_t = t - qe.dot(d1) / denom;
        if improved_t > 0.0 && improved_t < 1.0 {
            let mut remaining = CUBIC_SEARCH_STEPS;
            loop {
                t = improved_t;
                let qe = cubic_relative(qa, ab, br, as_, t);
                let d1 = cubic_derivative(ab, br, as_, t);
                remaining -= 1;
                if remaining == 0 {
                    break;
                }
                let d2 = cubic_second_derivative(br, as_, t);
                let denom = d1.dot(d1) + qe.dot(d2);
                if denom.abs() <= EPSILON {
                    break;
                }
                improved_t = t - qe.dot(d1) / denom;
                if improved_t <= 0.0 || improved_t >= 1.0 {
                    break;
                }
            }

            let qe = cubic_relative(qa, ab, br, as_, t);
            let d1 = cubic_derivative(ab, br, as_, t);
            let distance = qe.length();
            if distance < min_distance.abs() {
                min_distance = non_zero_sign(d1.cross(qe)) * distance;
                param = t;
            }
        }
    }

    if min_distance == 0.0 {
        min_distance = fallback_sign * 0.0;
    }

    if (0.0..=1.0).contains(&param) {
        return (SignedDistance::new(min_distance, 0.0), param);
    }

    let dot = if param < 0.5 {
        p1.sub(p0).normalize().dot(qa.normalize()).abs()
    } else {
        p3.sub(p2).normalize().dot(p3.sub(p).normalize()).abs()
    };
    (SignedDistance::new(min_distance, dot), param)
}

fn cubic_relative(qa: Point, ab: Point, br: Point, as_: Point, t: f64) -> Point {
    qa.add(ab.scale(3.0 * t))
        .add(br.scale(3.0 * t * t))
        .add(as_.scale(t * t * t))
}

fn cubic_derivative(ab: Point, br: Point, as_: Point, t: f64) -> Point {
    ab.scale(3.0)
        .add(br.scale(6.0 * t))
        .add(as_.scale(3.0 * t * t))
}

fn cubic_second_derivative(br: Point, as_: Point, t: f64) -> Point {
    br.scale(6.0).add(as_.scale(6.0 * t))
}

fn solve_quadratic(out: &mut [f64; 3], a: f64, b: f64, c: f64) -> i32 {
    if a == 0.0 || b.abs() > 1.0e12 * a.abs() {
        if b == 0.0 {
            return if c == 0.0 { -1 } else { 0 };
        }
        out[0] = -c / b;
        return 1;
    }

    let discriminant = b * b - 4.0 * a * c;
    if discriminant > 0.0 {
        let root = discriminant.sqrt();
        out[0] = (-b + root) / (2.0 * a);
        out[1] = (-b - root) / (2.0 * a);
        2
    } else if discriminant == 0.0 {
        out[0] = -b / (2.0 * a);
        1
    } else {
        0
    }
}

fn solve_cubic(out: &mut [f64; 3], a: f64, b: f64, c: f64, d: f64) -> i32 {
    if a != 0.0 {
        let bn = b / a;
        if bn.abs() < 1.0e6 {
            return solve_cubic_normed(out, bn, c / a, d / a);
        }
    }
    solve_quadratic(out, b, c, d)
}

fn solve_cubic_normed(out: &mut [f64; 3], a: f64, b: f64, c: f64) -> i32 {
    let a2 = a * a;
    let mut q = (a2 - 3.0 * b) / 9.0;
    let r = (a * (2.0 * a2 - 9.0 * b) + 27.0 * c) / 54.0;
    let r2 = r * r;
    let q3 = q * q * q;
    let a = a / 3.0;

    if r2 < q3 {
        let mut t = r / q3.sqrt();
        t = t.clamp(-1.0, 1.0).acos();
        q = -2.0 * q.sqrt();
        out[0] = q * (t / 3.0).cos() - a;
        out[1] = q * ((t + 2.0 * std::f64::consts::PI) / 3.0).cos() - a;
        out[2] = q * ((t - 2.0 * std::f64::consts::PI) / 3.0).cos() - a;
        3
    } else {
        let u = if r < 0.0 { 1.0 } else { -1.0 } * (r.abs() + (r2 - q3).sqrt()).cbrt();
        let v = if u == 0.0 { 0.0 } else { q / u };
        out[0] = u + v - a;
        if u == v || (u - v).abs() < 1.0e-12 * (u + v).abs() {
            out[1] = -0.5 * (u + v) - a;
            2
        } else {
            1
        }
    }
}

fn non_zero_sign(value: f64) -> f64 {
    if value > 0.0 { 1.0 } else { -1.0 }
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
            color: EdgeColor::RED,
        };

        assert!(
            (line
                .signed_distance_to(Point::new(5.0, 3.0), 1.0)
                .0
                .distance
                .abs()
                - 3.0)
                .abs()
                < 1.0e-6
        );
        assert!(
            (line
                .signed_distance_to(Point::new(13.0, 4.0), 1.0)
                .0
                .distance
                .abs()
                - 5.0)
                .abs()
                < 1.0e-6
        );
    }

    #[test]
    fn contour_contains_points() {
        let contour = Contour {
            fill_rule: FillRule::NonZero,
            segments: vec![
                Segment::Line {
                    p0: Point::new(0.0, 0.0),
                    p1: Point::new(10.0, 0.0),
                    color: EdgeColor::RED,
                },
                Segment::Line {
                    p0: Point::new(10.0, 0.0),
                    p1: Point::new(10.0, 10.0),
                    color: EdgeColor::GREEN,
                },
                Segment::Line {
                    p0: Point::new(10.0, 10.0),
                    p1: Point::new(0.0, 10.0),
                    color: EdgeColor::BLUE,
                },
                Segment::Line {
                    p0: Point::new(0.0, 10.0),
                    p1: Point::new(0.0, 0.0),
                    color: EdgeColor::RED,
                },
            ],
        };

        assert!(contour.contains(Point::new(5.0, 5.0)));
        assert!(!contour.contains(Point::new(15.0, 5.0)));
    }

    #[test]
    fn sharp_contours_use_multichannel_edge_colors() {
        let mut shape = Shape {
            contours: vec![Contour {
                fill_rule: FillRule::NonZero,
                segments: vec![
                    Segment::Line {
                        p0: Point::new(0.0, 0.0),
                        p1: Point::new(10.0, 0.0),
                        color: EdgeColor::WHITE,
                    },
                    Segment::Line {
                        p0: Point::new(10.0, 0.0),
                        p1: Point::new(10.0, 10.0),
                        color: EdgeColor::WHITE,
                    },
                    Segment::Line {
                        p0: Point::new(10.0, 10.0),
                        p1: Point::new(0.0, 10.0),
                        color: EdgeColor::WHITE,
                    },
                    Segment::Line {
                        p0: Point::new(0.0, 10.0),
                        p1: Point::new(0.0, 0.0),
                        color: EdgeColor::WHITE,
                    },
                ],
            }],
        };

        shape.color_edges();
        let colors: Vec<_> = shape.contours[0]
            .segments
            .iter()
            .map(Segment::color)
            .collect();

        assert!(colors.iter().all(|color| color.0.count_ones() == 2));
        assert_ne!(colors[0], colors[1]);
        assert_ne!(colors[3], colors[0]);
    }

    #[test]
    fn signed_distance_tracks_edge_side() {
        let line = Segment::Line {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(10.0, 0.0),
            color: EdgeColor::WHITE,
        };

        let above = line
            .signed_distance_to(Point::new(5.0, 2.0), 1.0)
            .0
            .distance;
        let below = line
            .signed_distance_to(Point::new(5.0, -2.0), 1.0)
            .0
            .distance;

        assert!(above * below < 0.0);
        assert!((above.abs() - 2.0).abs() < 1.0e-6);
        assert!((below.abs() - 2.0).abs() < 1.0e-6);
    }
}
