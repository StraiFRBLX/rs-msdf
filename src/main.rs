use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use rayon::prelude::*;
use rs_msdf::{
    DistanceFieldMode, Error, JsonCompression, JsonExportOptions, MsdfJsonExport, MsdfOptions,
    Result, expand_svg_inputs, generate_from_svg_file, write_json_export_file,
    write_metadata_json_file, write_png_file,
};

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// Input SVG file or glob pattern.
    input: PathBuf,

    /// Output texture size, either N for square or WxH.
    #[arg(short, long, value_parser = parse_size)]
    size: (u32, u32),

    /// Distance field type to generate.
    #[arg(short, long, value_enum, default_value_t = CliMode::Msdf)]
    mode: CliMode,

    /// Signed distance range in output pixels.
    #[arg(
        short = 'r',
        long = "range",
        alias = "distance-range",
        default_value_t = 4.0
    )]
    distance_range: f64,

    /// Output file. Use .png for a PNG plus metadata sidecar, or .json for a self-contained data export.
    #[arg(long, short)]
    output: Option<PathBuf>,

    /// Output directory for glob/bulk input.
    #[arg(short = 'd', long)]
    out_dir: Option<PathBuf>,

    /// Output format for glob/bulk input.
    #[arg(short, long, value_enum)]
    format: Option<OutputFormat>,

    /// Output JSON metadata file for PNG output. Defaults to the PNG path with a .json extension.
    #[arg(short = 'M', long)]
    metadata: Option<PathBuf>,

    /// Compress JSON pixel data with zstd and PNG-style row filters.
    #[arg(short, long)]
    compress: bool,

    /// Zstd compression level for JSON exports.
    #[arg(short = 'l', long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=22))]
    compression_level: u32,

    /// Number of worker threads for generation. Defaults to Rayon automatic sizing.
    #[arg(short, long)]
    jobs: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Png,
    Json,
}

impl OutputFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Json => "json",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliMode {
    Sdf,
    Psdf,
    Msdf,
    Mtsdf,
}

impl CliMode {
    fn suffix(self) -> &'static str {
        match self {
            Self::Sdf => "sdf",
            Self::Psdf => "psdf",
            Self::Msdf => "msdf",
            Self::Mtsdf => "mtsdf",
        }
    }
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

struct OutputJob {
    input: PathBuf,
    output: PathBuf,
    metadata: Option<PathBuf>,
    format: OutputFormat,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    if let Some(jobs) = args.jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build_global()?;
    }

    let inputs = expand_svg_inputs(&args.input)?;
    let output_jobs = resolve_output_jobs(&args, &inputs)?;
    let options = MsdfOptions::new(args.size.0, args.size.1, args.distance_range)?
        .with_mode(args.mode.into());
    let json_options = json_export_options(&args);

    if output_jobs.len() == 1 {
        process_job(&output_jobs[0], options, json_options)?;
        return Ok(());
    }

    let failures: Vec<_> = output_jobs
        .par_iter()
        .filter_map(|job| {
            process_job(job, options, json_options)
                .err()
                .map(|error| format!("{}: {error}", job.input.display()))
        })
        .collect();

    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("failed: {failure}");
        }
        return Err(Error::InvalidOptions(format!(
            "{} input(s) failed during bulk export",
            failures.len()
        )));
    }

    Ok(())
}

fn process_job(
    job: &OutputJob,
    options: MsdfOptions,
    json_options: JsonExportOptions,
) -> Result<()> {
    let output = generate_from_svg_file(&job.input, options)?;

    match job.format {
        OutputFormat::Png => {
            write_png_file(&job.output, &output)?;

            let metadata_path = job
                .metadata
                .clone()
                .unwrap_or_else(|| default_metadata_path(&job.output));
            write_metadata_json_file(metadata_path, &output.metadata, true)?;
        }
        OutputFormat::Json => {
            let export = MsdfJsonExport::from_output_with_options(&output, json_options)?;
            write_json_export_file(&job.output, &export)?;
        }
    }

    Ok(())
}

fn json_export_options(args: &Args) -> JsonExportOptions {
    if args.compress {
        JsonExportOptions {
            compression: JsonCompression::Zstd {
                level: args.compression_level,
            },
        }
    } else {
        JsonExportOptions {
            compression: JsonCompression::Raw,
        }
    }
}

fn resolve_output_jobs(args: &Args, inputs: &[PathBuf]) -> Result<Vec<OutputJob>> {
    if inputs.len() == 1 {
        let output = args.output.clone().ok_or_else(|| {
            Error::InvalidOptions("single-file input requires --output".to_string())
        })?;
        let format = output_format_from_path(&output)?;
        return Ok(vec![OutputJob {
            input: inputs[0].clone(),
            output,
            metadata: args.metadata.clone(),
            format,
        }]);
    }

    if args.output.is_some() {
        return Err(Error::InvalidOptions(
            "--output cannot be used when the input expands to multiple SVG files; use --out-dir and --format"
                .to_string(),
        ));
    }
    if args.metadata.is_some() {
        return Err(Error::InvalidOptions(
            "--metadata cannot be used with multiple SVG inputs".to_string(),
        ));
    }

    let out_dir = args
        .out_dir
        .as_ref()
        .ok_or_else(|| Error::InvalidOptions("bulk input requires --out-dir".to_string()))?;
    let format = args.format.ok_or_else(|| {
        Error::InvalidOptions("bulk input requires --format png or --format json".to_string())
    })?;
    std::fs::create_dir_all(out_dir)?;

    inputs
        .iter()
        .map(|input| {
            let stem = input
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| {
                    Error::InvalidOptions(format!(
                        "input path `{}` has no valid file stem",
                        input.display()
                    ))
                })?;
            let file_name = format!("{}.{}.{}", stem, args.mode.suffix(), format.extension());
            let output = out_dir.join(file_name);
            let metadata = (format == OutputFormat::Png).then(|| default_metadata_path(&output));
            Ok(OutputJob {
                input: input.clone(),
                output,
                metadata,
                format,
            })
        })
        .collect()
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
