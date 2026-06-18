# rs-msdf

`rs-msdf` generates signed distance field textures from SVG vector geometry.
It provides a Rust library and a CLI.

The default output is RGB MSDF. The tool can also generate SDF, PSDF, and MTSDF
textures.

## Install

Install the CLI from a local checkout:

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

The package name uses a hyphen. Rust code imports it with an underscore:

```rust
use rs_msdf::{generate_from_svg, MsdfOptions};
```

## CLI Usage

Generate a PNG and metadata sidecar:

```sh
rs-msdf input.svg --size 64 --output icon.msdf.png
```

Generate a rectangular texture:

```sh
rs-msdf input.svg --size 128x96 --range 4 --output icon.msdf.png
```

Generate a self-contained JSON file with embedded PNG bytes:

```sh
rs-msdf input.svg --size 64 --output icon.msdf.json
```

Generate MTSDF output:

```sh
rs-msdf input.svg --size 64 --mode mtsdf --output icon.mtsdf.png
```

Process a glob of SVG files:

```sh
rs-msdf "icons/*.svg" --size 64 --out-dir dist --format json
```

### CLI Options

- `--size`, `-s`: Required. Use `N` for a square texture or `WxH` for a
  rectangular texture.
- `--range`, `-r`: Signed distance range in output pixels. Defaults to `4.0`.
  `--distance-range` is accepted as an alias.
- `--mode`, `-m`: Output mode. Use `sdf`, `psdf`, `msdf`, or `mtsdf`.
  The default is `msdf`.
- `--output`, `-o`: Output file for one SVG. Use `.png` or `.json`.
- `--metadata`, `-M`: Metadata path for PNG output. Defaults to the PNG path
  with a `.json` extension.
- `--out-dir`, `-d`: Output directory for glob input.
- `--format`, `-f`: Output format for glob input. Use `png` or `json`.
- `--jobs`, `-j`: Number of Rayon worker threads.

When a glob expands to multiple SVG files, use `--out-dir` and `--format`
instead of `--output`. Generated files are named
`<input-stem>.<mode>.png` or `<input-stem>.<mode>.json`.

## JSON Export Schema

JSON output is a metadata object with a base64-encoded PNG payload:

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
  "svg_bounds": { "min_x": 0.0, "min_y": 0.0, "max_x": 24.0, "max_y": 24.0 },
  "geometry_bounds": { "min_x": 1.0, "min_y": 1.0, "max_x": 23.0, "max_y": 23.0 },
  "scale": 2.72,
  "translation": [0.32, 0.32],
  "png_base64": "iVBORw0KGgo..."
}
```

Decode `png_base64` as regular base64 PNG bytes:

```rust
use base64::Engine;

let png = base64::engine::general_purpose::STANDARD.decode(export.png_base64)?;
std::fs::write("icon.msdf.png", png)?;
```

JSON export does not include raw interleaved pixels and does not support zstd
compression.

## Rust API

```rust
use rs_msdf::{
    generate_from_svg,
    write_json_export_file,
    write_metadata_json_file,
    write_png_file,
    MsdfJsonExport,
    MsdfOptions,
};

fn main() -> rs_msdf::Result<()> {
    let svg = std::fs::read("input.svg")?;
    let output = generate_from_svg(&svg, MsdfOptions::new(64, 64, 4.0)?)?;

    write_png_file("icon.msdf.png", &output)?;
    write_metadata_json_file("icon.msdf.json", &output.metadata, true)?;

    let export = MsdfJsonExport::from_output(&output)?;
    write_json_export_file("icon.export.json", &export)?;

    Ok(())
}
```

Bulk processing uses the same glob expansion and parallel generation helpers as
the CLI:

```rust
use rs_msdf::{expand_svg_inputs, generate_from_svg_files, MsdfOptions};

let inputs = expand_svg_inputs("icons/*.svg")?;
let outputs = generate_from_svg_files(&inputs, MsdfOptions::new(64, 64, 4.0)?);

for (path, output) in outputs {
    let output = output?;
    println!("generated {} ({} bytes)", path.display(), output.pixels.len());
}
```

## SVG Support

`rs-msdf` uses the maintained `usvg` crate from the linebender/resvg project for
SVG parsing and normalization.

Supported input must resolve to precise vector contours:

- Paths and basic shapes.
- SVG transforms.
- Fill rules, including nonzero and even-odd.
- Visible strokes expanded into outline contours.
- Text when `usvg` can resolve it into vector outlines from available fonts.

Unsupported input fails with an explicit error instead of producing an
approximate distance field:

- Clip paths.
- Masks.
- Filters.
- Patterns.
- Embedded raster images.
- Text that remains unresolved after `usvg` parsing.

## Limitations

MSDF generation is contour-based. It does not reproduce full SVG visual output.
Effects such as filters, masks, raster images, and pattern paints do not have a
precise contour representation in this tool.

For best results, export icons as filled vector paths before generating a
distance field.
