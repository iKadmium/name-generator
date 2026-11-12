pub mod data;
pub mod mcp;

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use data::{Dataset, Person, PickError, Picked};

/// Upper bound on the number of people in one request.
pub const MAX_PEOPLE: usize = 100;

type AppState = Arc<Dataset>;

/// Header clients must send the API key in.
pub const API_KEY_HEADER: &str = "x-api-key";

/// Build the router. Every route requires `api_key` in the [`API_KEY_HEADER`]
/// header.
pub fn app(dataset: Dataset, api_key: String) -> Router {
    let dataset = Arc::new(dataset);
    let api_key: Arc<str> = api_key.into();
    Router::new()
        .route("/names", post(names))
        .nest_service("/mcp", mcp::http_service(dataset.clone()))
        .route_layer(middleware::from_fn_with_state(api_key, require_api_key))
        .with_state(dataset)
}

async fn require_api_key(State(key): State<Arc<str>>, req: Request, next: Next) -> Response {
    let authorized = req
        .headers()
        .get(API_KEY_HEADER)
        .is_some_and(|v| constant_time_eq(v.as_bytes(), key.as_bytes()));
    if authorized {
        next.run(req).await
    } else {
        ApiError(
            StatusCode::UNAUTHORIZED,
            format!("missing or invalid {API_KEY_HEADER} header"),
        )
        .into_response()
    }
}

/// Compare without short-circuiting on the first differing byte, so response
/// timing doesn't reveal how much of a guessed key is correct.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamesRequest {
    /// The people to name (1 to 100). One name is returned for each, in the
    /// same order.
    pub people: Vec<Person>,
    /// Names that must not be returned (case-insensitive).
    #[serde(default)]
    pub forbidden: Vec<String>,
    /// Weight names by popularity, so common names come up more often than
    /// rare ones. Defaults to false: every name in the year's top 100 is
    /// equally likely.
    #[serde(default)]
    pub weighted: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct NamesResponse {
    pub names: Vec<Picked>,
}

/// Pick names for a request. Shared by the REST and MCP interfaces.
pub fn generate(ds: &Dataset, req: &NamesRequest) -> Result<NamesResponse, ApiError> {
    if !(1..=MAX_PEOPLE).contains(&req.people.len()) {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("people must contain between 1 and {MAX_PEOPLE} entries"),
        ));
    }
    let names = ds.pick(&req.people, &req.forbidden, req.weighted, &mut rand::rng())?;
    Ok(NamesResponse { names })
}

pub struct ApiError(StatusCode, String);

impl ApiError {
    pub fn message(&self) -> &str {
        &self.1
    }
}

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
    Ok(Json(generate(&ds, &req)?))
}
