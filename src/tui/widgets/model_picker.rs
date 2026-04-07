use crate::provider::models_dev::ModelChoice;
use crate::tui::styles::Styles;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, StatefulWidget, Widget},
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
            .map(|m| {
                let label = format!("{} [{}]", m.display_name, m.qualified_id());
                ListItem::new(label)
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_symbol("> ")
            .highlight_style(
                Style::default()
                    .fg(self.styles.accent)
                    .add_modifier(Modifier::BOLD),
            );

        let mut state = ListState::default().with_selected(Some(self.highlight));
        StatefulWidget::render(list, popup_area, buf, &mut state);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_model(n: usize) -> ModelChoice {
        ModelChoice {
            provider_id: "test".to_string(),
            model_id: format!("model-{n:03}"),
            display_name: format!("Model-{n:03}"),
            context_length: None,
        }
    }

    fn buffer_text(buf: &Buffer) -> String {
        let area = buf.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(area.x + x, area.y + y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    // Terminal area 80x24 → centered_rect(60, 50, ...) gives a popup of ~12 rows and
    // ~10 inner rows (after borders). With 15 models the list overflows by 5, so
    // scrolling is required to reach the last item.
    fn render_picker(highlight: usize) -> String {
        let models: Vec<ModelChoice> = (0..15).map(make_model).collect();
        let styles = Styles::new();
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        ModelPicker {
            styles: &styles,
            models: &models,
            highlight,
        }
        .render(area, &mut buf);
        buffer_text(&buf)
    }

    #[test]
    fn test_scroll_to_end_of_overflow_list() {
        let text = render_picker(14);
        assert!(
            text.contains("Model-014"),
            "Expected 'Model-014' to be visible, got:\n{text}"
        );
        assert!(
            !text.contains("Model-000"),
            "Expected 'Model-000' to be scrolled off, but it was visible:\n{text}"
        );
    }

    #[test]
    fn test_scroll_to_start_of_overflow_list() {
        let text = render_picker(0);
        assert!(
            text.contains("Model-000"),
            "Expected 'Model-000' to be visible, got:\n{text}"
        );
        assert!(
            !text.contains("Model-014"),
            "Expected 'Model-014' to be out of view, but it was visible:\n{text}"
        );
    }
}
