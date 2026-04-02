use crate::tui::styles::Styles;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget},
};

pub struct InputState {
    pub text: String,
    pub cursor: usize,
    pub focused: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            focused: true,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn delete_char(&mut self) {
        if self.cursor > 0 {
            let prev = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            self.text.remove(prev);
            self.cursor = prev;
        }
    }

    pub fn take_text(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.text)
    }
}

pub struct InputWidget<'a> {
    pub styles: &'a Styles,
}

impl StatefulWidget for InputWidget<'_> {
    type State = InputState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let title = if state.focused {
            "Input (Enter to send)"
        } else {
            "Input"
        };
        let border_color = if state.focused {
            self.styles.accent
        } else {
            self.styles.muted
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(border_color));
        let inner = block.inner(area);
        block.render(area, buf);
        Paragraph::new(state.text.as_str()).render(inner, buf);
    }
}
