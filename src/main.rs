mod camera;
mod colormap;
mod ui;
mod zarr;
use ui::{Colorbar, HelpOverlay, StatusBar, StatusBarData};

use std::collections::HashMap;
use std::io;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, layout::Rect, prelude::CrosstermBackend};

use camera::Camera;
use colormap::{ColorMap, ColormapType};
use zarr::chunk_manager::{ChunkManager, visible_chunks};
use zarr::storage::{OpenArray, UnifiedStore};

const DEFAULT_CHUNK_SIZE: usize = 256;
const CACHE_CAPACITY: usize = 512;

#[tokio::main]
async fn main() -> Result<()> {
    let zarr_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "data_fire.zarr".to_string());

    eprintln!("Loading zarr from: {}", zarr_path);
    let mut zarr_data = load_zarr_data(&zarr_path).await?;
    eprintln!(
        "Found {} variables: {:?}",
        zarr_data.variables.len(),
        zarr_data.variables
    );

    // open initial variable
    let mut current_var_idx = 0;
    let current_var_path = &zarr_data.variables[current_var_idx];
    eprintln!("Opening variable: {}", current_var_path);

    // open array and get metadata
    let open_array = zarr_data.store.open_array(current_var_path)?;
    let meta = open_array
        .meta()
        .ok_or_else(|| anyhow::anyhow!("Missing dimension names for {}", current_var_path))?;
    let ndim = open_array.shape().len();

    // Use native chunk shape if available, otherwise default
    if let Some((native_lat, native_lon)) =
        open_array.native_chunk_shape(meta.lat_axis, meta.lon_axis)
    {
        eprintln!("  Using native chunk size: {}x{}", native_lat, native_lon);
        zarr_data.chunk_manager.chunk_size_lat = native_lat;
        zarr_data.chunk_manager.chunk_size_lon = native_lon;
    }

    zarr_data
        .open_arrays
        .insert(current_var_path.clone(), open_array);

    let mut current_meta = meta;
    let mut current_ndim = ndim;

    let chunk_lat = zarr_data.chunk_manager.chunk_size_lat;
    let chunk_lon = zarr_data.chunk_manager.chunk_size_lon;
    let estimated_chunk_mb = (chunk_lat * chunk_lon * 4) as f64 / (1024.0 * 1024.0);
    eprintln!(
        "Ready for lazy loading (chunk size: {}x{}, ~{:.1}MB per chunk)",
        chunk_lat, chunk_lon, estimated_chunk_mb
    );

    // terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // app state
    let mut should_quit = false;
    // centre camera on data bounds
    let lat_min = zarr_data
        .lat_data
        .first()
        .unwrap()
        .min(*zarr_data.lat_data.last().unwrap());
    let lat_max = zarr_data
        .lat_data
        .first()
        .unwrap()
        .max(*zarr_data.lat_data.last().unwrap());
    let lon_min = zarr_data
        .lon_data
        .first()
        .unwrap()
        .min(*zarr_data.lon_data.last().unwrap());
    let lon_max = zarr_data
        .lon_data
        .first()
        .unwrap()
        .max(*zarr_data.lon_data.last().unwrap());
    let center_lat = (lat_min + lat_max) / 2.0;
    let center_lon = (lon_min + lon_max) / 2.0;
    // set initial zoom to fit data in ~80 columns
    let lat_extent = lat_max - lat_min;
    let lon_extent = lon_max - lon_min;
    let initial_zoom = lon_extent.max(lat_extent) / 80.0;
    let mut camera = Camera::new(center_lat, center_lon, initial_zoom);
    let mut mouse_lat_lon: Option<(f32, f32)> = None;
    let mut clicked_value: Option<f32> = None;
    let mut show_help = false;
    let mut visible_chunk_count: usize;
    let mut current_colormap = ColormapType::Viridis;

    // hot loop
    while !should_quit {
        let current_var_path = &zarr_data.variables[current_var_idx];
        let current_var_name = current_var_path.trim_start_matches('/').to_string();

        // calculate viewport bounds (map area excludes colorbar and status bar)
        let area = terminal.get_frame().area();
        let colorbar_width: u16 = 12;
        let map_width = area.width.saturating_sub(colorbar_width);
        let map_height = area.height;
        let (top_lat, left_lon) = camera.screen_to_geo(0, 0, map_width, map_height);
        let (bottom_lat, right_lon) = camera.screen_to_geo(
            map_width,
            map_height.saturating_sub(1),
            map_width,
            map_height,
        );
        let view_lat_min = top_lat.min(bottom_lat);
        let view_lat_max = top_lat.max(bottom_lat);
        let view_lon_min = left_lon.min(right_lon);
        let view_lon_max = left_lon.max(right_lon);

        // get visible chunks
        let chunks = visible_chunks(
            view_lat_min,
            view_lat_max,
            view_lon_min,
            view_lon_max,
            &zarr_data.lat_data,
            &zarr_data.lon_data,
            zarr_data.chunk_manager.chunk_size_lat,
            zarr_data.chunk_manager.chunk_size_lon,
        );
        visible_chunk_count = chunks.len();

        if let Some(array) = zarr_data.open_arrays.get(current_var_path) {
            zarr_data.chunk_manager.load_visible_chunks(
                current_var_path,
                0,
                &chunks,
                array,
                current_meta.lat_axis,
                current_meta.lon_axis,
                current_ndim,
            );
        }

        terminal.draw(|frame| {
            let area = frame.area();

            let status_area = Rect {
                x: area.x,
                y: area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            };

            let colorbar_width = 12;
            let colorbar_area = Rect {
                x: area.width.saturating_sub(colorbar_width),
                y: area.y,
                width: colorbar_width,
                height: area.height.saturating_sub(1),
            };
            let map_width = area.width.saturating_sub(colorbar_width);

            let vmin = 0.005;
            let vmax = 0.3;

            let points_per_pixel =
                camera.data_points_per_pixel(&zarr_data.lat_data, &zarr_data.lon_data);
            let use_averaging = points_per_pixel > 1.5;

            for y in area.top()..area.bottom().saturating_sub(1) {
                for x in area.left()..map_width {
                    let value = if use_averaging {
                        // Block averaging for zoomed-out view
                        if let Some(((lat_start, lat_end), (lon_start, lon_end))) = camera
                            .pixel_to_index_range(
                                x,
                                y,
                                map_width,
                                area.height,
                                &zarr_data.lat_data,
                                &zarr_data.lon_data,
                            )
                        {
                            zarr_data.chunk_manager.get_averaged_value_if_cached(
                                current_var_path,
                                0,
                                (lat_start, lat_end),
                                (lon_start, lon_end),
                                current_meta.lat_axis,
                                current_meta.lon_axis,
                                current_ndim,
                            )
                        } else {
                            None
                        }
                    } else {
                        // Nearest neighbor for zoomed-in view
                        let (lat, lon) = camera.screen_to_geo(x, y, map_width, area.height);
                        if let Some((lat_idx, lon_idx)) = camera.geo_to_indices(
                            lat,
                            lon,
                            &zarr_data.lat_data,
                            &zarr_data.lon_data,
                        ) {
                            zarr_data.chunk_manager.get_value_if_cached(
                                current_var_path,
                                0,
                                lat_idx,
                                lon_idx,
                                current_meta.lat_axis,
                                current_meta.lon_axis,
                                current_ndim,
                            )
                        } else {
                            None
                        }
                    };

                    if let Some(v) = value {
                        let color = ColorMap::map_value(v, vmin, vmax, current_colormap);
                        let cell = &mut frame.buffer_mut()[(x, y)];
                        cell.set_char(' ');
                        cell.set_bg(color);
                    }
                }
            }

            // Render colorbar
            frame.render_widget(Colorbar::new(vmin, vmax, current_colormap), colorbar_area);

            // render status bar
            let status_data = StatusBarData {
                cursor_lat: mouse_lat_lon.map(|(lat, _)| lat),
                cursor_lon: mouse_lat_lon.map(|(_, lon)| lon),
                cursor_value: clicked_value,
                camera_zoom: camera.zoom,
                variable_name: Some(current_var_name.clone()),
                cached_chunks: Some(zarr_data.chunk_manager.cache_len()),
                visible_chunks: Some(visible_chunk_count),
                pending_chunks: Some(zarr_data.chunk_manager.pending_chunks),
            };
            frame.render_widget(StatusBar::new(&status_data), status_area);

            if show_help {
                frame.render_widget(HelpOverlay::default(), area);
            }
        })?;

        let poll_timeout = std::time::Duration::from_millis(100);
        while event::poll(poll_timeout)? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => should_quit = true,
                    KeyCode::Char('+') | KeyCode::Char('=') => camera.zoom_in(),
                    KeyCode::Char('-') => camera.zoom_out(),
                    KeyCode::Char('r') => camera.set(center_lat, center_lon, initial_zoom),
                    KeyCode::Left => camera.pan(0.0, -5.0 * camera.zoom),
                    KeyCode::Right => camera.pan(0.0, 5.0 * camera.zoom),
                    KeyCode::Up => camera.pan(5.0 * camera.zoom, 0.0),
                    KeyCode::Down => camera.pan(-5.0 * camera.zoom, 0.0),
                    KeyCode::Char('h') => camera.pan(0.0, -5.0 * camera.zoom),
                    KeyCode::Char('l') => camera.pan(0.0, 5.0 * camera.zoom),
                    KeyCode::Char('k') => camera.pan(5.0 * camera.zoom, 0.0),
                    KeyCode::Char('j') => camera.pan(-5.0 * camera.zoom, 0.0),
                    // help
                    KeyCode::Char('?') => show_help = !show_help,
                    // colormaps - cycle with c/C
                    KeyCode::Char('c') => current_colormap = current_colormap.next(),
                    KeyCode::Char('C') => current_colormap = current_colormap.prev(),
                    // variables - switch (chunks loaded on demand, cache keyed by var name)
                    KeyCode::Char('[') => {
                        if !zarr_data.variables.is_empty() {
                            let len = zarr_data.variables.len();
                            current_var_idx = (current_var_idx + len - 1) % len;
                            let var_path = &zarr_data.variables[current_var_idx];

                            // open array if not already cached
                            if let Ok(arr) = zarr_data.store.open_array(var_path)
                                && !zarr_data.open_arrays.contains_key(var_path)
                            {
                                zarr_data.open_arrays.insert(var_path.clone(), arr);
                            }

                            // update metadata
                            if let Some(arr) = zarr_data.open_arrays.get(var_path)
                                && let Some(meta) = arr.meta()
                            {
                                current_meta = meta;
                                current_ndim = arr.shape().len();
                            }
                        }
                    }
                    KeyCode::Char(']') => {
                        if !zarr_data.variables.is_empty() {
                            current_var_idx = (current_var_idx + 1) % zarr_data.variables.len();
                            let var_path = &zarr_data.variables[current_var_idx];

                            // open array if not already cached
                            if !zarr_data.open_arrays.contains_key(var_path)
                                && let Ok(arr) = zarr_data.store.open_array(var_path)
                            {
                                zarr_data.open_arrays.insert(var_path.clone(), arr);
                            }

                            // update metadata
                            if let Some(arr) = zarr_data.open_arrays.get(var_path)
                                && let Some(meta) = arr.meta()
                            {
                                current_meta = meta;
                                current_ndim = arr.shape().len();
                            }
                        }
                    }
                    _ => {}
                },
                Event::Mouse(mouse) => {
                    let (lat, lon) =
                        camera.screen_to_geo(mouse.column, mouse.row, map_width, map_height);
                    mouse_lat_lon = Some((lat, lon));

                    // use chunk manager for value lookup
                    if let Some((lat_idx, lon_idx)) =
                        camera.geo_to_indices(lat, lon, &zarr_data.lat_data, &zarr_data.lon_data)
                    {
                        clicked_value = zarr_data.chunk_manager.get_value_if_cached(
                            current_var_path,
                            0,
                            lat_idx,
                            lon_idx,
                            current_meta.lat_axis,
                            current_meta.lon_axis,
                            current_ndim,
                        );
                    } else {
                        clicked_value = None;
                    }
                }
                _ => {}
            }
        }
    }

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Names that indicate coordinate arrays (not data variables)
const COORD_NAMES: &[&str] = &[
    "lat",
    "latitude",
    "lon",
    "longitude",
    "x",
    "y",
    "time",
    "level",
    "crs",
    "spatial_ref",
    "year",
    "month",
];

/// Filter arrays to find coordinate arrays
fn find_coord_array(arrays: &[String], names: &[&str]) -> Option<String> {
    for name in names {
        let path = format!("/{}", name);
        if arrays.contains(&path) {
            return Some(path);
        }
    }
    None
}

/// Filter arrays to find data variables (non-coordinate arrays)
fn find_data_variables(arrays: &[String]) -> Vec<String> {
    arrays
        .iter()
        .filter(|name| {
            let base = name.trim_start_matches('/').to_lowercase();
            !COORD_NAMES.contains(&base.as_str())
        })
        .cloned()
        .collect()
}

/// Loaded zarr dataset with lazy chunk loading
struct ZarrData {
    store: UnifiedStore,
    lat_data: Vec<f32>,
    lon_data: Vec<f32>,
    variables: Vec<String>,
    open_arrays: HashMap<String, OpenArray>,
    chunk_manager: ChunkManager,
}

async fn load_zarr_data(path: &str) -> Result<ZarrData> {
    eprintln!("  Opening store...");
    let store = UnifiedStore::open(path)?;

    // discover arrays
    eprintln!("  Discovering arrays...");
    let arrays = store.discover_arrays().await?;
    eprintln!("  Found {} arrays", arrays.len());

    // find coordinate arrays
    let lat_path = find_coord_array(&arrays, &["latitude", "lat", "y"])
        .ok_or_else(|| anyhow::anyhow!("No latitude coordinate found"))?;
    let lon_path = find_coord_array(&arrays, &["longitude", "lon", "x"])
        .ok_or_else(|| anyhow::anyhow!("No longitude coordinate found"))?;

    eprintln!("  Loading coordinates...");
    let lat_data = store.load_coord_array(&lat_path)?;
    let lon_data = store.load_coord_array(&lon_path)?;
    eprintln!("  Coordinates: {}x{}", lat_data.len(), lon_data.len());

    // find data variables
    let variables = find_data_variables(&arrays);
    if variables.is_empty() {
        anyhow::bail!("No data variables found in zarr store");
    }

    // create chunk manager (will be updated with native chunk size when array is opened)
    let chunk_manager = ChunkManager::new(
        DEFAULT_CHUNK_SIZE,
        DEFAULT_CHUNK_SIZE,
        lat_data.len(),
        lon_data.len(),
        CACHE_CAPACITY,
    );

    Ok(ZarrData {
        store,
        lat_data,
        lon_data,
        variables,
        open_arrays: HashMap::new(),
        chunk_manager,
    })
}
