use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use rs_msdf::{DistanceFieldMode, Error, MsdfJsonExport, MsdfOptions, Result, generate_from_svg};

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// Input SVG file.
    input: PathBuf,

    /// Output texture size, either N for square or WxH.
    #[arg(long, value_parser = parse_size)]
    size: (u32, u32),

    /// Distance field type to generate.
    #[arg(long, value_enum, default_value_t = CliMode::Msdf)]
    mode: CliMode,

    /// Signed distance range in output pixels.
    #[arg(long = "distance-range", alias = "range", default_value_t = 4.0)]
    distance_range: f64,

    /// Output file. Use .png for a PNG plus metadata sidecar, or .json for a self-contained data export.
    #[arg(long, short)]
    output: PathBuf,

    /// Output JSON metadata file for PNG output. Defaults to the PNG path with a .json extension.
    #[arg(long)]
    metadata: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Png,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliMode {
    Sdf,
    Psdf,
    Msdf,
    Mtsdf,
}

impl From<CliMode> for DistanceFieldMode {
    fn from(mode: CliMode) -> Self {
        match mode {
            CliMode::Sdf => Self::Sdf,
            CliMode::Psdf => Self::Psdf,
            CliMode::Msdf => Self::Msdf,
            CliMode::Mtsdf => Self::Mtsdf,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let format = output_format_from_path(&args.output)?;
    let svg = std::fs::read(&args.input)?;
    let options = MsdfOptions::new(args.size.0, args.size.1, args.distance_range)?
        .with_mode(args.mode.into());
    let output = generate_from_svg(&svg, options)?;

    match format {
        OutputFormat::Png => {
            write_png(
                &args.output,
                output.width,
                output.height,
                output.channels,
                &output.pixels,
            )?;

            let metadata_path = args
                .metadata
                .unwrap_or_else(|| default_metadata_path(&args.output));
            let metadata = serde_json::to_vec_pretty(&output.metadata)?;
            std::fs::write(metadata_path, metadata)?;
        }
        OutputFormat::Json => {
            let export = MsdfJsonExport::from_output(&output);
            let json = serde_json::to_vec(&export)?;
            std::fs::write(&args.output, json)?;
        }
    }

    Ok(())
}

fn write_png(path: &Path, width: u32, height: u32, channels: usize, pixels: &[u8]) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(match channels {
        1 => png::ColorType::Grayscale,
        3 => png::ColorType::Rgb,
        4 => png::ColorType::Rgba,
        _ => {
            return Err(Error::InvalidOptions(format!(
                "unsupported channel count `{channels}`"
            )));
        }
    });
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels)?;
    Ok(())
}

fn default_metadata_path(output: &Path) -> PathBuf {
    output.with_extension("json")
}

fn output_format_from_path(path: &Path) -> Result<OutputFormat> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);

    match extension.as_deref() {
        Some("png") => Ok(OutputFormat::Png),
        Some("json") => Ok(OutputFormat::Json),
        Some(extension) => Err(Error::InvalidOptions(format!(
            "unsupported output extension `.{extension}`; use .png or .json"
        ))),
        None => Err(Error::InvalidOptions(
            "output path must end with .png or .json".to_string(),
        )),
    }
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

    #[test]
    fn infers_output_format_from_extension() {
        assert_eq!(
            output_format_from_path(Path::new("icon.png")).unwrap(),
            OutputFormat::Png
        );
        assert_eq!(
            output_format_from_path(Path::new("icon.JSON")).unwrap(),
            OutputFormat::Json
        );
        assert!(matches!(
            output_format_from_path(Path::new("icon.txt")),
            Err(Error::InvalidOptions(_))
        ));
        assert!(matches!(
            output_format_from_path(Path::new("icon")),
            Err(Error::InvalidOptions(_))
        ));
    }
}
