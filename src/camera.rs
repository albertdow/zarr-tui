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

    /// Get the range of data indices that a screen pixel covers.
    /// Returns ((lat_start, lat_end), (lon_start, lon_end)) as exclusive ranges.
    /// When zoomed in (pixel covers < 1 data point), returns single-element range.
    pub fn pixel_to_index_range(
        &self,
        screen_x: u16,
        screen_y: u16,
        screen_width: u16,
        screen_height: u16,
        lat_coords: &[f32],
        lon_coords: &[f32],
    ) -> Option<((usize, usize), (usize, usize))> {
        // get geo bounds for this pixel (corners)
        let (lat0, lon0) = self.screen_to_geo(screen_x, screen_y, screen_width, screen_height);
        let (lat1, lon1) =
            self.screen_to_geo(screen_x + 1, screen_y + 1, screen_width, screen_height);

        let lat_min = lat0.min(lat1);
        let lat_max = lat0.max(lat1);
        let lon_min = lon0.min(lon1);
        let lon_max = lon0.max(lon1);

        // find index ranges
        let lat_idx_min = binary_search_nearest(lat_coords, lat_min)?;
        let lat_idx_max = binary_search_nearest(lat_coords, lat_max)?;
        let lon_idx_min = binary_search_nearest(lon_coords, lon_min)?;
        let lon_idx_max = binary_search_nearest(lon_coords, lon_max)?;

        // ensure min <= max (handles descending coords)
        let (lat_start, lat_end) = (
            lat_idx_min.min(lat_idx_max),
            lat_idx_min.max(lat_idx_max) + 1,
        );
        let (lon_start, lon_end) = (
            lon_idx_min.min(lon_idx_max),
            lon_idx_min.max(lon_idx_max) + 1,
        );

        Some(((lat_start, lat_end), (lon_start, lon_end)))
    }

    /// Calculate how many data points per screen pixel (for deciding sampling strategy)
    pub fn data_points_per_pixel(&self, lat_coords: &[f32], lon_coords: &[f32]) -> f32 {
        if lat_coords.len() < 2 || lon_coords.len() < 2 {
            return 1.0;
        }
        // average spacing in coords
        let lat_span = (lat_coords[0] - lat_coords[lat_coords.len() - 1]).abs();
        let lon_span = (lon_coords[0] - lon_coords[lon_coords.len() - 1]).abs();
        let lat_spacing = lat_span / lat_coords.len() as f32;
        let lon_spacing = lon_span / lon_coords.len() as f32;

        // zoom is degrees per pixel
        let points_per_pixel_lat = self.zoom / lat_spacing;
        let points_per_pixel_lon = self.zoom / lon_spacing;

        points_per_pixel_lat.max(points_per_pixel_lon)
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
