# rs-msdf

A Rust-based multi-channel signed distance field generator for SVG files.

`rs-msdf` provides both a reusable Rust library and a CLI that converts one SVG
into an 8-bit RGB MSDF PNG plus JSON placement metadata.

## Setup

To use `rs-msdf` from another local Rust project, add it as a path dependency in
that project's `Cargo.toml`:

```toml
[dependencies]
rs-msdf = { path = "../rs-msdf" }
```

Adjust the path to wherever this repository lives relative to the consuming
project. If you want to depend on the Git repository instead:

```toml
[dependencies]
rs-msdf = { git = "https://github.com/StraiFRBLX/rs-msdf" }
```

The package name uses a hyphen in `Cargo.toml`, but Rust code imports it with an
underscore:

```rust
use rs_msdf::{generate_from_svg, MsdfOptions};

fn main() -> rs_msdf::Result<()> {
    let svg = std::fs::read("input.svg")?;
    let output = generate_from_svg(&svg, MsdfOptions::new(64, 64, 4.0)?)?;

    println!(
        "{}x{} {}-channel MSDF, {} bytes",
        output.width,
        output.height,
        output.channels,
        output.pixels.len()
    );

    Ok(())
}
```

To install the CLI from a local checkout:

```sh
cargo install --path .
```

## CLI

```sh
rs-msdf input.svg -s 64 -o icon.msdf.png
rs-msdf input.svg -s 128x96 -r 4 -o icon.msdf.png
rs-msdf input.svg -s 64 -o icon.msdf.json
rs-msdf input.svg -s 64 -m mtsdf -o icon.mtsdf.png
rs-msdf input.svg -s 512x384 -m mtsdf --compress -o icon.mtsdf.json
rs-msdf "icons/*.svg" -s 64 -d dist -f json
```

`-s, --size` is required and accepts either `N` for square textures or `WxH` for
rectangular textures. `-r, --range` is measured in output pixels and defaults to
`4.0`. The old `--distance-range` spelling is still accepted as an alias.

`-m, --mode` selects the generated distance field:

- `sdf`: single-channel true signed distance field.
- `psdf`: single-channel pseudo/perpendicular signed distance field.
- `msdf`: RGB multi-channel signed distance field. This is the default.
- `mtsdf`: RGB MSDF plus alpha true SDF.

The output format is inferred from `--output`. A `.png` output writes a PNG plus
a metadata sidecar; by default, metadata is written next to the PNG with a
`.json` extension. Use `-M, --metadata path/to/file.json` to choose a specific
metadata path.

A `.json` output writes a self-contained compact JSON export instead of a PNG.
By default, the interleaved pixel bytes are written as uncompressed base64
(`encoding: "base64"`). Pass `-c, --compress` to PNG-filter the rows, compress
them with zstd, and then base64-encode them (`encoding:
"base64+zstd+png-filter"`). Decode compressed JSON data by base64-decoding
`data`, zstd-decompressing it, reversing the per-row PNG filters, and
interpreting the result as `uncompressed_data_len` tightly packed pixel bytes.
Use `-l, --compression-level` to choose a zstd level from 1 to 22.

Bulk input is supported with glob patterns. When the input expands to multiple
SVG files, use `-d, --out-dir` and `-f, --format png|json` instead of
`-o, --output`.
Generated files are named `<input-stem>.<mode>.png` or
`<input-stem>.<mode>.json`. Use `-j, --jobs N` to choose a Rayon worker count.

## Library

```rust
use rs_msdf::{
    generate_from_svg,
    write_png_file,
    write_metadata_json_file,
    write_json_export_file,
    JsonExportOptions,
    MsdfJsonExport,
    MsdfOptions,
};

let svg = std::fs::read("input.svg")?;
let output = generate_from_svg(&svg, MsdfOptions::new(64, 64, 4.0)?)?;

write_png_file("icon.msdf.png", &output)?;
write_metadata_json_file("icon.msdf.json", &output.metadata, true)?;

let raw_json = MsdfJsonExport::from_output(&output);
write_json_export_file("icon.raw.json", &raw_json)?;

let compressed_json =
    MsdfJsonExport::from_output_with_options(&output, JsonExportOptions::zstd(10))?;
write_json_export_file("icon.compressed.json", &compressed_json)?;
```

For bulk processing from Rust, use the same glob expansion and parallel
generation helpers as the CLI:

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

The current implementation uses `usvg` for parsing and normalization, combines
all visible vector geometry into one silhouette, expands supported strokes into
filled outlines, and preserves SVG fill rules for sign calculation.

Features that cannot be represented as precise vector contours, such as masks,
clip paths, filters, patterns, embedded raster images, and text that remains
unconverted after `usvg` parsing, fail with explicit diagnostics.
