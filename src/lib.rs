pub mod data;

use std::sync::Arc;

use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use data::{Dataset, Person, PickError, Picked};

/// Upper bound on the number of people in one request.
pub const MAX_PEOPLE: usize = 100;

type AppState = Arc<Dataset>;

pub fn app(dataset: Dataset) -> Router {
    Router::new()
        .route("/names", post(names))
        .with_state(Arc::new(dataset))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NamesRequest {
    people: Vec<Person>,
    #[serde(default)]
    forbidden: Vec<String>,
}

#[derive(Serialize)]
struct NamesResponse {
    names: Vec<Picked>,
}

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        #[derive(Serialize)]
        struct Body {
            error: String,
        }
        (self.0, Json(Body { error: self.1 })).into_response()
    }
}

impl From<JsonRejection> for ApiError {
    fn from(r: JsonRejection) -> Self {
        Self(r.status(), r.body_text())
    }
}

impl From<PickError> for ApiError {
    fn from(e: PickError) -> Self {
        Self(StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
    }
}

async fn names(
    State(ds): State<AppState>,
    body: Result<Json<NamesRequest>, JsonRejection>,
) -> Result<Json<NamesResponse>, ApiError> {
    let Json(req) = body?;
    if !(1..=MAX_PEOPLE).contains(&req.people.len()) {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("people must contain between 1 and {MAX_PEOPLE} entries"),
        ));
    }
    let names = ds.pick(&req.people, &req.forbidden, &mut rand::rng())?;
    Ok(Json(NamesResponse { names }))
}
