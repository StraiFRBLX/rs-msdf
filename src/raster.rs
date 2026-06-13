use rayon::prelude::*;

use crate::error::{Error, Result};
use crate::geometry::{Contour, Point, Segment, Shape, SignedDistance};
use crate::metadata::Bounds;
use crate::{DistanceFieldMode, MsdfOptions};

const MIN_DEVIATION_RATIO: f64 = 1.111_111_111_111_111_2;
const PROTECTION_RADIUS_TOLERANCE: f64 = 1.001;
const STENCIL_ERROR: u8 = 1;
const STENCIL_PROTECTED: u8 = 2;
const ARTIFACT_T_EPSILON: f64 = 0.01;
const CANDIDATE: u8 = 1;
const ARTIFACT: u8 = 2;

pub(crate) struct RasterizedMsdf {
    pub pixels: Vec<u8>,
    pub channels: usize,
    pub geometry_bounds: Bounds,
    pub scale: f64,
    pub translation: [f64; 2],
}

pub(crate) fn render_msdf(shape: &Shape, options: MsdfOptions) -> Result<RasterizedMsdf> {
    let geometry_bounds = shape.bounds().ok_or(Error::EmptyGeometry)?;
    if geometry_bounds.width() <= 0.0 || geometry_bounds.height() <= 0.0 {
        return Err(Error::EmptyGeometry);
    }

    let fit_width = (f64::from(options.width) - options.range_px).max(1.0);
    let fit_height = (f64::from(options.height) - options.range_px).max(1.0);
    let scale = (fit_width / geometry_bounds.width()).min(fit_height / geometry_bounds.height());
    let fitted_width = geometry_bounds.width() * scale;
    let fitted_height = geometry_bounds.height() * scale;
    let projection = Projection {
        scale,
        translation: [
            (f64::from(options.width) - fitted_width) * 0.5 - geometry_bounds.min_x * scale,
            (f64::from(options.height) - fitted_height) * 0.5 - geometry_bounds.min_y * scale,
        ],
    };

    let channels = options.mode.channels();
    let width = options.width as usize;
    let height = options.height as usize;
    let pixel_count = width * height;
    let mut values = vec![0.0_f64; pixel_count * channels];
    let range = DistanceRange::symmetric(options.range_px);

    values
        .par_chunks_mut(channels)
        .enumerate()
        .for_each(|(index, pixel)| {
            let x = index as u32 % options.width;
            let y = index as u32 / options.width;
            let texture_point = Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let shape_point = projection.unproject(texture_point);
            let distance = shape_distance(shape, shape_point, options.mode);

            match options.mode {
                DistanceFieldMode::Sdf => {
                    pixel[0] = range.map(distance.true_distance * scale);
                }
                DistanceFieldMode::Psdf => {
                    pixel[0] = range.map(distance.pseudo_distance * scale);
                }
                DistanceFieldMode::Msdf => {
                    pixel[0] = range.map(distance.multi[0] * scale);
                    pixel[1] = range.map(distance.multi[1] * scale);
                    pixel[2] = range.map(distance.multi[2] * scale);
                }
                DistanceFieldMode::Mtsdf => {
                    pixel[0] = range.map(distance.multi[0] * scale);
                    pixel[1] = range.map(distance.multi[1] * scale);
                    pixel[2] = range.map(distance.multi[2] * scale);
                    pixel[3] = range.map(distance.true_distance * scale);
                }
            }
        });

    if channels >= 3 {
        correct_msdf_errors(
            &mut values,
            width,
            height,
            channels,
            options.range_px,
            shape,
            projection,
        );
    }

    Ok(RasterizedMsdf {
        pixels: values.into_iter().map(encode_value).collect(),
        channels,
        geometry_bounds,
        scale,
        translation: projection.translation,
    })
}

#[derive(Debug, Clone, Copy)]
struct DistanceRange {
    lower: f64,
    upper: f64,
}

impl DistanceRange {
    fn symmetric(width: f64) -> Self {
        Self {
            lower: -0.5 * width,
            upper: 0.5 * width,
        }
    }

    fn map(self, distance_px: f64) -> f64 {
        ((distance_px - self.lower) / (self.upper - self.lower)).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy)]
struct Projection {
    scale: f64,
    translation: [f64; 2],
}

impl Projection {
    fn project(self, p: Point) -> Point {
        Point::new(
            p.x * self.scale + self.translation[0],
            p.y * self.scale + self.translation[1],
        )
    }

    fn unproject(self, p: Point) -> Point {
        Point::new(
            (p.x - self.translation[0]) / self.scale,
            (p.y - self.translation[1]) / self.scale,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct DistanceSet {
    true_distance: f64,
    pseudo_distance: f64,
    multi: [f64; 3],
}

impl DistanceSet {
    fn scalar(self, mode: DistanceFieldMode) -> f64 {
        match mode {
            DistanceFieldMode::Sdf => self.true_distance,
            DistanceFieldMode::Psdf => self.pseudo_distance,
            DistanceFieldMode::Msdf | DistanceFieldMode::Mtsdf => median(self.multi),
        }
    }

    fn align_to_sign(mut self, sign: f64, mode: DistanceFieldMode) -> Self {
        if self.scalar(mode).signum() != sign.signum() {
            self.true_distance = -self.true_distance;
            self.pseudo_distance = -self.pseudo_distance;
            self.multi
                .iter_mut()
                .for_each(|distance| *distance = -*distance);
        }
        self
    }
}

fn shape_distance(shape: &Shape, p: Point, mode: DistanceFieldMode) -> DistanceSet {
    let shape_sign = if shape.contains(p) { 1.0 } else { -1.0 };
    let mut contour_distances = Vec::with_capacity(shape.contours.len());
    let mut shape_distance = None;
    let mut inner_distance = None;
    let mut outer_distance = None;

    for contour in &shape.contours {
        if contour.segments.is_empty() {
            continue;
        }

        let winding = contour.winding();
        let distance = contour_distance(contour, p, winding);
        let scalar = distance.scalar(mode);
        merge_distance(&mut shape_distance, distance, scalar);

        if winding > 0 && scalar >= 0.0 {
            merge_distance(&mut inner_distance, distance, scalar);
        }
        if winding < 0 && scalar <= 0.0 {
            merge_distance(&mut outer_distance, distance, scalar);
        }

        contour_distances.push((winding, distance, scalar));
    }

    let Some(shape_distance) = shape_distance.map(|(distance, _)| distance) else {
        return DistanceSet {
            true_distance: 0.0,
            pseudo_distance: 0.0,
            multi: [0.0; 3],
        };
    };

    let selected = match (inner_distance, outer_distance) {
        (Some((inner, inner_scalar)), Some((outer, outer_scalar))) => {
            if inner_scalar >= 0.0 && inner_scalar.abs() <= outer_scalar.abs() {
                refine_overlapping_distance(&contour_distances, inner, 1, outer_scalar, mode)
            } else if outer_scalar <= 0.0 && outer_scalar.abs() < inner_scalar.abs() {
                refine_overlapping_distance(&contour_distances, outer, -1, inner_scalar, mode)
            } else {
                shape_distance
            }
        }
        (Some((inner, inner_scalar)), None) if inner_scalar >= 0.0 => {
            refine_overlapping_distance(&contour_distances, inner, 1, f64::INFINITY, mode)
        }
        (None, Some((outer, outer_scalar))) if outer_scalar <= 0.0 => {
            refine_overlapping_distance(&contour_distances, outer, -1, f64::INFINITY, mode)
        }
        _ => shape_distance,
    };

    selected.align_to_sign(shape_sign, mode)
}

fn merge_distance(slot: &mut Option<(DistanceSet, f64)>, distance: DistanceSet, scalar: f64) {
    if slot.is_none_or(|(_, current)| scalar.abs() < current.abs()) {
        *slot = Some((distance, scalar));
    }
}

fn refine_overlapping_distance(
    contour_distances: &[(i32, DistanceSet, f64)],
    mut selected: DistanceSet,
    winding: i32,
    opposite_scalar: f64,
    mode: DistanceFieldMode,
) -> DistanceSet {
    let mut selected_scalar = selected.scalar(mode);

    for &(contour_winding, distance, scalar) in contour_distances {
        if contour_winding == winding {
            let closer_than_opposite = scalar.abs() < opposite_scalar.abs();
            let better_inside = winding > 0 && scalar > selected_scalar;
            let better_outside = winding < 0 && scalar < selected_scalar;
            if closer_than_opposite && (better_inside || better_outside) {
                selected = distance;
                selected_scalar = scalar;
            }
        }
    }

    for &(contour_winding, distance, scalar) in contour_distances {
        if contour_winding != winding
            && scalar * selected_scalar >= 0.0
            && scalar.abs() < selected_scalar.abs()
        {
            selected = distance;
            selected_scalar = scalar;
        }
    }

    selected
}

fn contour_distance(contour: &Contour, p: Point, winding: i32) -> DistanceSet {
    let contour_sign = if contour.contains(p) {
        f64::from(winding)
    } else {
        -f64::from(winding)
    };

    let mut true_selector = ClosestSelector::default();
    let mut pseudo_selector = PerpendicularSelector::default();
    let mut multi_selectors = [
        PerpendicularSelector::default(),
        PerpendicularSelector::default(),
        PerpendicularSelector::default(),
    ];

    let edge_count = contour.segments.len();
    for index in 0..edge_count {
        let edge = &contour.segments[index];
        let prev = &contour.segments[(index + edge_count - 1) % edge_count];
        let next = &contour.segments[(index + 1) % edge_count];
        let (distance, param) = edge.signed_distance_to(p, contour_sign);
        let distance =
            SignedDistance::new(align_sign(distance.distance, contour_sign), distance.dot);
        let sample = EdgeSample {
            edge,
            distance,
            param,
        };

        true_selector.add(sample);
        pseudo_selector.add(sample, prev, next, p);

        let color = edge.color();
        if color.has_red() {
            multi_selectors[0].add(sample, prev, next, p);
        }
        if color.has_green() {
            multi_selectors[1].add(sample, prev, next, p);
        }
        if color.has_blue() {
            multi_selectors[2].add(sample, prev, next, p);
        }
    }

    let true_distance = true_selector
        .best
        .map(|sample| sample.distance.distance)
        .unwrap_or(0.0);
    let pseudo_distance = pseudo_selector.distance(p);
    let mut multi = multi_selectors.map(|selector| selector.distance(p));

    if median(multi).signum() != contour_sign.signum() {
        multi.iter_mut().for_each(|distance| *distance = -*distance);
    }

    DistanceSet {
        true_distance,
        pseudo_distance,
        multi,
    }
}

#[derive(Clone, Copy)]
struct EdgeSample<'a> {
    edge: &'a Segment,
    distance: SignedDistance,
    param: f64,
}

#[derive(Default, Clone, Copy)]
struct ClosestSelector<'a> {
    best: Option<EdgeSample<'a>>,
}

impl<'a> ClosestSelector<'a> {
    fn add(&mut self, sample: EdgeSample<'a>) {
        if self
            .best
            .is_none_or(|best| sample.distance.is_closer_than(best.distance))
        {
            self.best = Some(sample);
        }
    }
}

#[derive(Clone, Copy)]
struct PerpendicularSelector<'a> {
    true_selector: ClosestSelector<'a>,
    min_negative_perpendicular_distance: f64,
    min_positive_perpendicular_distance: f64,
}

impl Default for PerpendicularSelector<'_> {
    fn default() -> Self {
        Self {
            true_selector: ClosestSelector::default(),
            min_negative_perpendicular_distance: f64::NEG_INFINITY,
            min_positive_perpendicular_distance: f64::INFINITY,
        }
    }
}

impl<'a> PerpendicularSelector<'a> {
    fn add(&mut self, sample: EdgeSample<'a>, prev: &Segment, next: &Segment, p: Point) {
        self.true_selector.add(sample);

        let edge = sample.edge;
        let ap = p.sub(edge.start());
        let bp = p.sub(edge.end());
        let a_dir = edge.direction_start();
        let b_dir = edge.direction_end();
        let prev_dir = prev.direction_end();
        let next_dir = next.direction_start();
        let a_domain_distance = ap.dot(prev_dir.add(a_dir).normalize());
        let b_domain_distance = -bp.dot(b_dir.add(next_dir).normalize());

        if a_domain_distance > 0.0 {
            let mut pd = sample.distance.distance;
            if get_perpendicular_distance(&mut pd, ap, a_dir.scale(-1.0)) {
                self.add_perpendicular_distance(-pd);
            }
        }

        if b_domain_distance > 0.0 {
            let mut pd = sample.distance.distance;
            if get_perpendicular_distance(&mut pd, bp, b_dir) {
                self.add_perpendicular_distance(pd);
            }
        }
    }

    fn add_perpendicular_distance(&mut self, distance: f64) {
        if distance <= 0.0 && distance > self.min_negative_perpendicular_distance {
            self.min_negative_perpendicular_distance = distance;
        }
        if distance >= 0.0 && distance < self.min_positive_perpendicular_distance {
            self.min_positive_perpendicular_distance = distance;
        }
    }

    fn distance(self, p: Point) -> f64 {
        let Some(sample) = self.true_selector.best else {
            return 0.0;
        };

        let mut min_distance = if sample.distance.distance < 0.0 {
            finite_or(
                self.min_negative_perpendicular_distance,
                sample.distance.distance,
            )
        } else {
            finite_or(
                self.min_positive_perpendicular_distance,
                sample.distance.distance,
            )
        };

        let pseudo_distance = sample
            .edge
            .pseudo_distance(sample.distance, p, sample.param);
        if pseudo_distance.abs() < min_distance.abs() {
            min_distance = pseudo_distance;
        }
        min_distance
    }
}

fn get_perpendicular_distance(distance: &mut f64, ep: Point, edge_dir: Point) -> bool {
    if ep.dot(edge_dir) > 0.0 {
        let perpendicular = ep.cross(edge_dir);
        if perpendicular.abs() < distance.abs() {
            *distance = perpendicular;
            return true;
        }
    }
    false
}

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

fn align_sign(distance: f64, sign: f64) -> f64 {
    if distance == 0.0 || distance.signum() == sign.signum() {
        distance
    } else {
        -distance
    }
}

fn encode_value(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn correct_msdf_errors(
    values: &mut [f64],
    width: usize,
    height: usize,
    channels: usize,
    range_px: f64,
    shape: &Shape,
    projection: Projection,
) {
    let mut stencil = vec![0_u8; width * height];
    protect_corners(&mut stencil, width, height, shape, projection);
    protect_edges(&mut stencil, values, width, height, channels, range_px);
    find_errors(&mut stencil, values, width, height, channels, range_px);
    apply_error_correction(&stencil, values, channels);
}

fn protect_corners(
    stencil: &mut [u8],
    width: usize,
    height: usize,
    shape: &Shape,
    projection: Projection,
) {
    for contour in &shape.contours {
        if contour.segments.is_empty() {
            continue;
        }

        let edge_count = contour.segments.len();
        for index in 0..edge_count {
            let prev = &contour.segments[(index + edge_count - 1) % edge_count];
            let edge = &contour.segments[index];
            let common_color = prev.color().bits() & edge.color().bits();
            if common_color & common_color.saturating_sub(1) == 0 {
                let p = projection.project(edge.start());
                let left = (p.x - 0.5).floor() as isize;
                let bottom = (p.y - 0.5).floor() as isize;
                mark_stencil(stencil, width, height, left, bottom, STENCIL_PROTECTED);
                mark_stencil(stencil, width, height, left + 1, bottom, STENCIL_PROTECTED);
                mark_stencil(stencil, width, height, left, bottom + 1, STENCIL_PROTECTED);
                mark_stencil(
                    stencil,
                    width,
                    height,
                    left + 1,
                    bottom + 1,
                    STENCIL_PROTECTED,
                );
            }
        }
    }
}

fn protect_edges(
    stencil: &mut [u8],
    values: &[f64],
    width: usize,
    height: usize,
    channels: usize,
    range_px: f64,
) {
    let axial_radius = PROTECTION_RADIUS_TOLERANCE / range_px;
    let diagonal_radius = axial_radius * 2.0_f64.sqrt();

    for y in 0..height {
        for x in 0..width.saturating_sub(1) {
            let left = pixel(values, width, channels, x, y);
            let right = pixel(values, width, channels, x + 1, y);
            protect_edge_pair(stencil, width, x, y, left, right, axial_radius);
            protect_edge_pair(stencil, width, x + 1, y, right, left, axial_radius);
        }
    }

    for y in 0..height.saturating_sub(1) {
        for x in 0..width {
            let bottom = pixel(values, width, channels, x, y);
            let top = pixel(values, width, channels, x, y + 1);
            protect_edge_pair(stencil, width, x, y, bottom, top, axial_radius);
            protect_edge_pair(stencil, width, x, y + 1, top, bottom, axial_radius);
        }
    }

    for y in 0..height.saturating_sub(1) {
        for x in 0..width.saturating_sub(1) {
            let lb = pixel(values, width, channels, x, y);
            let rb = pixel(values, width, channels, x + 1, y);
            let lt = pixel(values, width, channels, x, y + 1);
            let rt = pixel(values, width, channels, x + 1, y + 1);
            protect_edge_pair(stencil, width, x, y, lb, rt, diagonal_radius);
            protect_edge_pair(stencil, width, x + 1, y + 1, rt, lb, diagonal_radius);
            protect_edge_pair(stencil, width, x + 1, y, rb, lt, diagonal_radius);
            protect_edge_pair(stencil, width, x, y + 1, lt, rb, diagonal_radius);
        }
    }
}

fn protect_edge_pair(
    stencil: &mut [u8],
    width: usize,
    x: usize,
    y: usize,
    a: &[f64],
    b: &[f64],
    radius: f64,
) {
    let am = median([a[0], a[1], a[2]]);
    let bm = median([b[0], b[1], b[2]]);
    if (am - 0.5).abs() + (bm - 0.5).abs() < radius {
        let mask = edge_between_texels(a, b);
        if (mask & 0b001 != 0 && a[0] != am)
            || (mask & 0b010 != 0 && a[1] != am)
            || (mask & 0b100 != 0 && a[2] != am)
        {
            stencil[y * width + x] |= STENCIL_PROTECTED;
        }
    }
}

fn edge_between_texels(a: &[f64], b: &[f64]) -> u8 {
    (edge_between_texels_channel(a, b, 0) as u8)
        | ((edge_between_texels_channel(a, b, 1) as u8) << 1)
        | ((edge_between_texels_channel(a, b, 2) as u8) << 2)
}

fn edge_between_texels_channel(a: &[f64], b: &[f64], channel: usize) -> bool {
    let denominator = a[channel] - b[channel];
    if denominator.abs() <= f64::EPSILON {
        return false;
    }

    let t = (a[channel] - 0.5) / denominator;
    if t > 0.0 && t < 1.0 {
        let interpolated = [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)];
        (median(interpolated) - interpolated[channel]).abs() <= 1.0e-12
    } else {
        false
    }
}

fn find_errors(
    stencil: &mut [u8],
    values: &[f64],
    width: usize,
    height: usize,
    channels: usize,
    range_px: f64,
) {
    let h_span = MIN_DEVIATION_RATIO / range_px;
    let v_span = MIN_DEVIATION_RATIO / range_px;
    let d_span = MIN_DEVIATION_RATIO * 2.0_f64.sqrt() / range_px;

    for y in 0..height {
        for x in 0..width {
            let c = pixel(values, width, channels, x, y);
            let cm = median([c[0], c[1], c[2]]);
            let protected = stencil[y * width + x] & STENCIL_PROTECTED != 0;
            let classifier_h = ArtifactClassifier::new(h_span, protected);
            let classifier_v = ArtifactClassifier::new(v_span, protected);
            let classifier_d = ArtifactClassifier::new(d_span, protected);

            let left = (x > 0).then(|| pixel(values, width, channels, x - 1, y));
            let right = (x + 1 < width).then(|| pixel(values, width, channels, x + 1, y));
            let bottom = (y > 0).then(|| pixel(values, width, channels, x, y - 1));
            let top = (y + 1 < height).then(|| pixel(values, width, channels, x, y + 1));

            let has_error = left.is_some_and(|l| has_linear_artifact(classifier_h, cm, c, l))
                || bottom.is_some_and(|b| has_linear_artifact(classifier_v, cm, c, b))
                || right.is_some_and(|r| has_linear_artifact(classifier_h, cm, c, r))
                || top.is_some_and(|t| has_linear_artifact(classifier_v, cm, c, t))
                || (x > 0
                    && y > 0
                    && has_diagonal_artifact(
                        classifier_d,
                        cm,
                        c,
                        left.unwrap(),
                        bottom.unwrap(),
                        pixel(values, width, channels, x - 1, y - 1),
                    ))
                || (x + 1 < width
                    && y > 0
                    && has_diagonal_artifact(
                        classifier_d,
                        cm,
                        c,
                        right.unwrap(),
                        bottom.unwrap(),
                        pixel(values, width, channels, x + 1, y - 1),
                    ))
                || (x > 0
                    && y + 1 < height
                    && has_diagonal_artifact(
                        classifier_d,
                        cm,
                        c,
                        left.unwrap(),
                        top.unwrap(),
                        pixel(values, width, channels, x - 1, y + 1),
                    ))
                || (x + 1 < width
                    && y + 1 < height
                    && has_diagonal_artifact(
                        classifier_d,
                        cm,
                        c,
                        right.unwrap(),
                        top.unwrap(),
                        pixel(values, width, channels, x + 1, y + 1),
                    ));

            if has_error {
                stencil[y * width + x] |= STENCIL_ERROR;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ArtifactClassifier {
    span: f64,
    protected: bool,
}

impl ArtifactClassifier {
    fn new(span: f64, protected: bool) -> Self {
        Self { span, protected }
    }

    fn range_test(self, at: f64, bt: f64, xt: f64, am: f64, bm: f64, xm: f64) -> u8 {
        let inversion = (am > 0.5 && bm > 0.5 && xm <= 0.5) || (am < 0.5 && bm < 0.5 && xm >= 0.5);
        let extreme = (median([am, bm, xm]) - xm).abs() > 1.0e-12;

        if inversion || (!self.protected && extreme) {
            let ax_span = (xt - at) * self.span;
            let bx_span = (bt - xt) * self.span;
            let in_expected_range = xm >= am - ax_span
                && xm <= am + ax_span
                && xm >= bm - bx_span
                && xm <= bm + bx_span;
            if !in_expected_range {
                return CANDIDATE | ARTIFACT;
            }
            return CANDIDATE;
        }

        0
    }

    fn evaluate(self, flags: u8) -> bool {
        flags & ARTIFACT != 0
    }
}

fn has_linear_artifact(classifier: ArtifactClassifier, am: f64, a: &[f64], b: &[f64]) -> bool {
    let bm = median([b[0], b[1], b[2]]);
    (am - 0.5).abs() >= (bm - 0.5).abs()
        && (has_linear_artifact_inner(classifier, am, bm, a, b, a[1] - a[0], b[1] - b[0])
            || has_linear_artifact_inner(classifier, am, bm, a, b, a[2] - a[1], b[2] - b[1])
            || has_linear_artifact_inner(classifier, am, bm, a, b, a[0] - a[2], b[0] - b[2]))
}

fn has_linear_artifact_inner(
    classifier: ArtifactClassifier,
    am: f64,
    bm: f64,
    a: &[f64],
    b: &[f64],
    da: f64,
    db: f64,
) -> bool {
    let denominator = da - db;
    if denominator.abs() <= f64::EPSILON {
        return false;
    }

    let t = da / denominator;
    if t > ARTIFACT_T_EPSILON && t < 1.0 - ARTIFACT_T_EPSILON {
        let xm = interpolated_median(a, b, t);
        let flags = classifier.range_test(0.0, 1.0, t, am, bm, xm);
        classifier.evaluate(flags)
    } else {
        false
    }
}

fn has_diagonal_artifact(
    classifier: ArtifactClassifier,
    am: f64,
    a: &[f64],
    b: &[f64],
    c: &[f64],
    d: &[f64],
) -> bool {
    let dm = median([d[0], d[1], d[2]]);
    if (am - 0.5).abs() < (dm - 0.5).abs() {
        return false;
    }

    let abc = [a[0] - b[0] - c[0], a[1] - b[1] - c[1], a[2] - b[2] - c[2]];
    let linear = [-a[0] - abc[0], -a[1] - abc[1], -a[2] - abc[2]];
    let quadratic = [d[0] + abc[0], d[1] + abc[1], d[2] + abc[2]];
    let t_extreme = [
        safe_extremum(linear[0], quadratic[0]),
        safe_extremum(linear[1], quadratic[1]),
        safe_extremum(linear[2], quadratic[2]),
    ];

    has_diagonal_artifact_inner(
        classifier,
        am,
        dm,
        a,
        linear,
        quadratic,
        a[1] - a[0],
        b[1] - b[0] + c[1] - c[0],
        d[1] - d[0],
        t_extreme[0],
        t_extreme[1],
    ) || has_diagonal_artifact_inner(
        classifier,
        am,
        dm,
        a,
        linear,
        quadratic,
        a[2] - a[1],
        b[2] - b[1] + c[2] - c[1],
        d[2] - d[1],
        t_extreme[1],
        t_extreme[2],
    ) || has_diagonal_artifact_inner(
        classifier,
        am,
        dm,
        a,
        linear,
        quadratic,
        a[0] - a[2],
        b[0] - b[2] + c[0] - c[2],
        d[0] - d[2],
        t_extreme[2],
        t_extreme[0],
    )
}

#[allow(clippy::too_many_arguments)]
fn has_diagonal_artifact_inner(
    classifier: ArtifactClassifier,
    am: f64,
    dm: f64,
    a: &[f64],
    linear: [f64; 3],
    quadratic: [f64; 3],
    da: f64,
    dbc: f64,
    dd: f64,
    t_ex0: f64,
    t_ex1: f64,
) -> bool {
    let mut solutions = [0.0; 3];
    let count = solve_quadratic(&mut solutions, dd - dbc + da, dbc - da - da, da);
    for &t in &solutions[..count.max(0) as usize] {
        if t > ARTIFACT_T_EPSILON && t < 1.0 - ARTIFACT_T_EPSILON {
            let xm = interpolated_median_quadratic(a, linear, quadratic, t);
            let mut flags = classifier.range_test(0.0, 1.0, t, am, dm, xm);
            flags |=
                diagonal_extreme_range_test(classifier, t, xm, am, dm, t_ex0, a, linear, quadratic);
            flags |=
                diagonal_extreme_range_test(classifier, t, xm, am, dm, t_ex1, a, linear, quadratic);
            if classifier.evaluate(flags) {
                return true;
            }
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn diagonal_extreme_range_test(
    classifier: ArtifactClassifier,
    t: f64,
    xm: f64,
    am: f64,
    dm: f64,
    t_ex: f64,
    a: &[f64],
    linear: [f64; 3],
    quadratic: [f64; 3],
) -> u8 {
    if !(t_ex > 0.0 && t_ex < 1.0) {
        return 0;
    }

    let mut t_end = [0.0, 1.0];
    let mut end_median = [am, dm];
    let slot = usize::from(t_ex > t);
    t_end[slot] = t_ex;
    end_median[slot] = interpolated_median_quadratic(a, linear, quadratic, t_ex);
    classifier.range_test(t_end[0], t_end[1], t, end_median[0], end_median[1], xm)
}

fn safe_extremum(linear: f64, quadratic: f64) -> f64 {
    if quadratic.abs() <= f64::EPSILON {
        f64::NAN
    } else {
        -0.5 * linear / quadratic
    }
}

fn apply_error_correction(stencil: &[u8], values: &mut [f64], channels: usize) {
    for (index, mask) in stencil.iter().enumerate() {
        if mask & STENCIL_ERROR != 0 {
            let offset = index * channels;
            let med = median([values[offset], values[offset + 1], values[offset + 2]]);
            values[offset] = med;
            values[offset + 1] = med;
            values[offset + 2] = med;
        }
    }
}

fn mark_stencil(stencil: &mut [u8], width: usize, height: usize, x: isize, y: isize, mask: u8) {
    if x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height {
        stencil[y as usize * width + x as usize] |= mask;
    }
}

fn pixel(values: &[f64], width: usize, channels: usize, x: usize, y: usize) -> &[f64] {
    let index = (y * width + x) * channels;
    &values[index..index + 3]
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn interpolated_median(a: &[f64], b: &[f64], t: f64) -> f64 {
    median([mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)])
}

fn interpolated_median_quadratic(a: &[f64], linear: [f64; 3], quadratic: [f64; 3], t: f64) -> f64 {
    median([
        t * (t * quadratic[0] + linear[0]) + a[0],
        t * (t * quadratic[1] + linear[1]) + a[1],
        t * (t * quadratic[2] + linear[2]) + a[2],
    ])
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

fn median([a, b, c]: [f64; 3]) -> f64 {
    a.max(b.min(c)).min(b.max(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_signed_distance_midpoint() {
        let range = DistanceRange::symmetric(4.0);
        assert_eq!(encode_value(range.map(0.0)), 128);
        assert_eq!(encode_value(range.map(-2.0)), 0);
        assert_eq!(encode_value(range.map(2.0)), 255);
    }

    #[test]
    fn median_returns_middle_value() {
        assert_eq!(median([3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median([-4.0, 8.0, 0.0]), 0.0);
    }
}
