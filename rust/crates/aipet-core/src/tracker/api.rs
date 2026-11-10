//! The tracker's companion routes as the pet reads them (the tracker's `docs/integrations/aipet.md`, FNT-64):
//! `GET /api/v1/tasks/active`, the change feed's messages, `GET /api/attach/hello`'s companion part, `POST /enroll`'s
//! reply and `GET /api/v1/runs/by-session/{id}`.
//!
//! The tracker's `TaskProgress` is read into the pet's [`TrackerTask`], leniently as the wire is read: unknown
//! members are ignored and missing ones are their defaults, so a newer tracker still reads.

use serde::Deserialize;
use serde_json::Value;

use super::wire::{Agents, Count, Links, Need, Placement, Tone, TrackerTask};

/// A task as the tracker sends it (`TaskProgress`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Progress {
    task_id: String,
    short_id: String,
    title: String,
    status: String,
    phase: Option<String>,
    review_round: Option<u32>,
    subtasks: Count,
    acceptance: Count,
    placement: Option<ProgressPlacement>,
    agents: Agents,
    needs: Vec<Value>,
    tone: Tone,
    last_change: LastChange,
    links: Links,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ProgressPlacement {
    runner_name: Option<String>,
    lane: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LastChange {
    at: String,
}

/// The tracker's need, whose permission names its tool `tool_name`.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ProgressNeed {
    Permission {
        run_id: Option<String>,
        request_id: Option<String>,
        tool_name: Option<String>,
    },
    Decision {
        decision_id: Option<String>,
    },
    Input {
        run_id: Option<String>,
    },
    Review,
}

impl From<Progress> for TrackerTask {
    fn from(p: Progress) -> TrackerTask {
        let needs = p
            .needs
            .into_iter()
            .filter_map(|need| ProgressNeed::deserialize(need).ok())
            .map(|need| match need {
                ProgressNeed::Permission {
                    run_id,
                    request_id,
                    tool_name,
                } => Need::Permission {
                    run_id,
                    request_id,
                    tool: tool_name,
                },
                ProgressNeed::Decision { decision_id } => Need::Decision { decision_id },
                ProgressNeed::Input { run_id } => Need::Input { run_id },
                ProgressNeed::Review => Need::Review,
            })
            .collect();
        TrackerTask {
            id: p.task_id,
            short_id: p.short_id,
            title: p.title,
            status: p.status,
            phase: p.phase,
            review_round: p.review_round,
            subtasks: p.subtasks,
            acceptance: p.acceptance,
            placement: p.placement.map(|place| Placement {
                runner: place.runner_name,
                lane: place.lane,
            }),
            agents: p.agents,
            needs,
            tone: p.tone,
            // the watcher fills them from the run lookups
            sessions: Vec::new(),
            links: p.links,
            at: p.last_change.at,
        }
    }
}

/// One task of the tracker's JSON; none without a task id.
pub fn task(value: &Value) -> Option<TrackerTask> {
    let task = TrackerTask::from(Progress::deserialize(value).ok()?);
    (!task.id.is_empty()).then_some(task)
}

/// The tasks of a list; those without a task id are left out.
fn tasks(value: Option<&Value>) -> Vec<TrackerTask> {
    value
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(task).collect())
        .unwrap_or_default()
}

/// Where the feed is: its id (another one is another process or pod) and the last change's number.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub feed: String,
    pub seq: u64,
}

/// A message of the change feed, or `GET /api/v1/tasks/active`'s answer (a snapshot).
#[derive(Clone, Debug, PartialEq)]
pub enum FeedMessage {
    Snapshot { at: Point, tasks: Vec<TrackerTask> },
    Change { at: Point, change: Change },
}

/// What one change does to the view. The other kinds (a prompt waiting or answered, a run started or failed, ...)
/// come with a `task.progress` of the task they changed, so they change nothing of their own here.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Upsert(Box<TrackerTask>),
    Remove(String),
    Other(String),
}

impl FeedMessage {
    /// A feed message, or the active tasks' answer; an error says what's wrong.
    pub fn parse(text: &str) -> Result<FeedMessage, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        let at = Point {
            feed: value.get("feed").and_then(Value::as_str).unwrap_or_default().to_owned(),
            seq: value.get("seq").and_then(Value::as_u64).ok_or("no seq")?,
        };
        match value.get("type").and_then(Value::as_str) {
            // the active tasks' answer has no type
            None | Some("snapshot") => Ok(FeedMessage::Snapshot {
                at,
                tasks: tasks(value.get("tasks")),
            }),
            Some("change") => {
                let kind = value.get("kind").and_then(Value::as_str).unwrap_or_default();
                let task_id = value.get("task_id").and_then(Value::as_str);
                let change = match (kind, value.get("task").and_then(task), task_id) {
                    ("task.progress", Some(task), _) => Change::Upsert(Box::new(task)),
                    ("task.removed", _, Some(id)) => Change::Remove(id.to_owned()),
                    _ => Change::Other(kind.to_owned()),
                };
                Ok(FeedMessage::Change { at, change })
            }
            Some(other) => Err(format!("a message of type {other}")),
        }
    }
}

/// `POST /enroll`'s reply for a companion: the device's id and its bearer token.
pub fn enrolled(text: &str) -> Result<(String, String), String> {
    let value: Value = serde_json::from_str(text).map_err(|_| "the tracker's reply isn't JSON".to_owned())?;
    let field = |name: &str| value.get(name).and_then(Value::as_str).filter(|s| !s.is_empty());
    match (field("id"), field("token")) {
        (Some(id), Some(token)) => Ok((id.to_owned(), token.to_owned())),
        _ => Err("the tracker's reply has no device id or token".into()),
    }
}

/// Whether the user switched on answers and pause/stop for this device: `hello`'s `companion.actions_allowed`.
pub fn actions_allowed(hello: &str) -> bool {
    serde_json::from_str::<Value>(hello)
        .ok()
        .and_then(|v| v.pointer("/companion/actions_allowed").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// A refusal's `{error}` text, else the status line.
pub fn refusal(status_line: &str, body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| status_line.to_owned())
}

/// `GET /api/v1/runs/by-session/{id}`'s answer: the task the session's run works on, if it names one.
pub fn run_task(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    value.get("task_id").and_then(Value::as_str).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A task as the tracker's companion end-to-end test sees it.
    fn progress() -> Value {
        json!({
            "task_id": "fnt-12-example", "short_id": "FNT-12", "title": "Example task", "repository": "demo",
            "status": "in_progress", "phase": "reviewing", "review_round": 2,
            "subtasks": {"done": 0, "total": 1}, "acceptance": {"done": 0, "total": 2}, "progress_percent": 40,
            "placement": {"runner_id": "r1", "runner_name": "build-box", "lane": "lane-a"},
            "agents": {"runs": 1, "sessions": 1, "subagents": 1, "total": 3},
            "needs": [
                {"kind": "permission", "run_id": "run-7", "request_id": "req-3", "tool_name": "Bash"},
                {"kind": "input", "run_id": "run-7"},
                {"kind": "teleport"}
            ],
            "tone": "amber",
            "last_change": {"at": "2026-11-09T10:00:00Z", "kind": "task.progress"},
            "links": {"task": "fnt://task/FNT-12", "run": "fnt://run/run-7"}
        })
    }

    #[test]
    fn the_trackers_task_reads_as_the_pets() {
        let task = task(&progress()).unwrap();
        assert_eq!((task.id.as_str(), task.short_id.as_str()), ("fnt-12-example", "FNT-12"));
        assert_eq!((task.phase.as_deref(), task.review_round), (Some("reviewing"), Some(2)));
        assert_eq!((task.subtasks.total, task.acceptance.total), (1, 2));
        assert_eq!(
            task.placement,
            Some(Placement {
                runner: Some("build-box".into()),
                lane: Some("lane-a".into())
            })
        );
        assert_eq!(task.agents.total, 3);
        assert_eq!(
            task.needs,
            [
                Need::Permission {
                    run_id: Some("run-7".into()),
                    request_id: Some("req-3".into()),
                    tool: Some("Bash".into())
                },
                Need::Input {
                    run_id: Some("run-7".into())
                }
            ]
        );
        assert_eq!(task.tone, Tone::Amber);
        assert_eq!(task.at, "2026-11-09T10:00:00Z");
        assert_eq!(task.links.run.as_deref(), Some("fnt://run/run-7"));
        assert!(super::task(&json!({"title": "no id"})).is_none());
    }

    #[test]
    fn feed_messages_read_as_snapshots_and_changes() {
        let snapshot = json!({"type": "snapshot", "feed": "f1", "seq": 4, "tasks": [progress(), {"x": 1}]});
        let FeedMessage::Snapshot { at, tasks } = FeedMessage::parse(&snapshot.to_string()).unwrap() else {
            panic!("not a snapshot");
        };
        assert_eq!((at.feed.as_str(), at.seq, tasks.len()), ("f1", 4, 1));
        // the active tasks' answer is a snapshot without a type
        let active = json!({"feed": "f1", "seq": 9, "tasks": []});
        assert!(matches!(
            FeedMessage::parse(&active.to_string()),
            Ok(FeedMessage::Snapshot {
                at: Point { seq: 9, .. },
                ..
            })
        ));
        for (change, expected) in [
            (
                json!({"type": "change", "feed": "f1", "seq": 5, "kind": "task.progress", "task_id": "fnt-12-example", "at": "", "task": progress()}),
                "upsert",
            ),
            (
                json!({"type": "change", "feed": "f1", "seq": 6, "kind": "task.removed", "task_id": "fnt-12-example", "at": ""}),
                "remove",
            ),
            (
                json!({"type": "change", "feed": "f1", "seq": 7, "kind": "permission.waiting", "task_id": "t", "run_id": "r", "request_id": "q", "at": ""}),
                "other",
            ),
        ] {
            let Ok(FeedMessage::Change { change, .. }) = FeedMessage::parse(&change.to_string()) else {
                panic!("not a change: {change}");
            };
            let got = match change {
                Change::Upsert(_) => "upsert",
                Change::Remove(_) => "remove",
                Change::Other(_) => "other",
            };
            assert_eq!(got, expected);
        }
        assert!(FeedMessage::parse(r#"{"type":"change","feed":"f1"}"#).is_err());
        assert!(FeedMessage::parse(r#"{"type":"lagged","seq":1}"#).is_err());
    }

    #[test]
    fn enrolment_hello_and_refusals_read() {
        assert_eq!(
            enrolled(r#"{"id":"pet-1","token":"abc","certificate":null}"#),
            Ok(("pet-1".into(), "abc".into()))
        );
        assert!(enrolled(r#"{"id":"pet-1"}"#).is_err());
        assert!(enrolled("<html>").is_err());
        assert!(actions_allowed(
            r#"{"versions":[],"companion":{"actions_allowed":true}}"#
        ));
        assert!(!actions_allowed(r#"{"companion":{"actions_allowed":false}}"#));
        assert!(!actions_allowed(r#"{"companion":null}"#));
        assert_eq!(
            refusal("403 Forbidden", r#"{"error":"code already used"}"#),
            "code already used"
        );
        assert_eq!(refusal("502 Bad Gateway", "<html>"), "502 Bad Gateway");
        assert_eq!(
            run_task(r#"{"run_id":"r","task_id":"fnt-12-example"}"#).as_deref(),
            Some("fnt-12-example")
        );
        assert_eq!(run_task(r#"{"run_id":"r"}"#), None);
    }
}
