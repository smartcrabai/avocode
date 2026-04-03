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
    /// Whether the model-picker popup is currently visible.
    pub picker_open: bool,
    /// Index into `models` that is currently highlighted in the picker.
    pub picker_highlight: usize,
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
            picker_open: false,
            picker_highlight: 0,
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

    /// Open the model-picker popup.
    ///
    /// Sets `picker_open` to `true`, positions the highlight at the currently
    /// selected model (or 0 when no model is selected), and removes input focus
    /// so that printable keys no longer flow into the text box.
    fn open_model_picker(&mut self) {
        self.picker_highlight = self
            .selected_model
            .as_ref()
            .and_then(|sel| self.models.iter().position(|m| m.qualified_id() == *sel))
            .unwrap_or(0);
        self.picker_open = true;
        self.input.focused = false;
    }

    /// Close the model-picker popup without changing `selected_model`.
    ///
    /// Restores input focus.
    fn close_model_picker(&mut self) {
        self.picker_open = false;
        self.input.focused = true;
    }

    /// Move the picker highlight one row up, wrapping from the first item to
    /// the last.
    fn move_model_picker_up(&mut self) {
        if !self.models.is_empty() {
            self.picker_highlight =
                (self.picker_highlight + self.models.len() - 1) % self.models.len();
        }
    }

    /// Move the picker highlight one row down, wrapping from the last item to
    /// the first.
    fn move_model_picker_down(&mut self) {
        if !self.models.is_empty() {
            self.picker_highlight = (self.picker_highlight + 1) % self.models.len();
        }
    }

    /// Apply the currently highlighted model as `selected_model` and close the
    /// picker.
    fn apply_model_picker_selection(&mut self) {
        if let Some(model) = self.models.get(self.picker_highlight) {
            self.selected_model = Some(model.qualified_id());
        }
        self.close_model_picker();
    }

    /// Handle a terminal key event.
    ///
    /// **Ctrl+C** always quits, regardless of picker state.
    ///
    /// When the model-picker is open:
    /// - **Esc**      - close picker without changing model
    /// - **Enter**    - apply highlighted selection
    /// - **Up / k**   - move highlight up (wraps)
    /// - **Down / j** - move highlight down (wraps)
    ///
    /// When the picker is closed:
    /// - **Ctrl+T**   - open model picker
    /// - **Enter**    - submit current input
    /// - **Backspace** - delete last char
    /// - **Char(c)**  - insert character
    /// - **`PageUp` / `PageDown`** - scroll chat
    pub fn handle_key(&mut self, key: KeyEvent) {
        if (key.code, key.modifiers) == (KeyCode::Char('c'), KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        if self.picker_open {
            match (key.code, key.modifiers) {
                (KeyCode::Esc, KeyModifiers::NONE) => self.close_model_picker(),
                (KeyCode::Enter, KeyModifiers::NONE) => self.apply_model_picker_selection(),
                (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
                    self.move_model_picker_up();
                }
                (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
                    self.move_model_picker_down();
                }
                _ => {}
            }
            return;
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('t'), KeyModifiers::CONTROL) => self.open_model_picker(),
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
    // App::new -- default state
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
    // App::with_models -- model selection policy
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
    // Key handling -- quit
    // ================================================================

    #[test]
    fn test_ctrl_c_sets_should_quit() {
        let mut app = App::new();
        assert!(!app.should_quit);
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    // ================================================================
    // Key handling -- text input
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
    // Key handling -- message submission
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
    // Key handling -- chat scrolling
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

    // ================================================================
    // Model picker -- default state
    // ================================================================

    #[test]
    fn test_new_app_picker_is_closed_by_default() {
        let app = App::new();
        assert!(!app.picker_open);
    }

    #[test]
    fn test_new_app_picker_highlight_is_zero_by_default() {
        let app = App::new();
        assert_eq!(app.picker_highlight, 0);
    }

    // ================================================================
    // Model picker -- opening via Ctrl+T
    // ================================================================

    #[test]
    fn test_ctrl_t_opens_model_picker() {
        // Given: app with some models loaded
        let mut app = App::with_models(two_models(), None);
        assert!(!app.picker_open);
        // When: Ctrl+T is pressed
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        // Then: picker is open
        assert!(app.picker_open);
    }

    #[test]
    fn test_open_picker_sets_highlight_to_current_model_index() {
        // Given: app with two models, second model (openai/gpt-4o) selected
        let mut app = App::with_models(two_models(), Some("openai/gpt-4o".to_string()));
        assert_eq!(app.selected_model.as_deref(), Some("openai/gpt-4o"));
        // When: picker is opened (second item in sorted list)
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        // Then: highlight is at index 1 (openai/gpt-4o is second after sort)
        assert_eq!(app.picker_highlight, 1);
    }

    #[test]
    fn test_open_picker_sets_highlight_to_zero_for_first_model() {
        // Given: app with first model selected
        let mut app = App::with_models(two_models(), None);
        // First model is anthropic/claude-opus-4 (index 0)
        assert_eq!(
            app.selected_model.as_deref(),
            Some("anthropic/claude-opus-4")
        );
        // When: picker is opened
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        // Then: highlight is at index 0
        assert_eq!(app.picker_highlight, 0);
    }

    #[test]
    fn test_open_picker_unfocuses_input() {
        // Given: app with input focused (default)
        let mut app = App::with_models(two_models(), None);
        assert!(app.input.focused);
        // When: picker is opened
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        // Then: input is no longer focused (prevents accidental text edits and sends)
        assert!(!app.input.focused);
    }

    // ================================================================
    // Model picker -- closing with Esc
    // ================================================================

    #[test]
    fn test_esc_closes_picker_without_changing_model() {
        // Given: picker is open, currently on openai/gpt-4o
        let mut app = App::with_models(two_models(), Some("openai/gpt-4o".to_string()));
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        let model_before = app.selected_model.clone();
        // When: navigate to a different item then press Esc
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        // Then: picker is closed and model is unchanged
        assert!(!app.picker_open);
        assert_eq!(app.selected_model, model_before);
    }

    #[test]
    fn test_esc_restores_input_focus() {
        // Given: picker is open (input is unfocused)
        let mut app = App::with_models(two_models(), None);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        assert!(!app.input.focused);
        // When: Esc closes the picker
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        // Then: input focus is restored
        assert!(app.input.focused);
    }

    // ================================================================
    // Model picker -- applying selection with Enter
    // ================================================================

    #[test]
    fn test_enter_in_picker_applies_highlighted_model() {
        // Given: app starts with first model; picker opens, Down navigates to gpt-4o
        let mut app = App::with_models(two_models(), None);
        // anthropic/claude-opus-4 is index 0, openai/gpt-4o is index 1
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        // When: Enter applies the selection
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: selected model is now openai/gpt-4o
        assert_eq!(app.selected_model.as_deref(), Some("openai/gpt-4o"));
    }

    #[test]
    fn test_enter_in_picker_closes_picker() {
        // Given: picker is open
        let mut app = App::with_models(two_models(), None);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        assert!(app.picker_open);
        // When: Enter applies selection
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: picker is closed
        assert!(!app.picker_open);
    }

    #[test]
    fn test_enter_in_picker_restores_input_focus() {
        // Given: picker is open (input unfocused)
        let mut app = App::with_models(two_models(), None);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        assert!(!app.input.focused);
        // When: Enter applies selection
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: input focus is restored
        assert!(app.input.focused);
    }

    #[test]
    fn test_enter_in_picker_does_not_set_pending_submit() {
        // Given: picker open with text already typed in input before opening
        let mut app = App::with_models(two_models(), None);
        // Type text first, then open picker
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        // When: Enter in picker (applies selection, must NOT submit the input text)
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: no message was submitted
        assert!(app.pending_submit.is_none());
    }

    // ================================================================
    // Model picker -- navigation
    // ================================================================

    #[test]
    fn test_down_key_moves_highlight_down() {
        // Given: picker open at index 0
        let mut app = App::with_models(two_models(), None);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        assert_eq!(app.picker_highlight, 0);
        // When: Down key pressed
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        // Then: highlight is at index 1
        assert_eq!(app.picker_highlight, 1);
    }

    #[test]
    fn test_up_key_moves_highlight_up() {
        // Given: picker open, navigate to index 1
        let mut app = App::with_models(two_models(), None);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.picker_highlight, 1);
        // When: Up key pressed
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        // Then: highlight is back at index 0
        assert_eq!(app.picker_highlight, 0);
    }

    #[test]
    fn test_j_key_moves_highlight_down() {
        // Given: picker open at index 0
        let mut app = App::with_models(two_models(), None);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        // When: j (vim-style down) pressed
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        // Then: highlight at index 1
        assert_eq!(app.picker_highlight, 1);
    }

    #[test]
    fn test_k_key_moves_highlight_up() {
        // Given: picker open, navigate to index 1
        let mut app = App::with_models(two_models(), None);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.picker_highlight, 1);
        // When: k (vim-style up) pressed
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        // Then: highlight back at index 0
        assert_eq!(app.picker_highlight, 0);
    }

    #[test]
    fn test_navigation_wraps_from_last_to_first() {
        // Given: picker open with 2 models, navigate to last item (index 1)
        let mut app = App::with_models(two_models(), None);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.picker_highlight, 1);
        // When: Down pressed past the last item
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        // Then: wraps to first item (index 0)
        assert_eq!(app.picker_highlight, 0);
    }

    #[test]
    fn test_navigation_wraps_from_first_to_last() {
        // Given: picker open at first item (index 0)
        let mut app = App::with_models(two_models(), None);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        assert_eq!(app.picker_highlight, 0);
        // When: Up pressed before the first item
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        // Then: wraps to last item (index 1 for two_models)
        assert_eq!(app.picker_highlight, 1);
    }

    // ================================================================
    // Model picker -- input protection while picker is open
    // ================================================================

    #[test]
    fn test_printable_char_does_not_go_to_input_while_picker_open() {
        // Given: picker is open
        let mut app = App::with_models(two_models(), None);
        let text_before = app.input.text.clone();
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        // When: printable character typed while picker is open (should navigate, not insert)
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        // Then: input text is unchanged (the 'a' is not a j/k/c so it falls through to _ => {})
        assert_eq!(app.input.text, text_before);
    }

    #[test]
    fn test_input_stays_unfocused_while_picker_open() {
        // Given: picker is open
        let mut app = App::with_models(two_models(), None);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        // When: multiple navigation keys pressed
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        // Then: input is still unfocused throughout
        assert!(!app.input.focused);
    }
}
