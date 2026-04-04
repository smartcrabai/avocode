use crate::skill::SkillInfo;
use crate::tui::styles::Styles;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Widget},
};

/// Stateless popup renderer for slash-skill completion.
///
/// Accepts pre-filtered skills from `App::slash_filtered_skills`.
/// All state lives in `App`.
pub struct SlashCompletion<'a> {
    pub styles: &'a Styles,
    pub skills: &'a [&'a SkillInfo],
    pub highlight: usize,
}

impl Widget for SlashCompletion<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.skills.is_empty() {
            return;
        }

        let height = (u16::try_from(self.skills.len())
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
            .title(" Skills (Enter/Tab apply | Esc close) ")
            .border_style(Style::default().fg(self.styles.accent));

        let items: Vec<ListItem> = self
            .skills
            .iter()
            .copied()
            .enumerate()
            .map(|(i, s)| {
                if i == self.highlight {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("> /{}", s.name),
                            Style::default()
                                .fg(self.styles.accent)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" - {}", s.description),
                            Style::default().fg(self.styles.muted),
                        ),
                    ]))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  /{}", s.name), Style::default()),
                        Span::styled(
                            format!(" - {}", s.description),
                            Style::default().fg(self.styles.muted),
                        ),
                    ]))
                }
            })
            .collect();

        List::new(items).block(block).render(popup_area, buf);
    }
}
