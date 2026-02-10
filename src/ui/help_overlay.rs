use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

pub struct HelpOverlay;

impl Default for HelpOverlay {
    fn default() -> Self {
        Self
    }
}

impl HelpOverlay {
    fn keybindings() -> Vec<(&'static str, &'static str)> {
        vec![
            ("Navigation", ""),
            ("  Arrow keys / hjkl", "Pan view"),
            ("  + / =", "Zoom in"),
            ("  -", "Zoom out"),
            ("  r", "Reset view"),
            ("", ""),
            ("Data", ""),
            ("  [  /  ]", "Previous / next variable"),
            ("  c  /  C", "Next / previous colormap"),
            ("", ""),
            ("Display", ""),
            ("  ?", "Toggle this help"),
            ("  q", "Quit"),
        ]
    }

    fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
        let popup_width = area.width * percent_x / 100;
        let popup_height = area.height * percent_y / 100;
        let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
        Rect::new(x, y, popup_width, popup_height)
    }
}

impl Widget for HelpOverlay {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let popup_area = Self::centered_rect(60, 70, area);

        // clear bg
        Clear.render(popup_area, buf);

        let key_style = Style::default()
            .fg(ratatui::style::Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let header_style = Style::default()
            .fg(ratatui::style::Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

        let lines: Vec<Line> = Self::keybindings()
            .iter()
            .map(|(key, desc)| {
                if desc.is_empty() && !key.is_empty() {
                    Line::from(Span::styled(*key, header_style))
                } else if key.is_empty() {
                    Line::from("")
                } else {
                    Line::from(vec![
                        Span::styled(format!("{:<22}", key), key_style),
                        Span::raw(*desc),
                    ])
                }
            })
            .collect();

        let block = Block::default()
            .title(" Help - Press ? to close ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ratatui::style::Color::Cyan))
            .style(Style::default().bg(ratatui::style::Color::Black));

        Paragraph::new(lines).block(block).render(popup_area, buf);
    }
}
