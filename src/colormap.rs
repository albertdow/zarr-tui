use ratatui::style::Color;

/// Available colormap types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColormapType {
    Viridis,
    Plasma,
    Inferno,
    Magma,
    Coolwarm,
    Turbo,
    GistNcar,
}

impl ColormapType {
    pub const ALL: [ColormapType; 7] = [
        ColormapType::Viridis,
        ColormapType::Plasma,
        ColormapType::Inferno,
        ColormapType::Magma,
        ColormapType::Coolwarm,
        ColormapType::Turbo,
        ColormapType::GistNcar,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            ColormapType::Viridis => "viridis",
            ColormapType::Plasma => "plasma",
            ColormapType::Inferno => "inferno",
            ColormapType::Magma => "magma",
            ColormapType::Coolwarm => "coolwarm",
            ColormapType::Turbo => "turbo",
            ColormapType::GistNcar => "gist_ncar",
        }
    }

    pub fn next(&self) -> ColormapType {
        let idx = ColormapType::ALL
            .iter()
            .position(|c| c == self)
            .unwrap_or(0);
        ColormapType::ALL[(idx + 1) % ColormapType::ALL.len()]
    }

    pub fn prev(&self) -> ColormapType {
        let idx = ColormapType::ALL
            .iter()
            .position(|c| c == self)
            .unwrap_or(0);
        ColormapType::ALL[(idx + ColormapType::ALL.len() - 1) % ColormapType::ALL.len()]
    }
}

pub struct ColorMap;

impl ColorMap {
    /// Map a value to a color using the specified colormap
    pub fn map_value(value: f32, min: f32, max: f32, cmap: ColormapType) -> Color {
        let t = if (max - min).abs() < 1e-10 {
            0.5
        } else {
            ((value - min) / (max - min)).clamp(0.0, 1.0)
        };

        let colors = match cmap {
            ColormapType::Viridis => &VIRIDIS,
            ColormapType::Plasma => &PLASMA,
            ColormapType::Inferno => &INFERNO,
            ColormapType::Magma => &MAGMA,
            ColormapType::Coolwarm => &COOLWARM,
            ColormapType::Turbo => &TURBO,
            ColormapType::GistNcar => &GIST_NCAR,
        };

        interpolate_colormap(colors, t)
    }

    /// Get a color at normalised position t (0-1) for colorbar
    pub fn get_color_at(t: f32, cmap: ColormapType) -> Color {
        let colors = match cmap {
            ColormapType::Viridis => &VIRIDIS,
            ColormapType::Plasma => &PLASMA,
            ColormapType::Inferno => &INFERNO,
            ColormapType::Magma => &MAGMA,
            ColormapType::Coolwarm => &COOLWARM,
            ColormapType::Turbo => &TURBO,
            ColormapType::GistNcar => &GIST_NCAR,
        };

        interpolate_colormap(colors, t.clamp(0.0, 1.0))
    }
}

fn interpolate_colormap(colors: &[(f32, u8, u8, u8)], t: f32) -> Color {
    let n = colors.len();
    if n == 0 {
        return Color::Black;
    }
    if n == 1 {
        let (_, r, g, b) = colors[0];
        return Color::Rgb(r, g, b);
    }

    // Find segment
    let mut i = 0;
    for (idx, &(pos, _, _, _)) in colors.iter().enumerate() {
        if pos <= t {
            i = idx;
        }
    }
    let j = (i + 1).min(n - 1);

    let (t0, r0, g0, b0) = colors[i];
    let (t1, r1, g1, b1) = colors[j];

    let f = if (t1 - t0).abs() < 1e-10 {
        0.0
    } else {
        ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
    };

    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f) as u8;
    Color::Rgb(lerp(r0, r1), lerp(g0, g1), lerp(b0, b1))
}

// Colormap definitions (sampled key colors)

const VIRIDIS: [(f32, u8, u8, u8); 9] = [
    (0.000, 68, 1, 84),
    (0.125, 72, 40, 120),
    (0.250, 62, 74, 137),
    (0.375, 49, 104, 142),
    (0.500, 38, 130, 142),
    (0.625, 31, 158, 137),
    (0.750, 53, 183, 121),
    (0.875, 109, 205, 89),
    (1.000, 253, 231, 37),
];

const PLASMA: [(f32, u8, u8, u8); 9] = [
    (0.000, 13, 8, 135),
    (0.125, 75, 3, 161),
    (0.250, 125, 3, 168),
    (0.375, 168, 34, 150),
    (0.500, 203, 70, 121),
    (0.625, 229, 107, 93),
    (0.750, 248, 148, 65),
    (0.875, 253, 195, 40),
    (1.000, 240, 249, 33),
];

const INFERNO: [(f32, u8, u8, u8); 9] = [
    (0.000, 0, 0, 4),
    (0.125, 31, 12, 72),
    (0.250, 85, 15, 109),
    (0.375, 136, 34, 106),
    (0.500, 186, 54, 85),
    (0.625, 227, 89, 51),
    (0.750, 249, 140, 10),
    (0.875, 249, 201, 50),
    (1.000, 252, 255, 164),
];

const MAGMA: [(f32, u8, u8, u8); 9] = [
    (0.000, 0, 0, 4),
    (0.125, 28, 16, 68),
    (0.250, 79, 18, 123),
    (0.375, 129, 37, 129),
    (0.500, 181, 54, 122),
    (0.625, 229, 80, 100),
    (0.750, 251, 135, 97),
    (0.875, 254, 194, 135),
    (1.000, 252, 253, 191),
];

const COOLWARM: [(f32, u8, u8, u8); 9] = [
    (0.000, 59, 76, 192),
    (0.125, 98, 130, 234),
    (0.250, 141, 176, 254),
    (0.375, 184, 208, 249),
    (0.500, 221, 221, 221),
    (0.625, 245, 196, 173),
    (0.750, 244, 154, 123),
    (0.875, 222, 96, 77),
    (1.000, 180, 4, 38),
];

const TURBO: [(f32, u8, u8, u8); 9] = [
    (0.000, 48, 18, 59),
    (0.125, 86, 91, 214),
    (0.250, 25, 160, 251),
    (0.375, 29, 221, 161),
    (0.500, 126, 253, 75),
    (0.625, 208, 234, 43),
    (0.750, 254, 176, 42),
    (0.875, 243, 94, 21),
    (1.000, 122, 4, 3),
];

const GIST_NCAR: [(f32, u8, u8, u8); 9] = [
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
