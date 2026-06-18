use std::hint::black_box;
use std::time::{Duration, Instant};

use rs_msdf::{DistanceFieldMode, MsdfOptions, generate_from_svg};

const SIMPLE_SVG: &[u8] = br#"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
  <path d="M1 1 H9 V9 H1 Z" fill="black"/>
</svg>
"#;

const HOME_SAME_FILL_STROKE: &[u8] = include_bytes!("../tests/fixtures/home-same-fill-stroke.svg");
const MONOSPACE_OVERLAP: &[u8] = include_bytes!("../tests/fixtures/monospace-overlap.svg");

fn main() {
    let cases = [
        (
            "simple-msdf-64",
            SIMPLE_SVG,
            MsdfOptions::new(64, 64, 4.0).unwrap(),
            64,
        ),
        (
            "home-msdf-96",
            HOME_SAME_FILL_STROKE,
            MsdfOptions::new(96, 96, 4.0).unwrap(),
            32,
        ),
        (
            "overlap-mtsdf-512x128",
            MONOSPACE_OVERLAP,
            MsdfOptions::new(512, 128, 4.0)
                .unwrap()
                .with_mode(DistanceFieldMode::Mtsdf),
            8,
        ),
    ];

    for (name, svg, options, iterations) in cases {
        let elapsed = time_case(svg, options, iterations);
        let per_iter = elapsed.as_secs_f64() * 1_000.0 / f64::from(iterations);
        println!("{name}: {per_iter:.3} ms/iter ({iterations} iterations)");
    }
}

fn time_case(svg: &[u8], options: MsdfOptions, iterations: u32) -> Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        let output = generate_from_svg(black_box(svg), black_box(options)).unwrap();
        black_box(output);
    }
    start.elapsed()
}
