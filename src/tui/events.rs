#[derive(Debug)]
pub enum AppEvent {
    StreamChunk { text: String },
    StreamDone,
    Error(String),
}
