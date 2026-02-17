# Zarr TUI

A terminal-based interactive viewer for [Zarr](https://zarr.dev/) geospatial datasets, built with Rust. Visualise large multidimensional scientific data directly in your terminal with pan/zoom navigation, multiple colormaps, and lazy chunk loading.

## Features

- **Interactive map viewer** — pan, zoom, and inspect geospatial heatmaps rendered as coloured terminal cells
- **Lazy chunk loading** — only visible chunks are loaded and cached via an LRU eviction strategy, keeping memory usage bounded even for very large datasets
- **Local and S3 storage** — read Zarr stores from the local filesystem or directly from AWS S3
- **Multiple colourmaps** — cycle through colourmaps (Viridis, Plasma, Inferno, Magma, Coolwarm, Turbo, GistNcar)
- **Multi-variable support** — switch between data variables on the fly
- **Automatic coordinate detection** — discovers latitude/longitude dimensions from common naming conventions
- **Block averaging** — downsampling when zoomed out for efficient rendering
- **Colourbar legend** — always-visible gradient bar with labelled value range
- **Status bar** — real-time cursor position, data value, zoom level, and chunk loading statistics

## Installation

Requires [Rust](https://www.rust-lang.org/tools/install) (edition 2024).

```bash
git clone https://github.com/albertdow/zarr-tui.git
cd zarr-tui
cargo build --release
```

The binary will be at `target/release/zarr-tui`.

## Usage

```bash
# Local Zarr store
cargo run -- /path/to/data.zarr

# AWS S3 (uses default credentials or anonymous access)
cargo run -- s3://bucket-name/path/to/data.zarr
```

## Keybindings

| Key | Action |
| --- | --- |
| `Arrow keys` / `hjkl` | Pan view |
| `+` / `=` | Zoom in |
| `-` | Zoom out |
| `r` | Reset view |
| `[` / `]` | Previous / next variable |
| `c` / `C` | Next / previous colormap |
| `?` | Toggle help overlay |
| `q` / `Esc` | Quit |

## Supported data

- **Zarr versions**: V2 and V3 (via [zarrs](https://github.com/zarrs/zarrs))
- **Data types**: float32, float64, int32, int64, uint8, uint16
- **Coordinate names**: `lat`/`latitude`/`y`, `lon`/`longitude`/`x` are auto-detected

## Dependencies

Built on:

- [ratatui](https://ratatui.rs/) — TUI rendering framework
- [crossterm](https://github.com/crossterm-rs/crossterm) — cross-platform terminal control
- [zarrs](https://github.com/zarrs/zarrs) — pure Rust Zarr implementation
- [ndarray](https://github.com/rust-ndarray/ndarray) — N-dimensional arrays
- [tokio](https://tokio.rs/) — async runtime
- [object_store](https://github.com/apache/arrow-rs/tree/main/object_store) — cloud storage access (S3)

