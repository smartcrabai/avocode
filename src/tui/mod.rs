pub mod app;
pub mod events;
pub mod keybinds;
pub mod theme;
pub mod widgets;

use app::App;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    prelude::{StatefulWidget, Widget},
};
use std::io;
use widgets::{chat::ChatWidget, input::InputWidget, sidebar::SidebarWidget, statusbar::StatusBar};

#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Provider(#[from] crate::provider::ProviderError),
}

pub type Result<T> = std::result::Result<T, TuiError>;

/// Run the TUI application.
///
/// # Errors
///
/// Returns an error if the terminal cannot be initialized or an IO error occurs.
pub async fn run() -> Result<()> {
    // Load providers before entering raw mode so failures produce clean error output.
    let providers = crate::provider::models_dev::fetch_dynamic_providers().await?;
    let choices = crate::provider::models_dev::to_model_choices(&providers);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::with_models(choices, None);

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = if app.show_sidebar {
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Length(30), Constraint::Min(0)])
                    .split(area)
            } else {
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Min(0)])
                    .split(area)
            };

            let main_idx = usize::from(app.show_sidebar);

            if app.show_sidebar {
                SidebarWidget { theme: &app.theme }.render(
                    chunks[0],
                    frame.buffer_mut(),
                    &mut app.sidebar,
                );
            }

            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(5),
                    Constraint::Length(1),
                ])
                .split(chunks[main_idx]);

            ChatWidget { theme: &app.theme }.render(
                main_chunks[0],
                frame.buffer_mut(),
                &mut app.chat,
            );
            InputWidget { theme: &app.theme }.render(
                main_chunks[1],
                frame.buffer_mut(),
                &mut app.input,
            );
            StatusBar {
                theme: &app.theme,
                model: app.selected_model.as_deref().unwrap_or("(no model)"),
                mode: "INSERT",
                keys_hint: "^C quit | ^B sidebar | ^T theme | ^P model",
            }
            .render(main_chunks[2], frame.buffer_mut());
        })?;

        if event::poll(std::time::Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
        {
            app.handle_key(key);
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tui::{app::App, theme};

    #[test]
    fn test_app_creation() {
        let app = App::new();
        assert!(!app.should_quit);
        assert!(app.show_sidebar);
    }

    #[test]
    fn test_theme_cycling() {
        let mut app = App::new();
        let initial_name = app.theme.name;
        app.cycle_theme();
        let all = theme::all_themes();
        assert!(all.len() > 1 || app.theme.name == initial_name);
    }

    #[test]
    fn test_input_state() {
        use crate::tui::widgets::input::InputState;
        let mut input = InputState::new();
        input.insert_char('h');
        input.insert_char('i');
        assert_eq!(input.text, "hi");
        input.delete_char();
        assert_eq!(input.text, "h");
        let taken = input.take_text();
        assert_eq!(taken, "h");
        assert!(input.text.is_empty());
    }
}
