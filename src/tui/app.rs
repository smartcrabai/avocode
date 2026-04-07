use crate::provider::models_dev::ModelChoice;
use crate::skill::SkillInfo;
use crate::tui::{
    styles::Styles,
    widgets::{
        chat::{ChatMessage, ChatState, MessageRole},
        input::InputState,
    },
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// ---------------------------------------------------------------------------
// Built-in slash command types
// ---------------------------------------------------------------------------

/// Action performed when a TUI built-in slash command is executed.
enum BuiltinSlashAction {
    /// Open the model-picker popup (same as `Ctrl+T`).
    OpenModelPicker,
    /// Archive the current session and start a fresh one.
    ResetSession,
}

/// A single row in the slash-completion popup.
#[derive(Debug, Clone)]
pub struct SlashEntry {
    pub name: String,
    pub description: String,
}

struct BuiltinCommand {
    name: &'static str,
    description: &'static str,
    action: BuiltinSlashAction,
}

/// Built-in slash commands in fixed display order.
const BUILTIN_ENTRIES: &[BuiltinCommand] = &[
    BuiltinCommand {
        name: "model",
        description: "Open the model picker",
        action: BuiltinSlashAction::OpenModelPicker,
    },
    BuiltinCommand {
        name: "clear",
        description: "Clear conversation and start a new session",
        action: BuiltinSlashAction::ResetSession,
    },
    BuiltinCommand {
        name: "new",
        description: "Start a new session (alias for /clear)",
        action: BuiltinSlashAction::ResetSession,
    },
];

/// Which popup is currently visible in the TUI.
///
/// Exactly one popup can be open at a time; `None` means no popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    /// The model-picker overlay (opened via Ctrl+T or `/model`).
    ModelPicker,
    /// The slash-completion dropdown.
    SlashCompletion,
}

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
    /// Which popup is currently visible (`None` when no popup is open).
    pub active_popup: Option<PopupKind>,
    /// Index into `models` that is currently highlighted in the picker.
    pub picker_highlight: usize,
    /// Discovered skills for the current project (loaded once at startup).
    pub skills: Vec<SkillInfo>,
    /// Index into the filtered skill list that is highlighted.
    pub slash_highlight: usize,
    /// Set when the user submits a built-in reset command (`/clear` or `/new`);
    /// consumed and cleared by the TUI run loop to archive + recreate the session.
    pub pending_reset: bool,
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
            active_popup: None,
            picker_highlight: 0,
            skills: vec![],
            slash_highlight: 0,
            pending_reset: false,
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
    /// Sets `active_popup` to `Some(PopupKind::ModelPicker)`, positions the
    /// highlight at the currently selected model (or 0 when no model is
    /// selected), and removes input focus so that printable keys no longer
    /// flow into the text box.
    fn open_model_picker(&mut self) {
        self.picker_highlight = self
            .selected_model
            .as_ref()
            .and_then(|sel| self.models.iter().position(|m| m.qualified_id() == *sel))
            .unwrap_or(0);
        self.active_popup = Some(PopupKind::ModelPicker);
        self.input.focused = false;
    }

    /// Close the model-picker popup without changing `selected_model`.
    ///
    /// Restores input focus.
    fn close_model_picker(&mut self) {
        self.active_popup = None;
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
    /// When the slash-completion popup is open:
    /// - **Esc**      - close popup
    /// - **Enter / Tab** - apply highlighted skill
    /// - **Up / k**   - move highlight up (wraps)
    /// - **Down / j** - move highlight down (wraps)
    /// - **Backspace** - delete char (close if input becomes empty)
    /// - **Whitespace** - insert and close
    /// - **Char(c)**  - insert and keep open if still a slash-token
    ///
    /// When neither popup is open:
    /// - **Ctrl+T**   - open model picker
    /// - **Enter**    - submit current input
    /// - **Backspace** - delete last char
    /// - **Char(c)**  - insert character (`/` at start opens slash completion)
    /// - **`PageUp` / `PageDown`** - scroll chat
    pub fn handle_key(&mut self, key: KeyEvent) {
        if (key.code, key.modifiers) == (KeyCode::Char('c'), KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        if matches!(self.active_popup, Some(PopupKind::ModelPicker)) {
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
        if (key.code, key.modifiers) == (KeyCode::Char('t'), KeyModifiers::CONTROL) {
            self.open_model_picker();
            return;
        }
        if matches!(self.active_popup, Some(PopupKind::SlashCompletion)) {
            self.handle_slash_key(key);
            return;
        }
        match (key.code, key.modifiers) {
            (KeyCode::Enter, KeyModifiers::NONE) if self.input.focused => {
                self.submit_or_dispatch();
            }
            (KeyCode::Backspace, KeyModifiers::NONE) => self.input.delete_char(),
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.input.insert_char(c);
                if c == '/' && self.input.text == "/" {
                    self.active_popup = Some(PopupKind::SlashCompletion);
                    self.slash_highlight = 0;
                }
            }
            (KeyCode::PageUp, KeyModifiers::NONE) => self.chat.scroll_up(),
            (KeyCode::PageDown, KeyModifiers::NONE) => self.chat.scroll_down(),
            _ => {}
        }
    }

    /// Handle a key event while the slash-completion popup is open.
    fn handle_slash_key(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, KeyModifiers::NONE) => {
                self.active_popup = None;
            }
            (KeyCode::Enter | KeyCode::Tab, KeyModifiers::NONE) => {
                self.apply_slash_selection();
            }
            (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
                self.move_slash_up();
            }
            (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
                self.move_slash_down();
            }
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                self.input.delete_char();
                if self.input.text.is_empty() {
                    self.active_popup = None;
                }
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.input.insert_char(c);
                if c.is_whitespace() {
                    self.active_popup = None;
                }
            }
            _ => {}
        }
    }

    /// Move the slash-completion highlight one row up, wrapping.
    fn move_slash_up(&mut self) {
        let len = self.slash_filtered_entries().len();
        if len > 0 {
            self.slash_highlight = (self.slash_highlight + len - 1) % len;
        }
    }

    /// Move the slash-completion highlight one row down, wrapping.
    fn move_slash_down(&mut self) {
        let len = self.slash_filtered_entries().len();
        if len > 0 {
            self.slash_highlight = (self.slash_highlight + 1) % len;
        }
    }

    /// Apply the currently highlighted slash-completion entry (builtin or skill).
    ///
    /// Fills the input with `"/name "` and closes the popup.  The user must
    /// press Enter a second time to execute a built-in command.
    fn apply_slash_selection(&mut self) {
        let filtered = self.slash_filtered_entries();
        let idx = self.slash_highlight.min(filtered.len().saturating_sub(1));
        if let Some(entry) = filtered.get(idx) {
            self.input.text = format!("/{} ", entry.name);
            self.input.cursor = self.input.text.len();
        }
        self.active_popup = None;
    }

    /// Dispatch Enter based on the current input content.
    ///
    /// Matches the trimmed input against `BUILTIN_ENTRIES`; if a match is found
    /// the corresponding action is executed.  Otherwise the text is forwarded
    /// to the processor via `submit_message`.
    fn submit_or_dispatch(&mut self) {
        let trimmed = self.input.text.trim();
        if let Some(cmd) = BUILTIN_ENTRIES.iter().find(|cmd| {
            trimmed
                .strip_prefix('/')
                .is_some_and(|s| s.eq_ignore_ascii_case(cmd.name))
        }) {
            self.input.take_text();
            match cmd.action {
                BuiltinSlashAction::OpenModelPicker => self.open_model_picker(),
                BuiltinSlashAction::ResetSession => self.pending_reset = true,
            }
        } else {
            self.submit_message();
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

    /// Drain and return whether a session reset was requested by `/clear` or `/new`.
    pub fn take_pending_reset(&mut self) -> bool {
        std::mem::take(&mut self.pending_reset)
    }

    /// Reset the UI to its initial state while preserving the selected model and
    /// discovered skills.
    ///
    /// Clears: chat history, streaming buffer, input text, both popups, and any
    /// pending submit or reset actions.  Does **not** archive or create sessions
    /// — the run loop is responsible for that.
    pub fn reset_ui(&mut self) {
        let models = std::mem::take(&mut self.models);
        let selected_model = self.selected_model.take();
        let skills = std::mem::take(&mut self.skills);
        *self = Self::new();
        self.models = models;
        self.selected_model = selected_model;
        self.skills = skills;
    }

    /// Return slash-completion entries matching the current input prefix.
    ///
    /// Built-in commands (`model`, `clear`, `new`) always appear first in fixed
    /// order, followed by project skills filtered by prefix (case-insensitive).
    #[must_use]
    pub(super) fn slash_filtered_entries(&self) -> Vec<SlashEntry> {
        let filter = self
            .input
            .text
            .strip_prefix('/')
            .unwrap_or("")
            .to_ascii_lowercase();

        BUILTIN_ENTRIES
            .iter()
            .filter(|cmd| cmd.name.starts_with(&*filter))
            .map(|cmd| SlashEntry {
                name: cmd.name.to_string(),
                description: cmd.description.to_string(),
            })
            .chain(
                self.skills
                    .iter()
                    .filter(|s| s.name.to_ascii_lowercase().starts_with(&filter))
                    .map(|s| SlashEntry {
                        name: s.name.clone(),
                        description: s.description.clone(),
                    }),
            )
            .collect()
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
        assert!(!matches!(app.active_popup, Some(PopupKind::ModelPicker)));
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
        assert!(!matches!(app.active_popup, Some(PopupKind::ModelPicker)));
        // When: Ctrl+T is pressed
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        // Then: picker is open
        assert!(matches!(app.active_popup, Some(PopupKind::ModelPicker)));
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
        assert!(!matches!(app.active_popup, Some(PopupKind::ModelPicker)));
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
        assert!(matches!(app.active_popup, Some(PopupKind::ModelPicker)));
        // When: Enter applies selection
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: picker is closed
        assert!(!matches!(app.active_popup, Some(PopupKind::ModelPicker)));
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

    // ================================================================
    // Slash completion -- helpers
    // ================================================================

    fn make_skill(name: &str, description: &str) -> SkillInfo {
        SkillInfo {
            name: name.to_string(),
            description: description.to_string(),
            content: format!("{name} content"),
            location: std::path::PathBuf::from(format!("/fake/{name}/SKILL.md")),
        }
    }

    fn app_with_skills() -> App {
        let mut app = App::new();
        app.skills = vec![
            make_skill("commit", "Create a git commit"),
            make_skill("review", "Review code changes"),
            make_skill("refactor", "Refactor code"),
        ];
        app
    }

    // ================================================================
    // Slash completion -- default state
    // ================================================================

    #[test]
    fn test_new_app_slash_is_closed_by_default() {
        let app = App::new();
        assert!(!matches!(
            app.active_popup,
            Some(PopupKind::SlashCompletion)
        ));
    }

    #[test]
    fn test_new_app_slash_highlight_is_zero_by_default() {
        let app = App::new();
        assert_eq!(app.slash_highlight, 0);
    }

    #[test]
    fn test_new_app_has_empty_skills() {
        let app = App::new();
        assert!(app.skills.is_empty());
    }

    // ================================================================
    // Slash completion -- opening with `/` at start of input
    // ================================================================

    #[test]
    fn test_slash_at_start_opens_completion_when_skills_exist() {
        // Given: app with skills loaded, empty input
        let mut app = app_with_skills();
        assert!(app.input.text.is_empty());
        // When: `/` is typed
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        // Then: slash completion popup opens
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        assert_eq!(app.input.text, "/");
    }

    #[test]
    fn test_slash_opens_even_without_skills_due_to_builtins() {
        // Given: app with no skills (builtins are always available)
        let mut app = App::new();
        assert!(app.skills.is_empty());
        // When: `/` is typed
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        // Then: slash completion opens because built-in commands are always present
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        assert_eq!(app.input.text, "/");
    }

    #[test]
    fn test_slash_does_not_open_when_input_not_empty() {
        // Given: app with skills and some existing text
        let mut app = app_with_skills();
        app.input.insert_char('h');
        // When: `/` is typed
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        // Then: slash completion does NOT open (slash not at start)
        assert!(!matches!(
            app.active_popup,
            Some(PopupKind::SlashCompletion)
        ));
        assert_eq!(app.input.text, "h/");
    }

    // ================================================================
    // Slash completion -- filtering while typing
    // ================================================================

    #[test]
    fn test_typing_after_slash_filters_candidates() {
        // Given: slash completion is open with 3 skills
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        // When: typing "re" (matches "review" and "refactor")
        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        // Then: popup stays open, text is "/re"
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        assert_eq!(app.input.text, "/re");
    }

    #[test]
    fn test_typing_narrows_to_single_match() {
        // Given: slash completion is open
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        // When: typing "com" (matches only "commit")
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        // Then: popup stays open
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        assert_eq!(app.input.text, "/com");
    }

    // ================================================================
    // Slash completion -- closing with Esc
    // ================================================================

    #[test]
    fn test_esc_closes_slash_completion() {
        // Given: slash completion is open
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        // When: Esc is pressed
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        // Then: popup closes, text stays as "/"
        assert!(!matches!(
            app.active_popup,
            Some(PopupKind::SlashCompletion)
        ));
        assert_eq!(app.input.text, "/");
    }

    // ================================================================
    // Slash completion -- closing with whitespace
    // ================================================================

    #[test]
    fn test_space_closes_slash_completion() {
        // Given: slash completion is open
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        // When: space is typed
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        // Then: popup closes, space is inserted
        assert!(!matches!(
            app.active_popup,
            Some(PopupKind::SlashCompletion)
        ));
        assert_eq!(app.input.text, "/ ");
    }

    // ================================================================
    // Slash completion -- applying with Enter
    // ================================================================

    #[test]
    fn test_enter_applies_selected_skill() {
        // Given: slash completion is open; builtins (model=0, clear=1, new=2) come first,
        // then skills (commit=3, review=4, refactor=5). Navigate to "commit" at index 3.
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // → 1
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // → 2
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // → 3 (commit)
        // When: Enter is pressed
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: input text becomes "/commit ", popup closes
        assert!(!matches!(
            app.active_popup,
            Some(PopupKind::SlashCompletion)
        ));
        assert_eq!(app.input.text, "/commit ");
    }

    #[test]
    fn test_enter_does_not_submit_message_when_slash_open() {
        // Given: slash completion is open
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        // When: Enter is pressed
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: no message submitted (input was just a slash prefix)
        assert!(app.pending_submit.is_none());
    }

    #[test]
    fn test_enter_applies_navigated_skill() {
        // Given: slash completion open; with 3 builtins + 3 skills (6 total), navigate to
        // "review" at index 4 (model=0, clear=1, new=2, commit=3, review=4).
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // → 1
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // → 2
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // → 3
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // → 4 (review)
        // When: Enter applies selection
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: input text becomes "/review "
        assert!(!matches!(
            app.active_popup,
            Some(PopupKind::SlashCompletion)
        ));
        assert_eq!(app.input.text, "/review ");
    }

    // ================================================================
    // Slash completion -- applying with Tab
    // ================================================================

    #[test]
    fn test_tab_applies_selected_skill() {
        // Given: slash completion is open; navigate to "commit" at index 3
        // (model=0, clear=1, new=2, commit=3).
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // → 1
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // → 2
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // → 3 (commit)
        // When: Tab is pressed
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        // Then: input text becomes "/commit ", popup closes
        assert!(!matches!(
            app.active_popup,
            Some(PopupKind::SlashCompletion)
        ));
        assert_eq!(app.input.text, "/commit ");
    }

    // ================================================================
    // Slash completion -- navigation
    // ================================================================

    #[test]
    fn test_down_moves_slash_highlight() {
        // Given: slash completion open at index 0
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(app.slash_highlight, 0);
        // When: Down pressed
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        // Then: highlight moves to index 1
        assert_eq!(app.slash_highlight, 1);
    }

    #[test]
    fn test_up_moves_slash_highlight() {
        // Given: slash completion open, navigate to index 1
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.slash_highlight, 1);
        // When: Up pressed
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        // Then: highlight moves back to 0
        assert_eq!(app.slash_highlight, 0);
    }

    #[test]
    fn test_j_k_navigate_slash_completion() {
        // Given: slash completion open
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        // When: j pressed (vim-style down)
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        // Then: highlight at 1
        assert_eq!(app.slash_highlight, 1);
        // When: k pressed (vim-style up)
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        // Then: highlight back to 0
        assert_eq!(app.slash_highlight, 0);
    }

    #[test]
    fn test_slash_completion_wraps_from_last_to_first() {
        // Given: 3 builtins + 3 skills = 6 entries total (indices 0-5).
        // Navigate to last item (index 5).
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        for _ in 0..5 {
            app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert_eq!(app.slash_highlight, 5);
        // When: Down pressed past last
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        // Then: wraps to first
        assert_eq!(app.slash_highlight, 0);
    }

    #[test]
    fn test_slash_completion_wraps_from_first_to_last() {
        // Given: 3 builtins + 3 skills = 6 entries total (indices 0-5).
        // Start at first item (index 0).
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(app.slash_highlight, 0);
        // When: Up pressed before first
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        // Then: wraps to last (index 5)
        assert_eq!(app.slash_highlight, 5);
    }

    // ================================================================
    // Slash completion -- mutual exclusion with model picker
    // ================================================================

    #[test]
    fn test_opening_picker_closes_slash_completion() {
        // Given: slash completion is open
        let mut app = App::with_models(two_models(), None);
        app.skills = app_with_skills().skills;
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        // When: Ctrl+T opens model picker
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
        // Then: picker is open (active_popup is a single-value enum, so SlashCompletion is implicitly closed)
        assert!(matches!(app.active_popup, Some(PopupKind::ModelPicker)));
    }

    #[test]
    fn test_ctrl_c_quit_still_works_with_slash_open() {
        // Given: slash completion is open
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        // When: Ctrl+C pressed
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        // Then: quit flag set (regardless of popup state)
        assert!(app.should_quit);
    }

    // ================================================================
    // Slash completion -- backspace behavior
    // ================================================================

    #[test]
    fn test_backspace_on_slash_only_closes_popup() {
        // Given: slash completion open with just "/"
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        // When: Backspace removes the "/"
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        // Then: popup closes, input is empty
        assert!(!matches!(
            app.active_popup,
            Some(PopupKind::SlashCompletion)
        ));
        assert!(app.input.text.is_empty());
    }

    #[test]
    fn test_backspace_on_partial_filter_keeps_popup_open() {
        // Given: slash completion open with "/co"
        let mut app = app_with_skills();
        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        // When: Backspace removes "o"
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        // Then: popup stays open, text is "/c"
        assert!(matches!(app.active_popup, Some(PopupKind::SlashCompletion)));
        assert_eq!(app.input.text, "/c");
    }

    // ================================================================
    // Built-in slash entries -- slash_filtered_entries()
    // ================================================================

    #[test]
    fn test_slash_filtered_entries_builtins_appear_before_skills() {
        const SKILL_NAMES: &[&str] = &["commit", "review", "refactor"];
        // Given: app with skills loaded
        let app = app_with_skills();
        // When: entries requested
        let entries = app.slash_filtered_entries();
        // Then: all builtin entries (model/clear/new) appear before any skill entry
        let first_skill_pos = entries
            .iter()
            .position(|e| SKILL_NAMES.contains(&e.name.as_str()));
        let last_builtin_pos = entries
            .iter()
            .rposition(|e| BUILTIN_ENTRIES.iter().any(|b| b.name == e.name.as_str()));
        if let (Some(skill_pos), Some(builtin_pos)) = (first_skill_pos, last_builtin_pos) {
            assert!(
                builtin_pos < skill_pos,
                "expected all builtins before skills, but last builtin at {builtin_pos} >= first skill at {skill_pos}"
            );
        }
        // If no skills in entries, builtins-only is valid.
    }

    #[test]
    fn test_slash_filtered_entries_returns_builtins_when_no_skills() {
        // Given: app with no skills
        let app = App::new();
        assert!(app.skills.is_empty());
        // When: entries requested
        let entries = app.slash_filtered_entries();
        // Then: all three built-in entries (model, clear, new) are present
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"model"),
            "expected 'model' builtin, got: {names:?}"
        );
        assert!(
            names.contains(&"clear"),
            "expected 'clear' builtin, got: {names:?}"
        );
        assert!(
            names.contains(&"new"),
            "expected 'new' builtin, got: {names:?}"
        );
    }

    #[test]
    fn test_slash_filtered_entries_filters_by_prefix() {
        // Given: app with a "commit" skill and prefix "/mo" (matches only model builtin)
        let mut app = app_with_skills();
        app.input.text = "/mo".to_string();
        // When: entries requested
        let entries = app.slash_filtered_entries();
        // Then: only entries whose name starts with "mo" are included
        assert!(entries.iter().all(|e| e.name.starts_with("mo")));
        assert!(entries.iter().any(|e| e.name == "model"));
        assert!(!entries.iter().any(|e| e.name == "clear"));
    }

    #[test]
    fn test_slash_filtered_entries_empty_prefix_includes_all_builtins_and_skills() {
        // Given: app with skills, input is "/"
        let mut app = app_with_skills();
        app.input.text = "/".to_string();
        // When: entries requested
        let entries = app.slash_filtered_entries();
        // Then: all builtins AND all skills appear
        assert!(entries.iter().any(|e| e.name == "model"));
        assert!(entries.iter().any(|e| e.name == "clear"));
        assert!(entries.iter().any(|e| e.name == "new"));
        assert!(entries.iter().any(|e| e.name == "commit"));
        assert!(entries.iter().any(|e| e.name == "review"));
    }

    // ================================================================
    // Built-in submit path -- /model opens model picker
    // ================================================================

    #[test]
    fn test_submit_slash_model_exact_opens_model_picker() {
        // Given: app with models loaded, input is "/model"
        let mut app = App::with_models(two_models(), None);
        app.input.text = "/model".to_string();
        app.input.cursor = 6;
        // When: Enter is pressed
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: model picker is open
        assert!(
            matches!(app.active_popup, Some(PopupKind::ModelPicker)),
            "expected model picker to open after /model submit"
        );
    }

    #[test]
    fn test_submit_slash_model_does_not_set_pending_submit() {
        // Given: app with models loaded, input is "/model"
        let mut app = App::with_models(two_models(), None);
        app.input.text = "/model".to_string();
        app.input.cursor = 6;
        // When: Enter is pressed
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: no pending_submit (the command was handled as builtin, not forwarded to processor)
        assert!(
            app.pending_submit.is_none(),
            "expected pending_submit to be None after /model builtin"
        );
    }

    #[test]
    fn test_submit_slash_model_clears_input() {
        // Given: input is "/model"
        let mut app = App::with_models(two_models(), None);
        app.input.text = "/model".to_string();
        app.input.cursor = 6;
        // When: Enter submits the builtin
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: input text is cleared
        assert!(
            app.input.text.is_empty(),
            "expected input to be cleared after /model builtin"
        );
    }

    #[test]
    fn test_submit_slash_model_with_trailing_space_opens_picker() {
        // Given: input is "/model " (trailing space — produced by slash popup apply)
        let mut app = App::with_models(two_models(), None);
        app.input.text = "/model ".to_string();
        app.input.cursor = 7;
        // When: Enter submits
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: picker opens (trailing whitespace is trimmed before builtin check)
        assert!(
            matches!(app.active_popup, Some(PopupKind::ModelPicker)),
            "expected picker to open for '/model ' (trailing space)"
        );
    }

    // ================================================================
    // Built-in submit path -- /clear and /new set pending_reset
    // ================================================================

    #[test]
    fn test_submit_slash_clear_sets_pending_reset() {
        // Given: input is "/clear"
        let mut app = App::new();
        app.input.text = "/clear".to_string();
        app.input.cursor = 6;
        // When: Enter submits the builtin
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: pending_reset is set
        assert!(
            app.pending_reset,
            "expected pending_reset after /clear submit"
        );
    }

    #[test]
    fn test_submit_slash_new_sets_pending_reset() {
        // Given: input is "/new"
        let mut app = App::new();
        app.input.text = "/new".to_string();
        app.input.cursor = 4;
        // When: Enter submits the builtin
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: pending_reset is set
        assert!(
            app.pending_reset,
            "expected pending_reset after /new submit"
        );
    }

    #[test]
    fn test_submit_slash_clear_does_not_set_pending_submit() {
        // Given: input is "/clear"
        let mut app = App::new();
        app.input.text = "/clear".to_string();
        app.input.cursor = 6;
        // When: Enter submits
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: NOT forwarded to processor
        assert!(
            app.pending_submit.is_none(),
            "expected pending_submit to be None after /clear builtin"
        );
    }

    #[test]
    fn test_submit_slash_new_does_not_set_pending_submit() {
        // Given: input is "/new"
        let mut app = App::new();
        app.input.text = "/new".to_string();
        app.input.cursor = 4;
        // When: Enter submits
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: NOT forwarded to processor
        assert!(
            app.pending_submit.is_none(),
            "expected pending_submit to be None after /new builtin"
        );
    }

    #[test]
    fn test_submit_slash_clear_clears_input() {
        // Given: input is "/clear"
        let mut app = App::new();
        app.input.text = "/clear".to_string();
        app.input.cursor = 6;
        // When: Enter submits the builtin
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: input is cleared
        assert!(
            app.input.text.is_empty(),
            "expected input cleared after /clear"
        );
    }

    #[test]
    fn test_submit_slash_new_with_trailing_space_sets_pending_reset() {
        // Given: input is "/new " (trailing space from popup apply)
        let mut app = App::new();
        app.input.text = "/new ".to_string();
        app.input.cursor = 5;
        // When: Enter submits
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: pending_reset set (trailing whitespace trimmed before builtin check)
        assert!(
            app.pending_reset,
            "expected pending_reset for '/new ' (trailing space)"
        );
    }

    #[test]
    fn test_submit_slash_model_with_extra_argument_is_regular_message() {
        // Given: input is "/model gpt-4o" (has an extra argument)
        let mut app = App::with_models(two_models(), None);
        app.input.text = "/model gpt-4o".to_string();
        app.input.cursor = 13;
        // When: Enter submits
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Then: treated as a regular message (builtin detection is exact-match only)
        assert!(
            !matches!(app.active_popup, Some(PopupKind::ModelPicker)),
            "expected picker NOT to open for '/model gpt-4o' (extra arg)"
        );
        assert!(
            app.pending_submit.is_some(),
            "expected /model with extra arg to be forwarded as regular message"
        );
    }

    // ================================================================
    // take_pending_reset
    // ================================================================

    #[test]
    fn test_take_pending_reset_returns_false_initially() {
        // Given: fresh app
        let mut app = App::new();
        // When: take_pending_reset called
        // Then: returns false (no reset requested)
        assert!(!app.take_pending_reset());
    }

    #[test]
    fn test_take_pending_reset_drains_true_when_set() {
        // Given: pending_reset is true
        let mut app = App::new();
        app.pending_reset = true;
        // When: take_pending_reset called
        let taken = app.take_pending_reset();
        // Then: returns true and clears the flag
        assert!(taken);
        assert!(!app.pending_reset);
    }

    #[test]
    fn test_take_pending_reset_twice_returns_false_second_time() {
        // Given: pending_reset was set
        let mut app = App::new();
        app.pending_reset = true;
        // When: take_pending_reset called twice
        let first = app.take_pending_reset();
        let second = app.take_pending_reset();
        // Then: first call returns true, second returns false (drained)
        assert!(first);
        assert!(!second);
    }

    // ================================================================
    // reset_ui -- state cleared
    // ================================================================

    #[test]
    fn test_reset_ui_clears_chat_messages() {
        // Given: app has chat history
        let mut app = App::new();
        app.chat.push(ChatMessage {
            role: MessageRole::User,
            content: "hello".to_string(),
        });
        assert!(!app.chat.messages.is_empty());
        // When: reset_ui is called
        app.reset_ui();
        // Then: chat history is gone
        assert!(app.chat.messages.is_empty());
    }

    #[test]
    fn test_reset_ui_clears_streaming_buffer() {
        // Given: app has a partial streaming response
        let mut app = App::new();
        app.chat.streaming = Some("partial response…".to_string());
        // When: reset_ui
        app.reset_ui();
        // Then: streaming buffer cleared
        assert!(app.chat.streaming.is_none());
    }

    #[test]
    fn test_reset_ui_clears_input_text() {
        // Given: app has text in the input field
        let mut app = App::new();
        app.input.text = "/clear".to_string();
        app.input.cursor = 6;
        // When: reset_ui
        app.reset_ui();
        // Then: input is empty
        assert!(app.input.text.is_empty());
        assert_eq!(app.input.cursor, 0);
    }

    #[test]
    fn test_reset_ui_closes_slash_popup() {
        // Given: slash popup is open
        let mut app = app_with_skills();
        app.active_popup = Some(PopupKind::SlashCompletion);
        app.slash_highlight = 2;
        // When: reset_ui
        app.reset_ui();
        // Then: slash popup closed and highlight reset
        assert!(!matches!(
            app.active_popup,
            Some(PopupKind::SlashCompletion)
        ));
        assert_eq!(app.slash_highlight, 0);
    }

    #[test]
    fn test_reset_ui_closes_model_picker() {
        // Given: model picker is open
        let mut app = App::with_models(two_models(), None);
        app.active_popup = Some(PopupKind::ModelPicker);
        app.picker_highlight = 1;
        app.input.focused = false;
        // When: reset_ui
        app.reset_ui();
        // Then: picker closed and highlight reset
        assert!(!matches!(app.active_popup, Some(PopupKind::ModelPicker)));
        assert_eq!(app.picker_highlight, 0);
    }

    #[test]
    fn test_reset_ui_restores_input_focus() {
        // Given: input is unfocused (e.g. picker was open)
        let mut app = App::new();
        app.input.focused = false;
        // When: reset_ui
        app.reset_ui();
        // Then: input is focused again
        assert!(app.input.focused);
    }

    #[test]
    fn test_reset_ui_clears_pending_submit() {
        // Given: a pending submit is queued
        let mut app = App::new();
        app.pending_submit = Some("some message".to_string());
        // When: reset_ui
        app.reset_ui();
        // Then: pending_submit cleared
        assert!(app.pending_submit.is_none());
    }

    #[test]
    fn test_reset_ui_clears_pending_reset_flag() {
        // Given: pending_reset is set
        let mut app = App::new();
        app.pending_reset = true;
        // When: reset_ui
        app.reset_ui();
        // Then: pending_reset cleared
        assert!(!app.pending_reset);
    }

    #[test]
    fn test_reset_ui_preserves_selected_model() {
        // Given: a model is selected
        let mut app = App::with_models(two_models(), Some("openai/gpt-4o".to_string()));
        assert_eq!(app.selected_model.as_deref(), Some("openai/gpt-4o"));
        // When: reset_ui
        app.reset_ui();
        // Then: selected model is unchanged
        assert_eq!(app.selected_model.as_deref(), Some("openai/gpt-4o"));
    }

    #[test]
    fn test_reset_ui_preserves_model_list() {
        // Given: models are loaded
        let mut app = App::with_models(two_models(), None);
        assert_eq!(app.models.len(), 2);
        // When: reset_ui
        app.reset_ui();
        // Then: model list is unchanged
        assert_eq!(app.models.len(), 2);
    }

    #[test]
    fn test_reset_ui_preserves_skills() {
        // Given: skills are loaded
        let mut app = app_with_skills();
        let skill_count = app.skills.len();
        // When: reset_ui
        app.reset_ui();
        // Then: skills are unchanged
        assert_eq!(app.skills.len(), skill_count);
    }

    #[test]
    fn test_reset_ui_resets_scroll_offset() {
        // Given: chat has been scrolled
        let mut app = App::new();
        app.chat.scroll_offset = 42;
        // When: reset_ui
        app.reset_ui();
        // Then: scroll offset reset to 0
        assert_eq!(app.chat.scroll_offset, 0);
    }
}
