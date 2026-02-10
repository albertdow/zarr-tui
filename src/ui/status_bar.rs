use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// Data to display in status bar
pub struct StatusBarData {
    pub cursor_lat: Option<f32>,
    pub cursor_lon: Option<f32>,
    pub cursor_value: Option<f32>,
    pub camera_zoom: f32,
    pub variable_name: Option<String>,
    pub cached_chunks: Option<usize>,
    pub visible_chunks: Option<usize>,
    pub pending_chunks: Option<usize>,
}

impl Default for StatusBarData {
    fn default() -> Self {
        Self {
            cursor_lat: None,
            cursor_lon: None,
            cursor_value: None,
            camera_zoom: 1.0,
            variable_name: None,
            cached_chunks: None,
            visible_chunks: None,
            pending_chunks: None,
        }
    }
}

pub struct StatusBar<'a> {
    data: &'a StatusBarData,
}

impl<'a> StatusBar<'a> {
    pub fn new(data: &'a StatusBarData) -> Self {
        Self { data }
    }

    fn format_position(&self) -> String {
        match (self.data.cursor_lat, self.data.cursor_lon) {
            (Some(lat), Some(lon)) => {
                let lat_dir = if lat >= 0.0 { "˚N" } else { "˚S" };
                let lon_dir = if lon >= 0.0 { "˚E" } else { "˚W" };
                format!("{:6.2}{} {:7.2}{}", lat.abs(), lat_dir, lon.abs(), lon_dir)
            }
            _ => "  --.-    --.-    ".to_string(),
        }
    }

    fn format_value(&self) -> String {
        match self.data.cursor_value {
            Some(v) if v.is_finite() => format!("Val: {:8.4}", v),
            _ => "Val:  --  ".to_string(),
        }
    }

    fn format_zoom(&self) -> String {
        format!("Zoom: {:.2}x", 1.0 / self.data.camera_zoom)
    }

    fn format_chunks(&self) -> Option<String> {
        match (self.data.cached_chunks, self.data.visible_chunks, self.data.pending_chunks) {
            (Some(cached), Some(visible), Some(pending)) if pending > 0 => {
                Some(format!("Chunks: {}/{} (loading {}...)", cached, visible, pending))
            }
            (Some(cached), Some(visible), _) => Some(format!("Chunks: {}/{}", cached, visible)),
            _ => None,
        }
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style = Style::default()
            .fg(ratatui::style::Color::White)
            .bg(ratatui::style::Color::DarkGray);
        let highlight = Style::default()
            .fg(ratatui::style::Color::Yellow)
            .bg(ratatui::style::Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        let var_name = self.data.variable_name.as_deref().unwrap_or("--");

        let mut spans = vec![
            Span::styled(" Pos: ", style),
            Span::styled(self.format_position(), highlight),
            Span::styled(" | ", style),
            Span::styled(self.format_value(), style),
            Span::styled(" | ", style),
            Span::styled(self.format_zoom(), style),
            Span::styled(" | Var: ", style),
            Span::styled(var_name, highlight),
        ];

        if let Some(chunks) = self.format_chunks() {
            spans.push(Span::styled(" | ", style));
            spans.push(Span::styled(chunks, style));
        }

        let line = Line::from(spans);
        Paragraph::new(line).style(style).render(area, buf);
    }
}
