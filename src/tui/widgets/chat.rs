use crate::tui::styles::Styles;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget, Wrap},
};

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub scroll_offset: u16,
    pub streaming: Option<String>,
}

impl Default for ChatState {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            streaming: None,
        }
    }

    pub fn push(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(3);
    }
}

pub struct ChatWidget<'a> {
    pub styles: &'a Styles,
}

impl StatefulWidget for ChatWidget<'_> {
    type State = ChatState;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "total_lines fits in u16 for any reasonable terminal height"
    )]
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Chat")
            .border_style(Style::default().fg(self.styles.muted))
            .title_style(
                Style::default()
                    .fg(self.styles.accent)
                    .add_modifier(Modifier::BOLD),
            );

        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines: Vec<Line> = Vec::new();
        for msg in &state.messages {
            let (color, prefix) = match msg.role {
                MessageRole::User => (self.styles.foreground, "You"),
                MessageRole::Assistant => (self.styles.accent, "Assistant"),
                MessageRole::System => (self.styles.muted, "System"),
                MessageRole::Tool => (self.styles.muted, "Tool"),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{prefix}: "), Style::default().fg(color)),
                Span::raw(msg.content.as_str()),
            ]));
            lines.push(Line::from(""));
        }
        if let Some(streaming) = &state.streaming {
            lines.push(Line::from(vec![
                Span::styled("Assistant: ", Style::default().fg(self.styles.accent)),
                Span::raw(streaming.as_str()),
                Span::styled("|", Style::default().fg(self.styles.accent)),
            ]));
        }

        let text = Text::from(lines);
        let total_lines = text.height() as u16;
        let visible = inner.height;
        let max_offset = total_lines.saturating_sub(visible);
        let offset = state.scroll_offset.min(max_offset);

        Paragraph::new(text)
            .scroll((offset, 0))
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}
