use ratatui::style::Color;

pub struct ColorMap;

impl ColorMap {
    pub fn map_value(value: f64, min: f64, max: f64) -> Color {
        // normalise to 0-1
        let t = if (max - min).abs() < 1e-10 {
            0.5
        } else {
            ((value - min) / (max - min)).clamp(0.0, 1.0)
        };

        // gist_ncar key colours (sampled from matplotlib)
        const GIST_NCAR: [(f64, u8, u8, u8); 9] = [
            (0.000, 0, 0, 128),
            (0.125, 0, 76, 255),
            (0.250, 0, 206, 209),
            (0.375, 0, 255, 76),
            (0.500, 127, 255, 0),
            (0.625, 255, 230, 0),
            (0.750, 255, 105, 0),
            (0.875, 255, 0, 110),
            (1.000, 128, 0, 206),
        ];

        // find segment
        let i = ((t * 8.0).floor() as usize).min(7);
        let j = i + 1;

        let (t0, r0, g0, b0) = GIST_NCAR[i];
        let (t1, r1, g1, b1) = GIST_NCAR[j];
        let f = (t - t0) / (t1 - t0);

        let lerp = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * f) as u8;
        Color::Rgb(lerp(r0, r1), lerp(g0, g1), lerp(b0, b1))
    }
}
