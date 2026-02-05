mod camera;
mod colormap;
mod ui;
mod zarr;
use ui::{HelpOverlay, StatusBar, StatusBarData};

use ndarray::{ArrayD, IxDyn};
use std::{io, sync::Arc};
use zarrs::{array::Array, array_subset::ArraySubset};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, layout::Rect, prelude::CrosstermBackend};

use camera::Camera;
use colormap::ColorMap;
use zarrs_filesystem::FilesystemStore;

struct ArrayMeta {
    lon_axis: usize,
    lat_axis: usize,
}

impl ArrayMeta {
    fn from_array<TStorage: zarrs::storage::ReadableStorageTraits + ?Sized>(
        array: &Array<TStorage>,
    ) -> Option<Self> {
        // zarr v3: native dimension_names in array metadata
        let dim_names: Vec<String> = if let Some(names) = array.dimension_names() {
            names.iter().filter_map(|n| n.clone()).collect()
        } else {
            // zarr v2: xarray stores dimension names in _ARRAY_DIMENSIONS attr
            array
                .attributes()
                .get("_ARRAY_DIMENSIONS")
                .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())?
        };

        let lon_axis = dim_names
            .iter()
            .position(|s| matches!(s.as_str(), "lon" | "longitude" | "x"))?;
        let lat_axis = dim_names
            .iter()
            .position(|s| matches!(s.as_str(), "lat" | "latitude" | "y"))?;

        Some(Self { lon_axis, lat_axis })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let zarr_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "data_fire.zarr".to_string());
    let zarr_data = load_zarr_data(&zarr_path)?;

    // load initial variable with metadata
    let mut current_var_idx = 0;
    let (mut data, mut meta) =
        load_variable_with_meta(&zarr_data.store, &zarr_data.variables[current_var_idx])?;

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

    // main loop
    while !should_quit {
        let current_var_name = zarr_data.variables[current_var_idx]
            .trim_start_matches('/')
            .to_string();

        terminal.draw(|frame| {
            let area = frame.area();

            let status_area = Rect {
                x: area.x,
                y: area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            };

            let vmin = 0.005;
            let vmax = 0.3;

            for y in area.top()..area.bottom().saturating_sub(1) {
                for x in area.left()..area.right() {
                    let (lat, lon) = camera.screen_to_geo(x, y, area.width, area.height);

                    if let Some((lat_idx, lon_idx)) =
                        camera.geo_to_indices(lat, lon, &zarr_data.lat_data, &zarr_data.lon_data)
                    {
                        let mut idx = vec![0; data.ndim()];
                        idx[meta.lat_axis] = lat_idx;
                        idx[meta.lon_axis] = lon_idx;

                        if let Some(&value) = data.get(IxDyn(&idx)) {
                            let color = ColorMap::map_value(value, vmin, vmax);
                            let cell = frame.buffer_mut().get_mut(x, y);
                            cell.set_char(' ');
                            cell.set_bg(color);
                        }
                    }
                }
            }

            // render status bar
            let status_data = StatusBarData {
                cursor_lat: mouse_lat_lon.map(|(lat, _)| lat),
                cursor_lon: mouse_lat_lon.map(|(_, lon)| lon),
                cursor_value: clicked_value,
                camera_zoom: camera.zoom,
                variable_name: Some(current_var_name.clone()),
            };
            frame.render_widget(StatusBar::new(&status_data), status_area);

            if show_help {
                frame.render_widget(HelpOverlay::default(), area);
            }
        })?;
        let area = terminal.get_frame().area();
        while event::poll(std::time::Duration::from_millis(16))? {
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
                    // variables - switch and reload
                    KeyCode::Char('[') => {
                        if !zarr_data.variables.is_empty() {
                            let len = zarr_data.variables.len();
                            current_var_idx = (current_var_idx + len - 1) % len;
                            if let Ok((new_data, new_meta)) = load_variable_with_meta(
                                &zarr_data.store,
                                &zarr_data.variables[current_var_idx],
                            ) {
                                data = new_data;
                                meta = new_meta;
                            }
                        }
                    }
                    KeyCode::Char(']') => {
                        if !zarr_data.variables.is_empty() {
                            current_var_idx = (current_var_idx + 1) % zarr_data.variables.len();
                            if let Ok((new_data, new_meta)) = load_variable_with_meta(
                                &zarr_data.store,
                                &zarr_data.variables[current_var_idx],
                            ) {
                                data = new_data;
                                meta = new_meta;
                            }
                        }
                    }
                    _ => {}
                },
                Event::Mouse(mouse) => {
                    let (lat, lon) =
                        camera.screen_to_geo(mouse.column, mouse.row, area.width, area.height);
                    mouse_lat_lon = Some((lat, lon));

                    if let Some((lat_idx, lon_idx)) =
                        camera.geo_to_indices(lat, lon, &zarr_data.lat_data, &zarr_data.lon_data)
                    {
                        let mut idx = vec![0; data.ndim()];
                        idx[meta.lat_axis] = lat_idx;
                        idx[meta.lon_axis] = lon_idx;
                        clicked_value = data.get(IxDyn(&idx)).copied();
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

/// Load a 1D coordinate array, converting to f32 regardless of source type
fn load_coord_array(store: &Arc<FilesystemStore>, path: &str) -> Result<Vec<f32>> {
    use zarrs::array::DataType;

    let array = Array::open(store.clone(), path)?;
    let shape = array.shape().to_vec();
    let subset = ArraySubset::new_with_shape(shape);

    let data: Vec<f32> = match array.data_type() {
        DataType::Float32 => {
            let arr: ArrayD<f32> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.iter().copied().collect()
        }
        DataType::Float64 => {
            let arr: ArrayD<f64> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.iter().map(|&v| v as f32).collect()
        }
        DataType::Int32 => {
            let arr: ArrayD<i32> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.iter().map(|&v| v as f32).collect()
        }
        DataType::Int64 => {
            let arr: ArrayD<i64> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.iter().map(|&v| v as f32).collect()
        }
        dt => anyhow::bail!("Unsupported coordinate data type: {:?}", dt),
    };

    Ok(data)
}

/// Load a variable array, converting to f32 regardless of source type
fn load_variable_array(store: &Arc<FilesystemStore>, path: &str) -> Result<ArrayD<f32>> {
    use zarrs::array::DataType;

    let array = Array::open(store.clone(), path)?;
    let shape = array.shape().to_vec();
    let subset = ArraySubset::new_with_shape(shape);

    let data: ArrayD<f32> = match array.data_type() {
        DataType::Float32 => array.retrieve_array_subset_ndarray(&subset)?,
        DataType::Float64 => {
            let arr: ArrayD<f64> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.mapv(|v| v as f32)
        }
        DataType::Int32 => {
            let arr: ArrayD<i32> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.mapv(|v| v as f32)
        }
        DataType::Int64 => {
            let arr: ArrayD<i64> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.mapv(|v| v as f32)
        }
        DataType::UInt8 => {
            let arr: ArrayD<u8> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.mapv(|v| v as f32)
        }
        DataType::UInt16 => {
            let arr: ArrayD<u16> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.mapv(|v| v as f32)
        }
        dt => anyhow::bail!("Unsupported variable data type: {:?}", dt),
    };

    Ok(data)
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

/// Discover all arrays in a zarr store
fn discover_arrays(
    _store: &Arc<FilesystemStore>,
    base_path: &std::path::Path,
) -> Result<Vec<String>> {
    use std::fs;

    let mut arrays = Vec::new();

    for entry in fs::read_dir(base_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // skip hidden directories and zarr metadata
            if name.starts_with('.') || name == "__pycache__" {
                continue;
            }

            // check if this is a zarr array (has .zarray or zarr.json)
            let is_zarr_array = path.join(".zarray").exists() || path.join("zarr.json").exists();

            if is_zarr_array {
                arrays.push(format!("/{}", name));
            }
        }
    }

    arrays.sort();
    Ok(arrays)
}

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

/// Loaded zarr dataset
struct ZarrData {
    store: Arc<FilesystemStore>,
    lat_data: Vec<f32>,
    lon_data: Vec<f32>,
    variables: Vec<String>,
}

fn load_zarr_data(path: &str) -> Result<ZarrData> {
    let base_path = std::path::Path::new(path);
    let store = Arc::new(FilesystemStore::new(path)?);

    // discover arrays
    let arrays = discover_arrays(&store, base_path)?;

    // find coordinate arrays
    let lat_path = find_coord_array(&arrays, &["latitude", "lat", "y"])
        .ok_or_else(|| anyhow::anyhow!("No latitude coordinate found"))?;
    let lon_path = find_coord_array(&arrays, &["longitude", "lon", "x"])
        .ok_or_else(|| anyhow::anyhow!("No longitude coordinate found"))?;

    let lat_data = load_coord_array(&store, &lat_path)?;
    let lon_data = load_coord_array(&store, &lon_path)?;

    // find data variables
    let variables = find_data_variables(&arrays);
    if variables.is_empty() {
        anyhow::bail!("No data variables found in zarr store");
    }

    Ok(ZarrData {
        store,
        lat_data,
        lon_data,
        variables,
    })
}

/// Load a variable array with its metadata
fn load_variable_with_meta(
    store: &Arc<FilesystemStore>,
    path: &str,
) -> Result<(ArrayD<f32>, ArrayMeta)> {
    let array = Array::open(store.clone(), path)?;
    let meta = ArrayMeta::from_array(&array)
        .ok_or_else(|| anyhow::anyhow!("Missing dimension names in {}", path))?;
    let data = load_variable_array(store, path)?;
    Ok((data, meta))
}
