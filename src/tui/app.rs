use crate::provider::models_dev::ModelChoice;
use crate::tui::{
    styles::Styles,
    widgets::{
        chat::{ChatMessage, ChatState, MessageRole},
        input::InputState,
    },
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Simplified TUI state.
///
/// Only holds the minimum data needed for a single chat session:
/// chat history, text input, model list, and quit flag.
pub struct App {
    pub chat: ChatState,
    pub input: InputState,
    pub styles: Styles,
    pub should_quit: bool,
    pub models: Vec<ModelChoice>,
    /// `"provider_id/model_id"` form.
    pub selected_model: Option<String>,
    /// Set by `submit_message`, consumed and cleared by the TUI run loop.
    pub pending_submit: Option<String>,
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
            styles: Styles::new(),
            should_quit: false,
            models: vec![],
            selected_model: None,
            pending_submit: None,
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

    /// Handle a terminal key event.
    ///
    /// Recognised keys:
    /// - **Ctrl+C**  – quit
    /// - **Enter**   – submit current input
    /// - **Backspace** – delete last char
    /// - **Char(c)** – insert character
    /// - **`PageUp` / `PageDown`** – scroll chat
    pub fn handle_key(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => self.should_quit = true,
            (KeyCode::Enter, KeyModifiers::NONE) if self.input.focused => self.submit_message(),
            (KeyCode::Backspace, KeyModifiers::NONE) => self.input.delete_char(),
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.input.insert_char(c);
            }
            (KeyCode::PageUp, KeyModifiers::NONE) => self.chat.scroll_up(),
            (KeyCode::PageDown, KeyModifiers::NONE) => self.chat.scroll_down(),
            _ => {}
        }
    }

    fn submit_message(&mut self) {
        let text = self.input.take_text();
        if !text.is_empty() {
            self.chat.push(ChatMessage {
                role: MessageRole::User,
                content: text.clone(),
            });
            self.pending_submit = Some(text);
        }
    }

    /// Drain and return the pending submit text, if any.
    pub fn take_pending_submit(&mut self) -> Option<String> {
        self.pending_submit.take()
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

    // ================================================================
    // App::new — default state
    // ================================================================

    #[test]
    fn test_new_app_not_quit() {
        let app = App::new();
        assert!(!app.should_quit);
    }

    #[test]
    fn test_new_app_has_no_selected_model() {
        let app = App::new();
        assert!(app.selected_model.is_none());
    }

    #[test]
    fn test_new_app_has_empty_models() {
        let app = App::new();
        assert!(app.models.is_empty());
    }

    #[test]
    fn test_new_app_has_empty_pending_submit() {
        let app = App::new();
        assert!(app.pending_submit.is_none());
    }

    #[test]
    fn test_new_app_has_default_styles() {
        let app = App::new();
        let default = Styles::new();
        assert_eq!(app.styles.foreground, default.foreground);
        assert_eq!(app.styles.accent, default.accent);
        assert_eq!(app.styles.muted, default.muted);
    }

    // ================================================================
    // App::with_models — model selection policy
    // ================================================================

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

    #[test]
    fn test_with_models_stores_model_list() {
        let app = App::with_models(two_models(), None);
        assert_eq!(app.models.len(), 2);
    }

    // ================================================================
    // Key handling — quit
    // ================================================================

    #[test]
    fn test_ctrl_c_sets_should_quit() {
        let mut app = App::new();
        assert!(!app.should_quit);
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    // ================================================================
    // Key handling — text input
    // ================================================================

    #[test]
    fn test_char_inserts_into_input() {
        let mut app = App::new();
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(app.input.text, "a");
    }

    #[test]
    fn test_multiple_chars_accumulate() {
        let mut app = App::new();
        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(app.input.text, "hi");
    }

    #[test]
    fn test_backspace_deletes_last_char() {
        let mut app = App::new();
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(app.input.text.is_empty());
    }

    // ================================================================
    // Key handling — message submission
    // ================================================================

    #[test]
    fn test_enter_submits_message_and_creates_chat_entry() {
        let mut app = App::new();
        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.chat.messages.len(), 1);
        assert_eq!(app.chat.messages[0].role, MessageRole::User);
        assert_eq!(app.chat.messages[0].content, "hi");
    }

    #[test]
    fn test_enter_sets_pending_submit() {
        let mut app = App::new();
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.pending_submit, Some("x".to_string()));
    }

    #[test]
    fn test_enter_on_empty_input_is_noop() {
        let mut app = App::new();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.chat.messages.is_empty());
        assert!(app.pending_submit.is_none());
    }

    // ================================================================
    // Key handling — chat scrolling
    // ================================================================

    #[test]
    fn test_page_up_scrolls_chat() {
        let mut app = App::new();
        app.chat.scroll_offset = 10;
        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert!(app.chat.scroll_offset < 10);
    }

    #[test]
    fn test_page_down_scrolls_chat() {
        let mut app = App::new();
        let before = app.chat.scroll_offset;
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert!(app.chat.scroll_offset > before);
    }

    // ================================================================
    // take_pending_submit
    // ================================================================

    #[test]
    fn test_take_pending_submit_drains_value() {
        let mut app = App::new();
        app.pending_submit = Some("hello".to_string());
        let taken = app.take_pending_submit();
        assert_eq!(taken, Some("hello".to_string()));
        assert!(app.pending_submit.is_none());
    }

    #[test]
    fn test_take_pending_submit_returns_none_when_empty() {
        let mut app = App::new();
        assert!(app.take_pending_submit().is_none());
    }

    #[test]
    fn test_take_pending_submit_twice_returns_none_second_time() {
        let mut app = App::new();
        app.pending_submit = Some("once".to_string());
        let _ = app.take_pending_submit();
        assert!(app.take_pending_submit().is_none());
    }
}
