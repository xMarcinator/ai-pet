//! The MCP tools. Each returns `structuredContent` matching its `outputSchema` and the same JSON as a text block,
//! carries read-only, idempotent or destructive annotations, and reports a business failure as `isError` with the
//! reason; unknown tools and malformed calls are JSON-RPC errors (rmcp's). They are listed in [`TOOLS`]' order, not
//! rmcp's (by name). Task 6 adds its here, to [`TOOLS`], and to the conformance test's table.
//!
//! The avatar, settings and board tools ask the pet ([`aipet_core::control`]) on a blocking thread, so a pet slow to
//! answer holds up no other request.

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

use aipet_core::control::{self, ANSWER_WITHIN, Answer, Command, SettingsChange};

use crate::{State, hook, report};

/// Every tool, in the order `tools/list` gives them.
pub const TOOLS: [&str; 8] = [
    "status",
    "report_event",
    "list_avatars",
    "wear_avatar",
    "get_settings",
    "set_settings",
    "get_board",
    "dismiss_card",
];

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

/// The pet worn.
#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Worn {
    /// The pet's name.
    name: String,
    style: String,
    theme: String,
}

impl From<control::Worn> for Worn {
    fn from(worn: control::Worn) -> Worn {
        Worn {
            name: worn.name,
            style: worn.style,
            theme: worn.theme,
        }
    }
}

/// What `list_avatars` says.
#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Avatars {
    /// The pet worn now.
    worn: Worn,
    /// Every pet the user can wear, one entry per pet, in the order Settings shows them.
    avatars: Vec<Avatar>,
    /// What the avatars folder had that didn't load, and why.
    skipped: Vec<Skipped>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Avatar {
    name: String,
    /// Its drawings; the first is its default.
    styles: Vec<Style>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Style {
    /// What wear_avatar's style takes, such as "Pixel" or "Vector".
    style: String,
    /// "pixel", "painted" or "vector".
    kind: String,
    /// Its colour themes; the first is its default.
    themes: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Skipped {
    /// The folder or file in the avatars folder.
    path: String,
    reason: String,
}

/// What `wear_avatar` takes.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Wear {
    /// The pet, as list_avatars names it (any case).
    name: String,
    /// One of its styles; its default when left out.
    style: Option<String>,
    /// One of that style's themes; the style's default when left out.
    theme: Option<String>,
}

/// What `wear_avatar` answers: the pet now worn.
#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Wearing {
    /// The pet's name.
    worn: String,
    style: String,
    theme: String,
}

/// The settings `get_settings` and `set_settings` give.
#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Settings {
    /// How big the pet is drawn, in percent (75 to 200).
    pet_size: i32,
    /// How big the bubbles are drawn, in percent (80 to 120).
    bubble_size: i32,
    /// Whether the bubbles show.
    bubbles: bool,
    /// Whether the pet stays above other windows.
    on_top: bool,
    /// Whether the pet listens along to the music player.
    music: bool,
}

/// What `set_settings` takes: the settings to change; the rest stay.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Change {
    /// How big the pet is drawn, in percent (75 to 200).
    pet_size: Option<i32>,
    /// How big the bubbles are drawn, in percent (80 to 120).
    bubble_size: Option<i32>,
    bubbles: Option<bool>,
    on_top: Option<bool>,
    music: Option<bool>,
}

/// What `get_board` says.
#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Board {
    /// The board's cards, shown or folded into a stack.
    cards: Vec<Card>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Card {
    /// What dismiss_card takes.
    id: String,
    /// "claude", "codex", "tracker", "jira", "github" or "music".
    source: String,
    title: String,
    /// idle, thinking, working, attention, done, review, music, paused or error.
    state: String,
}

/// What `dismiss_card` takes.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Dismiss {
    /// A card's id, as get_board gives it.
    id: String,
}

/// What `dismiss_card` answers.
#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub(crate) struct Dismissed {
    dismissed: String,
}

/// An answer the command doesn't give: the pet and the listener disagree.
fn unexpected(answer: &Answer) -> String {
    format!("the pet answered something else: {answer:?}")
}

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

    /// Asks the pet, on a blocking thread, waiting up to [`ANSWER_WITHIN`].
    async fn ask(&self, command: Command) -> Result<Answer, String> {
        let pet = Arc::clone(&self.state.pet);
        tokio::task::spawn_blocking(move || control::ask(&pet, command, ANSWER_WITHIN))
            .await
            .unwrap_or_else(|e| Err(format!("asking the pet failed: {e}")))
    }

    async fn settings(&self, command: Command) -> Result<Json<Settings>, String> {
        match self.ask(command).await? {
            Answer::Settings(s) => Ok(Json(Settings {
                pet_size: s.pet_size,
                bubble_size: s.bubble_size,
                bubbles: s.bubbles,
                on_top: s.on_top,
                music: s.music,
            })),
            other => Err(unexpected(&other)),
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

    #[tool(
        description = "The pets the AiPet desktop pet can wear, each with its styles (drawings) and their colour \
                       themes, the one it wears now, and what in its avatars folder didn't load.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_avatars(&self) -> Result<Json<Avatars>, String> {
        match self.ask(Command::ListAvatars).await? {
            Answer::Avatars { worn, avatars, skipped } => Ok(Json(Avatars {
                worn: worn.into(),
                avatars: avatars
                    .into_iter()
                    .map(|a| Avatar {
                        name: a.name,
                        styles: a
                            .styles
                            .into_iter()
                            .map(|s| Style {
                                style: s.style,
                                kind: s.kind,
                                themes: s.themes,
                            })
                            .collect(),
                    })
                    .collect(),
                skipped: skipped
                    .into_iter()
                    .map(|s| Skipped {
                        path: s.path,
                        reason: s.reason,
                    })
                    .collect(),
            })),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Makes the AiPet desktop pet wear a pet from list_avatars, in one of its styles and themes \
                       (its defaults when left out), as choosing it in Settings does. It is saved.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn wear_avatar(&self, Parameters(wear): Parameters<Wear>) -> Result<Json<Wearing>, String> {
        let command = Command::Wear {
            name: wear.name,
            style: wear.style,
            theme: wear.theme,
        };
        match self.ask(command).await? {
            Answer::Worn(worn) => Ok(Json(Wearing {
                worn: worn.name,
                style: worn.style,
                theme: worn.theme,
            })),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "The AiPet desktop pet's settings: how big the pet and its bubbles are drawn, whether the \
                       bubbles show, whether it stays on top, and whether it listens along to the music player.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_settings(&self) -> Result<Json<Settings>, String> {
        self.settings(Command::GetSettings).await
    }

    #[tool(
        description = "Changes some of the AiPet desktop pet's settings (the rest stay), as Settings does, and \
                       saves them. An out-of-range size changes nothing. Answers with all the settings.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn set_settings(&self, Parameters(change): Parameters<Change>) -> Result<Json<Settings>, String> {
        self.settings(Command::SetSettings(SettingsChange {
            pet_size: change.pet_size,
            bubble_size: change.bubble_size,
            bubbles: change.bubbles,
            on_top: change.on_top,
            music: change.music,
        }))
        .await
    }

    #[tool(
        description = "The cards on the AiPet desktop pet's board: agents' chats, tracker tasks, reviews and the \
                       music player, with their state.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_board(&self) -> Result<Json<Board>, String> {
        match self.ask(Command::GetBoard).await? {
            Answer::Board(cards) => Ok(Json(Board {
                cards: cards
                    .into_iter()
                    .map(|c| Card {
                        id: c.id,
                        source: c.source,
                        title: c.title,
                        state: c.state,
                    })
                    .collect(),
            })),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Dismisses a card on the AiPet desktop pet's board, as its close button does: it stays hidden \
                       until it does something new.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn dismiss_card(&self, Parameters(dismiss): Parameters<Dismiss>) -> Result<Json<Dismissed>, String> {
        match self.ask(Command::Dismiss(dismiss.id)).await? {
            Answer::Dismissed(id) => Ok(Json(Dismissed { dismissed: id })),
            other => Err(unexpected(&other)),
        }
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
