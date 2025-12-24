use thiserror::Error;
use serde::{Serialize};

#[derive(Error, Debug)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        use axum::{http::StatusCode, Json};
        #[derive(Serialize)]
        struct ErrBody { error: String }
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, Json(ErrBody { error: "not found".into() })).into_response(),
            other => {
                tracing::error!("internal error: {other:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrBody { error: "internal server error".into() })).into_response()
            }
        }
    }
}

