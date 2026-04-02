pub mod app;
pub mod events;
pub mod keybinds;
pub mod theme;
pub mod widgets;

use std::io;
use std::sync::Arc;

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
use widgets::{chat::ChatWidget, input::InputWidget, sidebar::SidebarWidget, statusbar::StatusBar};

#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Provider(#[from] crate::provider::ProviderError),
    #[error(transparent)]
    Session(#[from] crate::session::SessionError),
}

pub type Result<T> = std::result::Result<T, TuiError>;

/// Run the TUI application.
///
/// # Errors
///
/// Returns an error if the terminal cannot be initialized or an IO error occurs.
#[expect(clippy::too_many_lines)]
pub async fn run() -> Result<()> {
    // Open session store and create a session for the current working directory.
    let ctx = crate::app::AppContext::new(std::env::current_dir()?);
    let store = Arc::new(ctx.open_session_store()?);

    let config = crate::config::loader::load(ctx.project_root()).unwrap_or_default();

    // Load providers before entering raw mode so failures produce clean error output.
    let providers = crate::provider::models_dev::fetch_dynamic_providers().await?;
    let providers = crate::provider::models_dev::filter_by_configured(
        providers,
        &config.configured_provider_ids(),
    );
    let choices = crate::provider::models_dev::to_model_choices(&providers);

    let session = crate::session::Session::new(
        ctx.project_id().to_string(),
        ctx.project_root().display().to_string(),
    );
    store.create_session(&session)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::with_models(choices, config.model);

    // Unbounded channel for events coming from background processor tasks.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<events::AppEvent>();

    loop {
        // Process all pending app events before drawing the next frame.
        while let Ok(ev) = event_rx.try_recv() {
            match ev {
                events::AppEvent::StreamChunk { text } => {
                    let buf = app.chat.streaming.get_or_insert_with(String::new);
                    buf.push_str(&text);
                }
                events::AppEvent::StreamDone => {
                    if let Some(text) = app.chat.streaming.take() {
                        app.chat.push(widgets::chat::ChatMessage {
                            role: widgets::chat::MessageRole::Assistant,
                            content: text,
                            timestamp: String::new(),
                        });
                    }
                }
                events::AppEvent::Error(e) => {
                    // Commit any partial streaming text as an incomplete assistant message.
                    if let Some(text) = app.chat.streaming.take()
                        && !text.is_empty()
                    {
                        app.chat.push(widgets::chat::ChatMessage {
                            role: widgets::chat::MessageRole::Assistant,
                            content: text,
                            timestamp: String::new(),
                        });
                    }
                    app.chat.push(widgets::chat::ChatMessage {
                        role: widgets::chat::MessageRole::System,
                        content: format!("[Error: {e}]"),
                        timestamp: String::new(),
                    });
                }
            }
        }

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

        // If the user submitted a message, launch the processor in a background task.
        if let Some(user_text) = app.take_pending_submit() {
            let model = app.selected_model.clone();
            let store_clone = Arc::clone(&store);
            let session_id = session.id.clone();
            let tx = event_tx.clone();

            tokio::spawn(async move {
                let (proc_tx, mut proc_rx) = tokio::sync::mpsc::channel(64);
                let options = crate::session::processor::ProcessOptions {
                    session_id,
                    user_message: user_text,
                    model,
                    agent: "default".to_owned(),
                    max_turns: None,
                };

                // Spawn the processor so the channel drain below runs concurrently.
                // Without this, a long response (>64 chunks) would fill the channel and deadlock.
                let proc_handle = tokio::spawn(async move {
                    crate::session::processor::process(&store_clone, options, proc_tx).await
                });

                // Forward processor events to the TUI event channel.
                while let Some(ev) = proc_rx.recv().await {
                    match ev {
                        crate::session::processor::ProcessEvent::PartUpdated { part, .. } => {
                            if let crate::session::Part::Text(t) = part {
                                let _ = tx.send(events::AppEvent::StreamChunk { text: t.text });
                            }
                        }
                        crate::session::processor::ProcessEvent::Done => {
                            let _ = tx.send(events::AppEvent::StreamDone);
                            return;
                        }
                        crate::session::processor::ProcessEvent::Error(e) => {
                            let _ = tx.send(events::AppEvent::Error(e));
                            return;
                        }
                        crate::session::processor::ProcessEvent::MessageCreated { .. } => {}
                    }
                }
                // Channel closed without Done/Error: surface the actual store error if available.
                let error_msg = match proc_handle.await {
                    Ok(Err(e)) => format!("processor error: {e}"),
                    _ => "internal error: processor closed unexpectedly".to_owned(),
                };
                let _ = tx.send(events::AppEvent::Error(error_msg));
            });
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
