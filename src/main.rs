use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use clap::Parser;
use rs_msdf::{generate_from_svg, MsdfOptions, Result};

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// Input SVG file.
    input: PathBuf,

    /// Output texture size, either N for square or WxH.
    #[arg(long, value_parser = parse_size)]
    size: (u32, u32),

    /// Signed distance range in output pixels.
    #[arg(long, default_value_t = 4.0)]
    range: f64,

    /// Output PNG file.
    #[arg(long, short)]
    output: PathBuf,

    /// Output JSON metadata file. Defaults to the PNG path with a .json extension.
    #[arg(long)]
    metadata: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let svg = std::fs::read(&args.input)?;
    let options = MsdfOptions::new(args.size.0, args.size.1, args.range)?;
    let output = generate_from_svg(&svg, options)?;

    write_png(
        &args.output,
        output.width,
        output.height,
        &output.rgb_pixels,
    )?;

    let metadata_path = args
        .metadata
        .unwrap_or_else(|| default_metadata_path(&args.output));
    let metadata = serde_json::to_vec_pretty(&output.metadata)?;
    std::fs::write(metadata_path, metadata)?;

    Ok(())
}

fn write_png(path: &Path, width: u32, height: u32, rgb_pixels: &[u8]) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgb_pixels)?;
    Ok(())
}

fn default_metadata_path(output: &Path) -> PathBuf {
    output.with_extension("json")
}

fn parse_size(value: &str) -> std::result::Result<(u32, u32), String> {
    if let Some((width, height)) = value.split_once(['x', 'X']) {
        let width = parse_dimension(width)?;
        let height = parse_dimension(height)?;
        Ok((width, height))
    } else {
        let size = parse_dimension(value)?;
        Ok((size, size))
    }
}

fn parse_dimension(value: &str) -> std::result::Result<u32, String> {
    let dimension = value
        .parse::<u32>()
        .map_err(|_| format!("invalid texture dimension `{value}`"))?;
    if dimension == 0 {
        Err("texture dimensions must be greater than zero".to_string())
    } else {
        Ok(dimension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs_msdf::Error;

    #[test]
    fn parses_square_size() {
        assert_eq!(parse_size("64").unwrap(), (64, 64));
    }

    #[test]
    fn parses_rectangular_size() {
        assert_eq!(parse_size("128x96").unwrap(), (128, 96));
        assert_eq!(parse_size("128X96").unwrap(), (128, 96));
    }

    #[test]
    fn rejects_zero_size() {
        assert!(parse_size("0").is_err());
        assert!(matches!(
            MsdfOptions::new(0, 1, 4.0),
            Err(Error::InvalidOptions(_))
        ));
    }
}
