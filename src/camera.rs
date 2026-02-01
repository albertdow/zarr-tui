/// simplified version
#[derive(Debug)]
pub struct Camera {
    pub center_lat: f64,
    pub center_lon: f64,
    pub zoom: f64,
}

impl Camera {
    pub fn new(center_lat: f64, center_lon: f64, zoom: f64) -> Self {
        Self {
            center_lat,
            center_lon,
            zoom,
        }
    }

    pub fn pan(&mut self, dlat: f64, dlon: f64) {
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
    ) -> (f64, f64) {
        let center_x = screen_width as f64 / 2.0;
        let center_y = screen_height as f64 / 2.0;

        let dx = screen_x as f64 - center_x;
        let dy = screen_y as f64 - center_y;

        let lon = self.center_lon + dx * self.zoom;
        // negative because screen Y is inverted
        let lat = self.center_lat - dy * self.zoom;

        (lat, lon)
    }

    pub fn geo_to_indices(
        &self,
        lat: f64,
        lon: f64,
        lat_coords: &[f32],
        lon_coords: &[f32],
    ) -> Option<(usize, usize)> {
        let lat_idx = lat_coords
            .iter()
            .position(|&coord_lat| (coord_lat - lat as f32).abs() < 2.0)?;

        let lon_idx = lon_coords
            .iter()
            .position(|&coord_lon| (coord_lon - lon as f32).abs() < 2.0)?;

        Some((lat_idx, lon_idx))
    }
}
