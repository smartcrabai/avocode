use crate::tui::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget, Widget},
};

pub struct SidebarItem {
    pub id: String,
    pub title: String,
    pub updated_at: String,
}

pub struct SidebarState {
    pub items: Vec<SidebarItem>,
    pub selected: usize,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
        }
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.items.is_empty() {
            self.selected = self.selected.saturating_sub(1);
        }
    }
}

pub struct SidebarWidget<'a> {
    pub theme: &'a Theme,
}

impl StatefulWidget for SidebarWidget<'_> {
    type State = SidebarState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Sessions")
            .border_style(self.theme.border_style())
            .title_style(self.theme.title_style());
        let inner = block.inner(area);
        block.render(area, buf);
        let items: Vec<ListItem> = state
            .items
            .iter()
            .map(|s| ListItem::new(s.title.as_str()))
            .collect();
        let mut list_state = ListState::default();
        list_state.select(Some(state.selected));
        StatefulWidget::render(
            List::new(items).highlight_style(
                Style::default()
                    .bg(self.theme.selection_bg)
                    .fg(self.theme.selection_fg)
                    .add_modifier(Modifier::BOLD),
            ),
            inner,
            buf,
            &mut list_state,
        );
    }
}
