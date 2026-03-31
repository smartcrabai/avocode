use crate::provider::models_dev::ModelChoice;
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
    /// Dynamically loaded model list (sorted, flat).
    pub models: Vec<ModelChoice>,
    /// Currently selected model in `"provider_id/model_id"` form.
    pub selected_model: Option<String>,
    /// Whether the model picker overlay is open.
    pub picker_open: bool,
    /// Cursor position inside the model picker list.
    pub picker_cursor: usize,
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
            models: vec![],
            selected_model: None,
            picker_open: false,
            picker_cursor: 0,
        }
    }

    /// Create an `App` pre-loaded with a dynamic model list and an optional preferred model.
    ///
    /// Initial selection policy:
    /// 1. Use `config_model` if it exists in `models`.
    /// 2. Otherwise fall back to the first model in the (already sorted) list.
    /// 3. If `models` is empty, `selected_model` stays `None`.
    #[must_use]
    pub fn with_models(models: Vec<ModelChoice>, config_model: Option<String>) -> Self {
        let selected_model = config_model
            .filter(|cfg| models.iter().any(|m| m.qualified_id() == *cfg))
            .or_else(|| models.first().map(ModelChoice::qualified_id));

        let mut app = Self::new();
        app.models = models;
        app.selected_model = selected_model;
        app
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.picker_open {
            match (key.code, key.modifiers) {
                (KeyCode::Down, _) => {
                    if !self.models.is_empty() {
                        self.picker_cursor = (self.picker_cursor + 1) % self.models.len();
                    }
                }
                (KeyCode::Up, _) => {
                    if !self.models.is_empty() {
                        self.picker_cursor = if self.picker_cursor == 0 {
                            self.models.len() - 1
                        } else {
                            self.picker_cursor - 1
                        };
                    }
                }
                (KeyCode::Enter, KeyModifiers::NONE) => {
                    if let Some(choice) = self.models.get(self.picker_cursor) {
                        self.selected_model = Some(choice.qualified_id());
                    }
                    self.picker_open = false;
                }
                (KeyCode::Esc, _) => {
                    self.picker_open = false;
                }
                _ => {}
            }
            return;
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => self.should_quit = true,
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => {
                self.show_sidebar = !self.show_sidebar;
            }
            (KeyCode::Char('t'), KeyModifiers::CONTROL) => self.cycle_theme(),
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => self.open_model_picker(),
            (KeyCode::Enter, KeyModifiers::NONE) if self.input.focused => self.submit_message(),
            (KeyCode::Backspace, KeyModifiers::NONE) => self.input.delete_char(),
            (KeyCode::Char(c), KeyModifiers::NONE) => self.input.insert_char(c),
            (KeyCode::PageUp, _) => self.chat.scroll_up(),
            (KeyCode::PageDown, _) => self.chat.scroll_down(),
            _ => {}
        }
    }

    fn open_model_picker(&mut self) {
        if self.models.is_empty() {
            return;
        }
        if let Some(ref selected) = self.selected_model
            && let Some(idx) = self
                .models
                .iter()
                .position(|m| m.qualified_id() == *selected)
        {
            self.picker_cursor = idx;
        }
        self.picker_open = true;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    fn make_choice(provider_id: &str, model_id: &str) -> ModelChoice {
        ModelChoice {
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            display_name: model_id.to_string(),
            context_length: None,
        }
    }

    fn two_models() -> Vec<ModelChoice> {
        vec![
            make_choice("anthropic", "claude-opus-4"),
            make_choice("openai", "gpt-4o"),
        ]
    }

    // ---- App::with_models initial state ----

    #[test]
    fn test_with_models_picker_closed_and_cursor_at_zero() {
        let app = App::with_models(two_models(), None);
        assert!(!app.picker_open);
        assert_eq!(app.picker_cursor, 0);
        assert_eq!(app.models.len(), 2);
    }

    #[test]
    fn test_with_models_config_model_in_list_is_selected() {
        let app = App::with_models(two_models(), Some("openai/gpt-4o".to_string()));
        assert_eq!(app.selected_model.as_deref(), Some("openai/gpt-4o"));
    }

    #[test]
    fn test_with_models_config_model_not_in_list_falls_back_to_first() {
        let app = App::with_models(two_models(), Some("unknown/model".to_string()));
        assert_eq!(
            app.selected_model.as_deref(),
            Some("anthropic/claude-opus-4")
        );
    }

    #[test]
    fn test_with_models_no_config_selects_first() {
        let app = App::with_models(two_models(), None);
        assert_eq!(
            app.selected_model.as_deref(),
            Some("anthropic/claude-opus-4")
        );
    }

    #[test]
    fn test_with_models_empty_list_no_selection() {
        let app = App::with_models(vec![], None);
        assert!(app.selected_model.is_none());
    }

    // ---- Ctrl+P opens picker ----

    #[test]
    fn test_ctrl_p_opens_model_picker() {
        let mut app = App::with_models(two_models(), None);
        assert!(!app.picker_open);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert!(app.picker_open);
    }

    #[test]
    fn test_ctrl_p_no_op_when_models_empty() {
        let mut app = App::with_models(vec![], None);
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert!(!app.picker_open);
    }

    #[test]
    fn test_ctrl_p_positions_cursor_on_current_selection() {
        let mut app = App::with_models(two_models(), Some("openai/gpt-4o".to_string()));
        app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert_eq!(app.picker_cursor, 1);
    }

    // ---- Picker navigation ----

    #[test]
    fn test_picker_down_advances_cursor() {
        let mut app = App::with_models(two_models(), None);
        app.picker_open = true;
        app.picker_cursor = 0;
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.picker_cursor, 1);
    }

    #[test]
    fn test_picker_down_wraps_at_end() {
        let mut app = App::with_models(two_models(), None);
        app.picker_open = true;
        app.picker_cursor = 1; // last index for two_models
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.picker_cursor, 0);
    }

    #[test]
    fn test_picker_up_decrements_cursor() {
        let mut app = App::with_models(two_models(), None);
        app.picker_open = true;
        app.picker_cursor = 1;
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.picker_cursor, 0);
    }

    #[test]
    fn test_picker_up_wraps_at_start() {
        let mut app = App::with_models(two_models(), None);
        app.picker_open = true;
        app.picker_cursor = 0;
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.picker_cursor, 1); // wraps to last index
    }

    // ---- Picker confirmation ----

    #[test]
    fn test_picker_enter_confirms_selection_and_closes_picker() {
        let mut app = App::with_models(two_models(), None);
        app.picker_open = true;
        app.picker_cursor = 1;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.selected_model.as_deref(), Some("openai/gpt-4o"));
        assert!(!app.picker_open);
    }

    #[test]
    fn test_picker_esc_closes_without_changing_selection() {
        let mut app = App::with_models(two_models(), None);
        let original = app.selected_model.clone();
        app.picker_open = true;
        app.picker_cursor = 1; // moved but not confirmed
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.picker_open);
        assert_eq!(app.selected_model, original);
    }

    // ---- Picker does not leak keys to normal handler ----

    #[test]
    fn test_picker_open_blocks_normal_key_handling() {
        let mut app = App::with_models(two_models(), None);
        app.picker_open = true;
        app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
        assert!(app.show_sidebar); // default is true, should remain true
    }
}
