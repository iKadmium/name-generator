//! MCP (Model Context Protocol) server exposing name generation as a tool.
//!
//! Available over Streamable HTTP at `/mcp` (behind the same API key as the
//! REST API) and over stdio (`name-generator --stdio`).

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{Json, ServerHandler, tool, tool_handler, tool_router};

use crate::data::Dataset;
use crate::{NamesRequest, NamesResponse, generate};

#[derive(Clone)]
pub struct NameServer {
    dataset: Arc<Dataset>,
    #[allow(dead_code, reason = "read by the code tool_handler generates")]
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl NameServer {
    pub fn new(dataset: Arc<Dataset>) -> Self {
        Self {
            dataset,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Generate realistic first names for people born in England and Wales, \
            given each person's gender and year of birth. Names are drawn at random from \
            the ONS top 100 baby names for the nearest year on record (1904-2024, every \
            10 years), uniformly by default or weighted by popularity if `weighted` is \
            true. All returned names are distinct, and none are in `forbidden`."
    )]
    fn generate_names(
        &self,
        Parameters(req): Parameters<NamesRequest>,
    ) -> Result<Json<NamesResponse>, String> {
        generate(&self.dataset, &req)
            .map(Json)
            .map_err(|e| e.message().to_owned())
    }
}

#[tool_handler]
impl ServerHandler for NameServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Use generate_names to get plausible first names for people from England \
                 and Wales by gender and birth year.",
            )
    }
}

/// Streamable HTTP service for mounting into the Axum router.
///
/// Runs statelessly with plain JSON responses: the tool has no state and sends
/// no notifications, so no sessions or SSE streams are needed. Host header
/// validation (a DNS-rebinding defence for unauthenticated local servers) is
/// disabled because the endpoint requires the API key and must work behind
/// whatever hostname it is deployed on.
pub fn http_service(
    dataset: Arc<Dataset>,
) -> StreamableHttpService<NameServer, LocalSessionManager> {
    StreamableHttpService::new(
        move || Ok(NameServer::new(dataset.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .disable_allowed_hosts(),
    )
}
