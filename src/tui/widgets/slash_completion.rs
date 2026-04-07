use crate::tui::app::SlashEntry;
use crate::tui::styles::Styles;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Widget},
};

/// Stateless popup renderer for slash-command completion.
///
/// Accepts pre-filtered entries from `App::slash_filtered_entries`.
/// All state lives in `App`.
pub struct SlashCompletion<'a> {
    pub styles: &'a Styles,
    pub entries: &'a [SlashEntry],
    pub highlight: usize,
}

impl Widget for SlashCompletion<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.entries.is_empty() {
            return;
        }

        let height = (u16::try_from(self.entries.len())
            .unwrap_or(u16::MAX)
            .saturating_add(2))
        .min(area.height / 2)
        .max(3);
        let width = (50u16).min(area.width);
        let popup_area = Rect {
            x: area.x,
            y: area
                .y
                .saturating_add(area.height.saturating_sub(height + 6)),
            width,
            height,
        };

        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Commands (Enter/Tab apply | Esc close) ")
            .border_style(Style::default().fg(self.styles.accent));

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                if i == self.highlight {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("> /{}", e.name),
                            Style::default()
                                .fg(self.styles.accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" - {}", e.description),
                            Style::default().fg(self.styles.muted),
                        ),
                    ]))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  /{}", e.name), Style::default()),
                        Span::styled(
                            format!(" - {}", e.description),
                            Style::default().fg(self.styles.muted),
                        ),
                    ]))
                }
            })
            .collect();

        List::new(items).block(block).render(popup_area, buf);
    }
}
