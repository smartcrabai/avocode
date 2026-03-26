use crate::tui::{
    keybinds::KeyBindings,
    theme::{self, Theme},
    widgets::{
        chat::{ChatMessage, ChatState, MessageRole},
        input::InputState,
        sidebar::SidebarState,
    },
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub struct App {
    pub chat: ChatState,
    pub input: InputState,
    pub sidebar: SidebarState,
    pub theme: Theme,
    pub keybinds: KeyBindings,
    pub show_sidebar: bool,
    pub should_quit: bool,
    pub theme_index: usize,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    #[must_use]
    pub fn new() -> Self {
        Self {
            chat: ChatState::new(),
            input: InputState::new(),
            sidebar: SidebarState::new(),
            theme: theme::default_theme(),
            keybinds: KeyBindings::default(),
            show_sidebar: true,
            should_quit: false,
            theme_index: 0,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => self.should_quit = true,
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => {
                self.show_sidebar = !self.show_sidebar;
            }
            (KeyCode::Char('t'), KeyModifiers::CONTROL) => self.cycle_theme(),
            (KeyCode::Enter, KeyModifiers::NONE) if self.input.focused => self.submit_message(),
            (KeyCode::Backspace, KeyModifiers::NONE) => self.input.delete_char(),
            (KeyCode::Char(c), KeyModifiers::NONE) => self.input.insert_char(c),
            (KeyCode::PageUp, _) => self.chat.scroll_up(),
            (KeyCode::PageDown, _) => self.chat.scroll_down(),
            _ => {}
        }
    }

    fn submit_message(&mut self) {
        let text = self.input.take_text();
        if !text.is_empty() {
            self.chat.push(ChatMessage {
                role: MessageRole::User,
                content: text,
                timestamp: String::new(),
            });
        }
    }

    pub fn cycle_theme(&mut self) {
        const THEME_COUNT: usize = 5;
        self.theme_index = (self.theme_index + 1) % THEME_COUNT;
        let constructors: [fn() -> Theme; THEME_COUNT] = [
            theme::default_theme,
            theme::dracula_theme,
            theme::nord_theme,
            theme::gruvbox_theme,
            theme::tokyo_night_theme,
        ];
        self.theme = constructors[self.theme_index]();
    }
}
