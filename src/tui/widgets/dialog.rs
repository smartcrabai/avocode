use crate::tui::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Margin, Rect},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

pub enum DialogKind {
    Confirm {
        message: String,
        on_confirm: Box<dyn FnOnce() + Send>,
    },
    Input {
        prompt: String,
        value: String,
    },
    Info {
        message: String,
    },
}

pub struct DialogWidget<'a> {
    pub theme: &'a Theme,
    pub title: &'a str,
    pub message: &'a str,
    pub buttons: &'a [&'a str],
}

impl Widget for DialogWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title)
            .border_style(self.theme.primary_style());
        let inner = block.inner(area);
        block.render(area, buf);
        let buttons_str = self.buttons.join("  |  ");
        let content = format!("{}\n\n[{}]", self.message, buttons_str);
        Paragraph::new(content).alignment(Alignment::Center).render(
            inner.inner(Margin {
                vertical: 1,
                horizontal: 2,
            }),
            buf,
        );
    }
}
