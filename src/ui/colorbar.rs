use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

use crate::colormap::{ColorMap, ColormapType};

/// A vertical colorbar legend widget
pub struct Colorbar {
    vmin: f32,
    vmax: f32,
    cmap: ColormapType,
    num_labels: usize,
}

impl Colorbar {
    pub fn new(vmin: f32, vmax: f32, cmap: ColormapType) -> Self {
        Self {
            vmin,
            vmax,
            cmap,
            num_labels: 5,
        }
    }

    fn format_value(&self, value: f32) -> String {
        let abs = value.abs();
        if abs == 0.0 {
            "0".to_string()
        } else if abs >= 1000.0 {
            format!("{:.0e}", value)
        } else if abs >= 1.0 {
            format!("{:.2}", value)
        } else if abs >= 0.01 {
            format!("{:.3}", value)
        } else {
            format!("{:.2e}", value)
        }
    }
}

impl Widget for Colorbar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 6 || area.height < 3 {
            return;
        }

        // Layout: [gradient 2 cols] [space 1 col] [labels rest]
        let gradient_width = 2;
        let label_start = gradient_width + 1;
        let bar_height = area.height.saturating_sub(1); // leave space for colormap name

        // Draw colormap name at bottom
        let name = self.cmap.name();
        let name_y = area.y + area.height - 1;
        let name_style = Style::default().fg(ratatui::style::Color::DarkGray);
        for (i, ch) in name.chars().take(area.width as usize).enumerate() {
            buf[(area.x + i as u16, name_y)]
                .set_char(ch)
                .set_style(name_style);
        }

        // Draw gradient (vertical, high values at top)
        for row in 0..bar_height {
            let t = 1.0 - (row as f32 / (bar_height.saturating_sub(1).max(1)) as f32);
            let color = ColorMap::get_color_at(t, self.cmap);

            for col in 0..gradient_width {
                let cell = &mut buf[(area.x + col, area.y + row)];
                cell.set_char(' ');
                cell.set_bg(color);
            }
        }

        // Draw labels
        if self.num_labels > 0 && area.width > label_start {
            let label_width = area.width - label_start;
            let label_style = Style::default().fg(ratatui::style::Color::Gray);

            for i in 0..self.num_labels {
                let frac = if self.num_labels > 1 {
                    i as f32 / (self.num_labels - 1) as f32
                } else {
                    0.5
                };
                let row = (frac * (bar_height.saturating_sub(1)) as f32) as u16;
                let value = self.vmax - frac * (self.vmax - self.vmin);
                let label = self.format_value(value);

                buf[(area.x + gradient_width, area.y + row)]
                    .set_char('─')
                    .set_style(label_style);

                for (j, ch) in label.chars().take(label_width as usize).enumerate() {
                    buf[(area.x + label_start + j as u16, area.y + row)]
                        .set_char(ch)
                        .set_style(label_style);
                }
            }
        }
    }
}
