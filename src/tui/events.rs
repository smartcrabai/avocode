use crossterm::event::Event as CrosstermEvent;

#[derive(Debug)]
pub enum AppEvent {
    Crossterm(CrosstermEvent),
    Tick,
    NewMessage { content: String },
    StreamChunk { text: String },
    StreamDone,
    Error(String),
}

pub struct EventStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
}

impl EventStream {
    #[must_use]
    pub fn new() -> (Self, tokio::sync::mpsc::UnboundedSender<AppEvent>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { rx }, tx)
    }

    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }
}
