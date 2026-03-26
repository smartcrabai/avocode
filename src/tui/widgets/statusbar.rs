use crate::tui::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

pub struct StatusBar<'a> {
    pub theme: &'a Theme,
    pub model: &'a str,
    pub mode: &'a str,
    pub keys_hint: &'a str,
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let left = Line::from(vec![
            Span::styled(
                format!(" {} ", self.mode),
                Style::default()
                    .fg(self.theme.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("| {} ", self.model)),
        ]);
        let right = Line::from(vec![Span::styled(
            self.keys_hint,
            Style::default().fg(self.theme.muted),
        )]);

        Paragraph::new(left).render(area, buf);
        Paragraph::new(right)
            .alignment(Alignment::Right)
            .render(area, buf);
    }
}
