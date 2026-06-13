# rs-msdf

A small Rust-based multi-channel signed distance field generator for SVG files.

`rs-msdf` provides both a reusable Rust library and a CLI that converts one SVG
into an 8-bit RGB MSDF PNG plus JSON placement metadata.

## CLI

```sh
rs-msdf input.svg --size 64 --output icon.msdf.png
rs-msdf input.svg --size 128x96 --range 4 --output icon.msdf.png
```

`--size` is required and accepts either `N` for square textures or `WxH` for
rectangular textures. `--range` is measured in output pixels and defaults to
`4.0`.

By default, metadata is written next to the PNG with a `.json` extension. Use
`--metadata path/to/file.json` to choose a specific metadata path.

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
