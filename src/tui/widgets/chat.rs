use crate::tui::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget, Wrap},
};

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: String,
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
        self.scroll_offset += 3;
    }
}

pub struct ChatWidget<'a> {
    pub theme: &'a Theme,
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
            .border_style(self.theme.border_style())
            .title_style(self.theme.title_style());

        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines: Vec<Line> = Vec::new();
        for msg in &state.messages {
            let (color, prefix) = match msg.role {
                MessageRole::User => (self.theme.user_msg, "You"),
                MessageRole::Assistant => (self.theme.assistant_msg, "Assistant"),
                MessageRole::System => (self.theme.muted, "System"),
                MessageRole::Tool => (self.theme.accent, "Tool"),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{prefix}: "), Style::default().fg(color)),
                Span::raw(msg.content.clone()),
            ]));
            lines.push(Line::from(""));
        }
        if let Some(streaming) = &state.streaming {
            lines.push(Line::from(vec![
                Span::styled("Assistant: ", Style::default().fg(self.theme.assistant_msg)),
                Span::raw(streaming.clone()),
                Span::styled("|", Style::default().fg(self.theme.accent)),
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
