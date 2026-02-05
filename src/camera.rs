/// simplified version
#[derive(Debug)]
pub struct Camera {
    pub center_lat: f32,
    pub center_lon: f32,
    pub zoom: f32,
}

impl Camera {
    pub fn new(center_lat: f32, center_lon: f32, zoom: f32) -> Self {
        Self {
            center_lat,
            center_lon,
            zoom,
        }
    }

    pub fn pan(&mut self, dlat: f32, dlon: f32) {
        self.center_lat += dlat;
        self.center_lon += dlon;

        // clamp to valid values
        self.center_lat = self.center_lat.clamp(-90.0, 90.0);
        self.center_lon = self.center_lon.clamp(-180.0, 180.0);
    }

    pub fn zoom_in(&mut self) {
        self.zoom *= 0.8;
        self.zoom = self.zoom.max(0.001);
    }

    pub fn zoom_out(&mut self) {
        self.zoom *= 1.2;
        self.zoom = self.zoom.min(100.0);
    }

    pub fn set(&mut self, center_lat: f32, center_lon: f32, zoom: f32) {
        self.center_lat = center_lat;
        self.center_lon = center_lon;
        self.zoom = zoom;
    }

    pub fn reset(&mut self) {
        self.center_lat = 0.0;
        self.center_lon = 0.0;
        self.zoom = 1.0;
    }

    pub fn screen_to_geo(
        &self,
        screen_x: u16,
        screen_y: u16,
        screen_width: u16,
        screen_height: u16,
    ) -> (f32, f32) {
        let center_x = screen_width as f32 / 2.0;
        let center_y = screen_height as f32 / 2.0;

        let dx = screen_x as f32 - center_x;
        let dy = screen_y as f32 - center_y;

        let lon = self.center_lon + dx * self.zoom;
        // negative because screen Y is inverted
        let lat = self.center_lat - dy * self.zoom;

        (lat, lon)
    }

    pub fn geo_to_indices(
        &self,
        lat: f32,
        lon: f32,
        lat_coords: &[f32],
        lon_coords: &[f32],
    ) -> Option<(usize, usize)> {
        let lat_idx = binary_search_nearest(lat_coords, lat)?;
        let lon_idx = binary_search_nearest(lon_coords, lon)?;
        Some((lat_idx, lon_idx))
    }
}

/// Binary search to find nearest index in a sorted array (ascending or descending)
fn binary_search_nearest(coords: &[f32], value: f32) -> Option<usize> {
    if coords.is_empty() {
        return None;
    }

    // check if value is in range
    let (min, max) = if coords[0] < coords[coords.len() - 1] {
        (coords[0], coords[coords.len() - 1])
    } else {
        (coords[coords.len() - 1], coords[0])
    };

    if value < min || value > max {
        return None;
    }

    // determine if ascending or descending
    let ascending = coords[0] < coords[coords.len() - 1];

    let idx = if ascending {
        coords.partition_point(|&x| x < value)
    } else {
        coords.partition_point(|&x| x > value)
    };

    // clamp and find nearest between idx-1 and idx
    let idx = idx.min(coords.len() - 1);
    if idx == 0 {
        return Some(0);
    }

    let prev = idx - 1;
    if (coords[prev] - value).abs() < (coords[idx] - value).abs() {
        Some(prev)
    } else {
        Some(idx)
    }
}
