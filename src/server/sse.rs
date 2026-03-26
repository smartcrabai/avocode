use axum::response::sse::{Event, KeepAlive, Sse};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

pub fn sse_handler(
    rx: broadcast::Receiver<super::state::ServerEvent>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let stream = BroadcastStream::new(rx).filter_map(|msg| {
        msg.ok().and_then(|event| {
            serde_json::to_string(&event)
                .ok()
                .map(|data| Ok(Event::default().data(data)))
        })
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
