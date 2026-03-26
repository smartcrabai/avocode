use crossterm::event::{KeyCode, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBind {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBind {
    #[must_use]
    pub fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
        }
    }
    #[must_use]
    pub fn key(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE,
        }
    }
}

pub struct KeyBindings {
    pub quit: KeyBind,
    pub send_message: KeyBind,
    pub new_session: KeyBind,
    pub toggle_sidebar: KeyBind,
    pub next_theme: KeyBind,
    pub scroll_up: KeyBind,
    pub scroll_down: KeyBind,
    pub focus_input: KeyBind,
    pub focus_chat: KeyBind,
    pub open_command: KeyBind,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            quit: KeyBind::ctrl('c'),
            send_message: KeyBind {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
            },
            new_session: KeyBind::ctrl('n'),
            toggle_sidebar: KeyBind::ctrl('b'),
            next_theme: KeyBind::ctrl('t'),
            scroll_up: KeyBind {
                code: KeyCode::PageUp,
                modifiers: KeyModifiers::NONE,
            },
            scroll_down: KeyBind {
                code: KeyCode::PageDown,
                modifiers: KeyModifiers::NONE,
            },
            focus_input: KeyBind::key('i'),
            focus_chat: KeyBind {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
            },
            open_command: KeyBind::ctrl('p'),
        }
    }
}
