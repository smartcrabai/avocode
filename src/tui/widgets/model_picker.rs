use crate::provider::models_dev::ModelChoice;
use crate::tui::styles::Styles;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Widget},
};

/// Centered popup overlay for selecting the active model.
///
/// Stateless renderer -- all state lives in `App`.
pub struct ModelPicker<'a> {
    pub styles: &'a Styles,
    pub models: &'a [ModelChoice],
    pub highlight: usize,
}

impl Widget for ModelPicker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let popup_area = centered_rect(60, 50, area);

        // Clear the background so the popup is readable over any content below.
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Select Model (Enter apply | Esc close) ")
            .border_style(Style::default().fg(self.styles.accent));

        let items: Vec<ListItem> = self
            .models
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let label = format!("{} [{}]", m.display_name, m.qualified_id());
                if i == self.highlight {
                    ListItem::new(Line::from(vec![Span::styled(
                        format!("> {label}"),
                        Style::default()
                            .fg(self.styles.accent)
                            .add_modifier(Modifier::BOLD),
                    )]))
                } else {
                    ListItem::new(format!("  {label}"))
                }
            })
            .collect();

        List::new(items).block(block).render(popup_area, buf);
    }
}

/// Return a rectangle centred within `area`, sized at `percent_x` x `percent_y`.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let margin_v = (100 - percent_y) / 2;
    let margin_h = (100 - percent_x) / 2;

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(margin_v),
            Constraint::Percentage(percent_y),
            Constraint::Percentage(margin_v),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(margin_h),
            Constraint::Percentage(percent_x),
            Constraint::Percentage(margin_h),
        ])
        .split(vert[1])[1]
}
