mod camera;
mod colormap;

use ndarray::{ArrayD, IxDyn};
use std::{io, sync::Arc};
use zarrs::{array::Array, array_subset::ArraySubset};

struct ArrayMeta {
    shape: Vec<u64>,
    lon_axis: usize,
    lat_axis: usize,
}

impl ArrayMeta {
    fn from_array<TStorage: zarrs::storage::ReadableStorageTraits + ?Sized>(
        array: &Array<TStorage>,
    ) -> Option<Self> {
        let shape = array.shape().to_vec();
        let dim_names = array.dimension_names().as_ref()?;

        let lon_axis = dim_names.iter().position(|n| {
            n.as_ref()
                .map(|s| matches!(s.as_str(), "lon" | "longitude" | "x"))
                .unwrap_or(false)
        })?;
        let lat_axis = dim_names.iter().position(|n| {
            n.as_ref()
                .map(|s| matches!(s.as_str(), "lat" | "latitude" | "y"))
                .unwrap_or(false)
        })?;

        Some(Self {
            shape,
            lon_axis,
            lat_axis,
        })
    }

    fn lon_size(&self) -> usize {
        self.shape[self.lon_axis] as usize
    }
    fn lat_size(&self) -> usize {
        self.shape[self.lat_axis] as usize
    }

    fn make_index(&self, lon_idx: usize, lat_idx: usize) -> IxDyn {
        let mut idx = vec![0; self.shape.len()];
        idx[self.lon_axis] = lon_idx;
        idx[self.lat_axis] = lat_idx;
        IxDyn(&idx)
    }
}

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    layout::Rect,
    prelude::CrosstermBackend,
    style::{Color, Style},
    widgets::Paragraph,
};

use camera::Camera;
use colormap::ColorMap;
use zarrs_filesystem::FilesystemStore;

#[tokio::main]
async fn main() -> Result<()> {
    let (lat_data, lon_data, temp_data, meta) = load_zarr_data().await?;

    // terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // app state
    let mut should_quit = false;
    let mut camera = Camera::new(0.0, 0.0, 1.0);
    let mut mouse_lat_lon: Option<(f64, f64)> = None;
    let mut clicked_value: Option<f64> = None;

    // main loop
    while !should_quit {
        terminal.draw(|frame| {
            let area = frame.area();

            // find min/max for colour scaling
            let min_temp = temp_data.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max_temp = temp_data.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));

            // render each cell as a coloured space

            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    // Convert screen position to geographic coordinates using camera
                    let (lat, lon) = camera.screen_to_geo(x, y, area.width, area.height);

                    // Convert geographic to array indices
                    if let Some((lat_idx, lon_idx)) =
                        camera.geo_to_indices(lat, lon, &lat_data, &lon_data)
                    {
                        if lon_idx < meta.lon_size() && lat_idx < meta.lat_size() {
                            let idx = meta.make_index(lon_idx, lat_idx);
                            let value = temp_data[&idx];
                            let color = ColorMap::map_value(value, min_temp, max_temp);

                            let cell = frame.buffer_mut().get_mut(x, y);
                            cell.set_char(' ');
                            cell.set_bg(color);
                        }
                    }
                }
            }
            // render status bar
            let status = match (mouse_lat_lon, clicked_value) {
                (Some((lat, lon)), Some(val)) => {
                    format!("Lat: {:.2}, Lon: {:.2} | Value: {:.4}", lat, lon, val)
                }
                (Some((lat, lon)), None) => format!("Lat: {:.2}, Lon: {:.2}", lat, lon),
                _ => String::new(),
            };
            let status_widget =
                Paragraph::new(status).style(Style::default().fg(Color::White).bg(Color::Black));
            frame.render_widget(
                status_widget,
                Rect::new(0, area.height.saturating_sub(1), area.width, 1),
            );
        })?;
        let area = terminal.get_frame().area();
        while event::poll(std::time::Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') => should_quit = true,
                    KeyCode::Char('+') | KeyCode::Char('=') => camera.zoom_in(),
                    KeyCode::Char('-') => camera.zoom_out(),
                    KeyCode::Char('r') => camera.reset(),
                    KeyCode::Left => camera.pan(0.0, -5.0),
                    KeyCode::Right => camera.pan(0.0, 5.0),
                    KeyCode::Up => camera.pan(5.0, 0.0),
                    KeyCode::Down => camera.pan(-5.0, 0.0),
                    KeyCode::Char('h') => camera.pan(0.0, -5.0),
                    KeyCode::Char('l') => camera.pan(0.0, 5.0),
                    KeyCode::Char('k') => camera.pan(5.0, 0.0),
                    KeyCode::Char('j') => camera.pan(-5.0, 0.0),
                    _ => {}
                },
                Event::Mouse(mouse) => {
                    let (lat, lon) =
                        camera.screen_to_geo(mouse.column, mouse.row, area.width, area.height);
                    mouse_lat_lon = Some((lat, lon));

                    if let Some((lat_idx, lon_idx)) =
                        camera.geo_to_indices(lat, lon, &lat_data, &lon_data)
                    {
                        if lon_idx < meta.lon_size() && lat_idx < meta.lat_size() {
                            let idx = meta.make_index(lon_idx, lat_idx);
                            clicked_value = Some(temp_data[&idx]);
                        } else {
                            clicked_value = None;
                        }
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

async fn load_zarr_data() -> Result<(Vec<f32>, Vec<f32>, ArrayD<f64>, ArrayMeta)> {
    // let path = "sample.zarr";
    let path = "data.zarr";
    let store = Arc::new(FilesystemStore::new(path)?);

    let lat_array = Array::open(store.clone(), "/latitude")?;
    let lat_shape = lat_array.shape().to_vec();
    let lat: ArrayD<f32> =
        lat_array.retrieve_array_subset_ndarray(&ArraySubset::new_with_shape(lat_shape))?;

    let lon_array = Array::open(store.clone(), "/longitude")?;
    let lon_shape = lon_array.shape().to_vec();
    let lon: ArrayD<f32> =
        lon_array.retrieve_array_subset_ndarray(&ArraySubset::new_with_shape(lon_shape))?;

    let temp_array = Array::open(store.clone(), "/cat3_probability")?;
    let meta = ArrayMeta::from_array(&temp_array)
        .ok_or_else(|| anyhow::anyhow!("Missing dimension names (lon/lat) in array metadata"))?;
    let shape = temp_array.shape().to_vec();
    let temp: ArrayD<f64> =
        temp_array.retrieve_array_subset_ndarray(&ArraySubset::new_with_shape(shape))?;

    Ok((
        lat.iter().copied().collect(),
        lon.iter().copied().collect(),
        temp,
        meta,
    ))
}
