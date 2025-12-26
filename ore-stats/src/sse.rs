//! Server-Sent Events (SSE) endpoints
//!
//! - `/sse/rounds` - Round updates (throttled to 500ms)
//! - `/sse/deployments` - Deployment events (batched: 10 items or 200ms)

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::State,
    response::sse::{Event, Sse},
};
use futures_util::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::app_state::{AppState, LiveBroadcastData};

/// GET /sse/rounds - Stream round updates (throttled)
pub async fn sse_rounds(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.subscribe_rounds();
    let stream = BroadcastStream::new(rx);
    
    let event_stream = stream
        .filter_map(|result| {
            match result {
                Ok(data) => {
                    match &data {
                        LiveBroadcastData::Round(_) => {
                            let json = serde_json::to_string(&data).ok()?;
                            Some(Ok(Event::default().event("round").data(json)))
                        }
                        LiveBroadcastData::WinningSquare { .. } => {
                            let json = serde_json::to_string(&data).ok()?;
                            Some(Ok(Event::default().event("winning_square").data(json)))
                        }
                        _ => None,
                    }
                }
                Err(_) => None,
            }
        });
    
    Sse::new(event_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// GET /sse/deployments - Stream deployment events (batched)
pub async fn sse_deployments(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.subscribe_deployments();
    let stream = BroadcastStream::new(rx);
    
    let event_stream = stream
        .filter_map(|result| {
            match result {
                Ok(data) => {
                    match &data {
                        LiveBroadcastData::Deployment(_) => {
                            let json = serde_json::to_string(&data).ok()?;
                            Some(Ok(Event::default().event("deployment").data(json)))
                        }
                        LiveBroadcastData::WinningSquare { .. } => {
                            let json = serde_json::to_string(&data).ok()?;
                            Some(Ok(Event::default().event("winning_square").data(json)))
                        }
                        _ => None,
                    }
                }
                Err(_) => None,
            }
        });
    
    Sse::new(event_stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

