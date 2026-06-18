# rs-msdf

`rs-msdf` is a Rust library and command line tool for generating signed
distance field textures from SVG vector geometry.

It generates these texture types:

- `sdf`: single-channel true signed distance field.
- `psdf`: single-channel perpendicular signed distance field.
- `msdf`: RGB multi-channel signed distance field. This is the default.
- `mtsdf`: RGB MSDF with true SDF in the alpha channel.

The tool accepts SVG input that can be represented as vector contours. It writes
PNG textures, JSON metadata, or a self-contained JSON export that embeds the PNG
as a base64 string.

## Install

Install the CLI from this checkout:

```sh
cargo install --path .
```

Use the library from another local Rust project:

```toml
[dependencies]
rs-msdf = { path = "../rs-msdf" }
```

Use the Git repository directly:

```toml
[dependencies]
rs-msdf = { git = "https://github.com/StraiFRBLX/rs-msdf" }
```

Rust code imports the crate as `rs_msdf`:

```rust
use rs_msdf::{generate_from_svg, MsdfOptions};
```

## CLI

Generate a PNG and metadata sidecar:

```sh
rs-msdf input.svg --size 64 --output icon.msdf.png
```

Generate a rectangular texture:

```sh
rs-msdf input.svg --size 128x96 --range 4 --output icon.msdf.png
```

Generate a JSON file that contains metadata and a base64 PNG string:

```sh
rs-msdf input.svg --size 64 --output icon.msdf.json
```

Generate MTSDF output:

```sh
rs-msdf input.svg --size 64 --mode mtsdf --output icon.mtsdf.png
```

Process multiple SVG files with a glob:

```sh
rs-msdf "icons/*.svg" --size 64 --out-dir dist --format png
rs-msdf "icons/*.svg" --size 64 --out-dir dist --format json
```

### Options

| Option | Required | Description |
| --- | --- | --- |
| `input` | Yes | SVG file path or glob pattern. |
| `--size`, `-s` | Yes | Output texture size. Use `N` or `WxH`. |
| `--mode`, `-m` | No | `sdf`, `psdf`, `msdf`, or `mtsdf`. Defaults to `msdf`. |
| `--range`, `-r` | No | Signed distance range in output pixels. Defaults to `4.0`. |
| `--output`, `-o` | For single input | Output file. Must end in `.png` or `.json`. |
| `--metadata`, `-M` | No | Metadata path for PNG output. Defaults to the PNG path with `.json`. |
| `--out-dir`, `-d` | For glob input | Directory for bulk output. |
| `--format`, `-f` | For glob input | Bulk output format, `png` or `json`. |
| `--jobs`, `-j` | No | Number of worker threads. Defaults to Rayon automatic sizing. |

`--distance-range` is also accepted as an alias for `--range`.

When `input` expands to more than one SVG, use `--out-dir` and `--format`
instead of `--output`. Bulk files are named with the input stem and mode, for
example `search.msdf.png` or `search.mtsdf.json`.

## Output Files

### PNG Output

A `.png` output writes the generated distance field as an 8-bit PNG.

PNG output also writes a metadata JSON file. By default, the metadata path is the
PNG path with a `.json` extension. Use `--metadata` to choose another path.

Metadata fields:

| Field | Description |
| --- | --- |
| `format` | Texture format string, such as `msdf-rgb8` or `mtsdf-rgba8`. |
| `channels` | Channel names: `r`, `rgb`, or `rgba`. |
| `bytes_per_pixel` | Number of bytes per output pixel. |
| `width`, `height` | Output texture size in pixels. |
| `range_px` | Signed distance range in output pixels. |
| `svg_bounds` | SVG canvas bounds used for placement. |
| `geometry_bounds` | Bounds of the vector geometry that was rasterized. |
| `scale` | Scale from SVG units to texture pixels. |
| `translation` | Translation from SVG units to texture pixels. |

### JSON Output

A `.json` output writes a self-contained object. It contains the same placement
metadata plus `png_base64`, which is the generated PNG file encoded with normal
base64.

Example:

```json
{
  "kind": "rs-msdf",
  "version": 3,
  "format": "msdf-rgb8",
  "encoding": "base64-png",
  "channels": "rgb",
  "bytes_per_pixel": 3,
  "width": 64,
  "height": 64,
  "range_px": 4.0,
  "svg_bounds": {
    "min_x": 0.0,
    "min_y": 0.0,
    "max_x": 24.0,
    "max_y": 24.0
  },
  "geometry_bounds": {
    "min_x": 1.0,
    "min_y": 1.0,
    "max_x": 23.0,
    "max_y": 23.0
  },
  "scale": 2.727272727272727,
  "translation": [0.18181818181818182, 0.18181818181818182],
  "png_base64": "iVBORw0KGgo..."
}
```

`png_base64` decodes to a complete PNG file. It is not raw pixel data.

```rust
use base64::Engine;

let png_bytes = base64::engine::general_purpose::STANDARD.decode(png_base64)?;
std::fs::write("icon.msdf.png", png_bytes)?;
```

## Rust API

Generate from SVG bytes:

```rust
use rs_msdf::{generate_from_svg, MsdfOptions};

fn main() -> rs_msdf::Result<()> {
    let svg = std::fs::read("input.svg")?;
    let output = generate_from_svg(&svg, MsdfOptions::new(64, 64, 4.0)?)?;

    println!(
        "{}x{} {}, {} bytes",
        output.width,
        output.height,
        output.metadata.format,
        output.pixels.len()
    );

    Ok(())
}
```

Write PNG, metadata, and self-contained JSON output:

```rust
use rs_msdf::{
    generate_from_svg_file,
    write_json_export_file,
    write_metadata_json_file,
    write_png_file,
    MsdfJsonExport,
    MsdfOptions,
};

fn main() -> rs_msdf::Result<()> {
    let output = generate_from_svg_file("input.svg", MsdfOptions::new(64, 64, 4.0)?)?;

    write_png_file("icon.msdf.png", &output)?;
    write_metadata_json_file("icon.msdf.json", &output.metadata, true)?;

    let export = MsdfJsonExport::from_output(&output)?;
    write_json_export_file("icon.export.json", &export)?;

    Ok(())
}
```

Select another distance field mode:

```rust
use rs_msdf::{DistanceFieldMode, MsdfOptions};

let options = MsdfOptions::new(64, 64, 4.0)?.with_mode(DistanceFieldMode::Mtsdf);
```

Process multiple files:

```rust
use rs_msdf::{expand_svg_inputs, generate_from_svg_files, MsdfOptions};

let inputs = expand_svg_inputs("icons/*.svg")?;
let outputs = generate_from_svg_files(&inputs, MsdfOptions::new(64, 64, 4.0)?);

for (path, output) in outputs {
    let output = output?;
    println!("generated {} as {}", path.display(), output.metadata.format);
}
```

## SVG Support

`rs-msdf` uses `usvg` from the linebender/resvg project to parse and normalize
SVG input before generating contours.

Supported:

- Paths and basic SVG shapes that normalize to paths.
- Line, quadratic, and cubic path segments.
- SVG transforms.
- Nonzero and even-odd fill rules.
- Visible strokes that can be expanded into outline contours.
- Text when `usvg` can resolve it into vector outlines using available fonts.

Rejected:

- Clip paths.
- Masks.
- Filters.
- Patterns.
- Embedded raster images.
- Text that remains unresolved after SVG parsing.

Rejected input returns an `UnsupportedSvg` error with a direct message. The tool
does not silently rasterize unsupported SVG features because raster effects do
not produce precise MSDF contours.

## Placement

The generated geometry is fit into the requested output size while preserving
the vector geometry aspect ratio.

Use the metadata to map between SVG coordinates and texture coordinates:

```text
texture_x = svg_x * scale + translation[0]
texture_y = svg_y * scale + translation[1]
```

`geometry_bounds` describes the actual vector geometry. `svg_bounds` describes
the SVG canvas size reported by `usvg`.

## Notes

- Output pixels are 8-bit per channel.
- `msdf` and `mtsdf` use RGB edge coloring for sharp corners.
- `mtsdf` stores true SDF in alpha.
- JSON output stores a PNG file in `png_base64`.
- For best results, export icons as filled vector paths before generating a
  distance field.
