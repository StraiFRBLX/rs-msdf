use rs_msdf::{DistanceFieldMode, MsdfOptions, generate_from_svg};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;

const SIZE: u32 = 64;
const RANGE_PX: f64 = 4.0;
const MEAN_ERROR_LIMIT: f64 = 0.14;
const MAX_REGION_AREA: usize = 96;
const HIGH_ERROR_THRESHOLD: f64 = 0.40;

#[derive(Debug, Deserialize)]
struct IconSet {
    width: Option<u32>,
    height: Option<u32>,
    icons: HashMap<String, Icon>,
}

#[derive(Debug, Deserialize)]
struct Icon {
    body: String,
    width: Option<u32>,
    height: Option<u32>,
}

#[test]
fn majesticons_render_truth_visual_subset() {
    let fixture = include_str!("fixtures/majesticons.json");
    let iconset: IconSet = serde_json::from_str(fixture).unwrap();
    let names = selected_icons(&iconset);
    let mut failures = Vec::new();

    for name in names {
        let icon = iconset
            .icons
            .get(&name)
            .unwrap_or_else(|| panic!("fixture is missing icon `{name}`"));
        let svg = build_svg(&iconset, icon);
        let output = generate_from_svg(
            svg.as_bytes(),
            MsdfOptions::new(SIZE, SIZE, RANGE_PX)
                .unwrap()
                .with_mode(DistanceFieldMode::Msdf),
        )
        .unwrap_or_else(|error| panic!("failed to generate `{name}`: {error}"));

        let reference = render_reference_alpha(&svg, output.metadata.scale, output.metadata.translation);
        let reconstruction = reconstruct_alpha(&output.pixels);
        let report = compare_alpha(&reference, &reconstruction);
        if report.mean_error > MEAN_ERROR_LIMIT || report.max_region_area > MAX_REGION_AREA {
            write_failure_sheet(&name, &reference, &reconstruction).unwrap();
            failures.push(format!(
                "{name}: mean {:.4}, max high-error region {} px",
                report.mean_error, report.max_region_area
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "iconset visual failures:\n{}",
        failures.join("\n")
    );
}

fn selected_icons(iconset: &IconSet) -> Vec<String> {
    if std::env::var_os("RS_MSDF_ICONSET_FULL").is_some() {
        let mut names = iconset.icons.keys().cloned().collect::<Vec<_>>();
        names.sort();
        names
    } else {
        [
            "monitor",
            "chats-2",
            "home",
            "academic-cap",
            "airplane",
            "send-line",
        ]
        .into_iter()
        .filter(|name| iconset.icons.contains_key(*name))
        .map(str::to_string)
        .collect()
    }
}

fn build_svg(iconset: &IconSet, icon: &Icon) -> String {
    let width = icon.width.or(iconset.width).unwrap_or(24);
    let height = icon.height.or(iconset.height).unwrap_or(24);
    let body = icon.body.replace("currentColor", "#000");
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" color="#000">{body}</svg>"##
    )
}

fn render_reference_alpha(svg: &str, scale: f64, translation: [f64; 2]) -> Vec<f64> {
    let mut options = usvg::Options::default();
    options.style_sheet = Some("* { color: #000000; }".to_string());
    let tree = usvg::Tree::from_data(svg.as_bytes(), &options).unwrap();
    let mut pixmap = tiny_skia::Pixmap::new(SIZE, SIZE).unwrap();
    let transform = tiny_skia::Transform::from_row(
        scale as f32,
        0.0,
        0.0,
        scale as f32,
        translation[0] as f32,
        translation[1] as f32,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap
        .pixels()
        .iter()
        .map(|pixel| f64::from(pixel.alpha()) / 255.0)
        .collect()
}

fn reconstruct_alpha(pixels: &[u8]) -> Vec<f64> {
    pixels
        .chunks_exact(3)
        .map(|pixel| {
            let mut channels = [
                f64::from(pixel[0]) / 255.0,
                f64::from(pixel[1]) / 255.0,
                f64::from(pixel[2]) / 255.0,
            ];
            channels.sort_by(f64::total_cmp);
            ((channels[1] - 0.5) * RANGE_PX + 0.5).clamp(0.0, 1.0)
        })
        .collect()
}

#[derive(Debug)]
struct CompareReport {
    mean_error: f64,
    max_region_area: usize,
}

fn compare_alpha(reference: &[f64], reconstruction: &[f64]) -> CompareReport {
    let errors = reference
        .iter()
        .zip(reconstruction)
        .map(|(reference, reconstruction)| (reference - reconstruction).abs())
        .collect::<Vec<_>>();
    let mean_error = errors.iter().sum::<f64>() / errors.len() as f64;
    let max_region_area = largest_high_error_region(&errors);
    CompareReport {
        mean_error,
        max_region_area,
    }
}

fn largest_high_error_region(errors: &[f64]) -> usize {
    let mut visited = vec![false; errors.len()];
    let mut largest = 0;

    for index in 0..errors.len() {
        if visited[index] || errors[index] <= HIGH_ERROR_THRESHOLD {
            continue;
        }

        let mut area = 0;
        let mut queue = VecDeque::from([index]);
        visited[index] = true;

        while let Some(current) = queue.pop_front() {
            area += 1;
            let x = current % SIZE as usize;
            let y = current / SIZE as usize;
            let neighbors = [
                (x > 0).then_some(current - 1),
                (x + 1 < SIZE as usize).then_some(current + 1),
                (y > 0).then_some(current - SIZE as usize),
                (y + 1 < SIZE as usize).then_some(current + SIZE as usize),
            ];

            for neighbor in neighbors.into_iter().flatten() {
                if !visited[neighbor] && errors[neighbor] > HIGH_ERROR_THRESHOLD {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }

        largest = largest.max(area);
    }

    largest
}

fn write_failure_sheet(name: &str, reference: &[f64], reconstruction: &[f64]) -> std::io::Result<()> {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/iconset-visual");
    fs::create_dir_all(&out_dir)?;
    let mut sheet = vec![0_u8; (SIZE * 3 * SIZE * 4) as usize];

    for y in 0..SIZE as usize {
        for x in 0..SIZE as usize {
            let index = y * SIZE as usize + x;
            let reference_px = (reference[index] * 255.0).round() as u8;
            let reconstruction_px = (reconstruction[index] * 255.0).round() as u8;
            let diff_px = ((reference[index] - reconstruction[index]).abs() * 255.0).round() as u8;
            write_rgba(&mut sheet, x, y, 0, reference_px, reference_px, reference_px);
            write_rgba(
                &mut sheet,
                x + SIZE as usize,
                y,
                0,
                reconstruction_px,
                reconstruction_px,
                reconstruction_px,
            );
            write_rgba(&mut sheet, x + SIZE as usize * 2, y, 0, diff_px, 0, 255 - diff_px);
        }
    }

    let path = out_dir.join(format!("{name}.png"));
    let file = fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, SIZE * 3, SIZE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder.write_header()?;
    png_writer.write_image_data(&sheet)?;
    Ok(())
}

fn write_rgba(sheet: &mut [u8], x: usize, y: usize, panel_gap: usize, r: u8, g: u8, b: u8) {
    let width = SIZE as usize * 3 + panel_gap;
    let offset = (y * width + x) * 4;
    sheet[offset] = r;
    sheet[offset + 1] = g;
    sheet[offset + 2] = b;
    sheet[offset + 3] = 255;
}
