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
rs-msdf input.svg --size 64 --output icon.msdf.png
rs-msdf input.svg --size 128x96 --distance-range 4 --output icon.msdf.png
rs-msdf input.svg --size 64 --output icon.msdf.json
rs-msdf input.svg --size 64 --mode mtsdf --output icon.mtsdf.png
```

`--size` is required and accepts either `N` for square textures or `WxH` for
rectangular textures. `--distance-range` is measured in output pixels and
defaults to `4.0`. The old `--range` spelling is still accepted as an alias.

`--mode` selects the generated distance field:

- `sdf`: single-channel true signed distance field.
- `psdf`: single-channel pseudo/perpendicular signed distance field.
- `msdf`: RGB multi-channel signed distance field. This is the default.
- `mtsdf`: RGB MSDF plus alpha true SDF.

The output format is inferred from `--output`. A `.png` output writes a PNG plus
a metadata sidecar; by default, metadata is written next to the PNG with a
`.json` extension. Use `--metadata path/to/file.json` to choose a specific
metadata path.

A `.json` output writes a self-contained compact JSON export instead of a PNG. It
includes the distance-field metadata and the interleaved pixel bytes as a base64
string, which is suitable for conversion into byte-oriented runtimes such as Luau
buffers.

## Library

```rust
use rs_msdf::{generate_from_svg, MsdfOptions};

let svg = std::fs::read("input.svg")?;
let output = generate_from_svg(&svg, MsdfOptions::new(64, 64, 4.0)?)?;
```

## SVG Support

The current implementation uses `usvg` for parsing and normalization, combines
all visible vector geometry into one silhouette, expands supported strokes into
filled outlines, and preserves SVG fill rules for sign calculation.

Features that cannot be represented as precise vector contours, such as masks,
clip paths, filters, patterns, embedded raster images, and text that remains
unconverted after `usvg` parsing, fail with explicit diagnostics.
