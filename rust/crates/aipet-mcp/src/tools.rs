//! The MCP tools. Each returns `structuredContent` matching its `outputSchema` and the same JSON as a text block,
//! carries read-only, idempotent or destructive annotations, and reports a business failure as `isError` with the
//! reason; unknown tools and malformed calls are JSON-RPC errors (rmcp's). They are listed in [`TOOLS`]' order, not
//! rmcp's (by name). Tasks 5 and 6 add theirs here, to [`TOOLS`], and to the conformance test's table.

use std::sync::Arc;
use std::time::SystemTime;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CacheScope, Implementation, ListToolsResult, PaginatedRequestParams, ProtocolVersion, ResultType,
    ServerCapabilities, ServerConfig,
};
use rmcp::schemars::JsonSchema;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData, Json, ServerHandler, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{State, hook, report};

/// Every tool, in the order `tools/list` gives them.
pub const TOOLS: [&str; 2] = ["status", "report_event"];

/// What `status` says: the pet's version and port, the last 30 hook events it recorded (from the pipe or HTTP, as
/// the doctor's ping gives them), and which agents' hooks have reported since the pet started.
#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Status {
    /// The pet's version.
    version: String,
    /// The port the pet listens on.
    port: u16,
    /// The last 30 lines of hook-events.log this pet wrote, oldest first.
    hook_events: Vec<String>,
    /// Whether an event from each agent's hooks reached the pet since it started.
    registrations_seen: Seen,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Seen {
    claude: bool,
    codex: bool,
}

/// What Codex's hooks give `report_event`.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Report {
    /// The agent whose hook reports: "codex".
    agent: String,
    /// The hook's input: `session_id`, `hook_event_name`, and the fields that event has.
    event: Map<String, Value>,
}

/// What `report_event` answers: nothing, once the event is queued.
#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Reported {}

/// The MCP server, one per session (or per stateless request).
#[derive(Clone)]
pub(crate) struct Pet {
    state: Arc<State>,
    tool_router: ToolRouter<Pet>,
}

impl Pet {
    pub(crate) fn new(state: Arc<State>) -> Pet {
        Pet {
            state,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl Pet {
    #[tool(
        description = "The AiPet desktop pet's status: its version and port, the last 30 hook events it got, and \
                       which agents' hooks have reported since it started.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn status(&self) -> Json<Status> {
        let ingress = &self.state.ingress;
        Json(Status {
            version: self.state.version.clone(),
            port: self.state.port,
            hook_events: ingress.recent(),
            registrations_seen: Seen {
                claude: ingress.seen("claude"),
                codex: ingress.seen("codex"),
            },
        })
    }

    #[tool(
        description = "For Codex's hooks only, which call it on every hook event to show the chat's status on the \
                       AiPet desktop pet. Never call it yourself: it reports nothing about your work.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn report_event(&self, Parameters(report): Parameters<Report>) -> Result<Json<Reported>, String> {
        let events = &self.state.events;
        let envelope =
            report::envelope(&report.agent, &report.event, SystemTime::now(), &hook::agent_file).map_err(|why| {
                events.drop_event(&format!("report_event refused it: {why}"));
                why
            })?;
        if !events.push(envelope) {
            return Err("the pet couldn't queue the event, so it was dropped".into());
        }
        Ok(Json(Reported {}))
    }
}

#[tool_handler]
impl ServerHandler for Pet {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("aipet", self.state.version.clone()))
            .with_instructions("Drives the AiPet desktop pet running for this user.")
    }

    /// rmcp's list, in [`TOOLS`]' order.
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by_key(|tool| TOOLS.iter().position(|name| *name == tool.name).unwrap_or(usize::MAX));
        let cache_hints = context
            .protocol_version()
            .is_some_and(|version| version >= ProtocolVersion::V_2026_07_28);
        Ok(ListToolsResult {
            result_type: Some(ResultType::COMPLETE),
            tools,
            meta: None,
            next_cursor: None,
            ttl_ms: cache_hints.then_some(0),
            cache_scope: cache_hints.then_some(CacheScope::Public),
        })
    }
}
