#[derive(Debug)]
pub enum AppEvent {
    StreamChunk { session_id: String, text: String },
    StreamDone { session_id: String },
    Error(String),
}
