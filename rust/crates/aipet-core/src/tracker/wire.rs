//! Protocol version 2's task messages (fnt-81): what the tracker watcher hands the Board for each active tracker
//! task, as the tracker's `docs/integrations/aipet.md` writes them.
//!
//! ```text
//! {"v":2,"type":"task","op":"upsert","source":"tracker","task":{"id":..,"short_id":..,"title":..,..}}
//! {"v":2,"type":"task","op":"remove","source":"tracker","id":".."}
//! ```
//!
//! Reading is lenient, as a newer tracker may add to it: unknown members are ignored, a missing one is its default,
//! a need of a kind it doesn't know is dropped and a tone it doesn't know is normal. Only the pet's own watcher makes
//! these: the hook server refuses a task line ([`TASK_ONLY_FROM_WATCHER`]).
//!
//! [`TASK_ONLY_FROM_WATCHER`]: aipet_ipc::protocol::TASK_ONLY_FROM_WATCHER

use aipet_ipc::protocol::{ID, KIND, OP, REMOVE, SOURCE, TASK, UPSERT, V, VERSION_2};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

/// `source` of every task message the pet makes.
pub const SOURCE_TRACKER: &str = "tracker";

/// One active tracker task, as its bubble shows it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackerTask {
    /// The tracker's task id, e.g. `fnt-12-example`.
    pub id: String,
    /// What the user calls it, e.g. `FNT-12`.
    pub short_id: String,
    /// The tracker sends only the short id where the title failed its secret check.
    pub title: String,
    pub status: String,
    /// `planning`, `implementing`, `testing`, `reviewing`, `fixing`, `verifying`, or none.
    pub phase: Option<String>,
    pub review_round: Option<u32>,
    pub subtasks: Count,
    pub acceptance: Count,
    pub placement: Option<Placement>,
    pub agents: Agents,
    #[serde(deserialize_with = "known_needs")]
    pub needs: Vec<Need>,
    pub tone: Tone,
    /// The agent sessions working on it: chats with these ids collapse into its bubble.
    pub sessions: Vec<String>,
    pub links: Links,
    /// When it last changed, as the tracker wrote it (RFC 3339).
    pub at: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Count {
    pub done: u32,
    pub total: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Placement {
    pub runner: Option<String>,
    pub lane: Option<String>,
}

/// How many agents work on the task; `total` is the bubble's badge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Agents {
    pub total: u32,
    pub runs: u32,
    pub sessions: u32,
    pub subagents: u32,
}

/// What waits on the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Need {
    Permission {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        /// The tool's name only: the tracker sends no input.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<String>,
    },
    Decision {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decision_id: Option<String>,
    },
    /// A run waits for the user's reply to its message.
    Input {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<String>,
    },
    Review,
}

/// Red wins over amber: a failed run or a paused guard, then something that needs the user.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    #[default]
    Normal,
    Amber,
    Red,
}

impl<'de> Deserialize<'de> for Tone {
    /// A tone this pet doesn't know, or none, is normal.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(
            match Option::<Value>::deserialize(d)?.as_ref().and_then(Value::as_str) {
                Some("amber") => Tone::Amber,
                Some("red") => Tone::Red,
                _ => Tone::Normal,
            },
        )
    }
}

/// `fnt://` deep links into the tracker's desktop client; they never carry a token.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Links {
    pub task: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
}

/// A version 2 task message.
#[derive(Clone, Debug, PartialEq)]
pub enum TaskMessage {
    Upsert(Box<TrackerTask>),
    Remove(String),
}

impl TaskMessage {
    /// The message's line, without the `\n`.
    pub fn to_line(&self) -> String {
        let mut line = Map::new();
        line.insert(V.into(), VERSION_2.into());
        line.insert(KIND.into(), TASK.into());
        match self {
            TaskMessage::Upsert(task) => {
                line.insert(OP.into(), UPSERT.into());
                line.insert(SOURCE.into(), SOURCE_TRACKER.into());
                line.insert(TASK.into(), serde_json::to_value(task).expect("a task is plain data"));
            }
            TaskMessage::Remove(id) => {
                line.insert(OP.into(), REMOVE.into());
                line.insert(SOURCE.into(), SOURCE_TRACKER.into());
                line.insert(ID.into(), id.as_str().into());
            }
        }
        Value::Object(line).to_string()
    }

    /// A task message's line; an error says what's wrong with any other.
    pub fn parse(line: &str) -> Result<TaskMessage, String> {
        let fields: Map<String, Value> = serde_json::from_str(line).map_err(|e| format!("not a JSON object: {e}"))?;
        if fields.get(V).and_then(Value::as_f64) != Some(f64::from(VERSION_2)) {
            return Err("not version 2".into());
        }
        if fields.get(KIND).and_then(Value::as_str) != Some(TASK) {
            return Err("not a task message".into());
        }
        match fields.get(OP).and_then(Value::as_str) {
            Some(UPSERT) => {
                let task = fields.get(TASK).ok_or("an upsert without its task")?;
                let task = TrackerTask::deserialize(task).map_err(|e| format!("its task: {e}"))?;
                if task.id.is_empty() {
                    return Err("a task without an id".into());
                }
                Ok(TaskMessage::Upsert(Box::new(task)))
            }
            Some(REMOVE) => match fields.get(ID).and_then(Value::as_str) {
                Some(id) if !id.is_empty() => Ok(TaskMessage::Remove(id.to_owned())),
                _ => Err("a remove without an id".into()),
            },
            _ => Err("an op other than upsert or remove".into()),
        }
    }
}

/// The needs this pet knows; a newer tracker's other kinds are left out.
fn known_needs<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Need>, D::Error> {
    let raw = Option::<Vec<Value>>::deserialize(d)?.unwrap_or_default();
    Ok(raw
        .into_iter()
        .filter_map(|need| Need::deserialize(need).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tracker doc's upsert example (`docs/integrations/aipet.md`, "AiPet's protocol v2").
    const DOC_UPSERT: &str = r#"{"v":2,"type":"task","op":"upsert","source":"tracker","task":{"id":"fnt-12-example","short_id":"FNT-12","title":"Example task","status":"In progress","phase":"reviewing","review_round":2,"subtasks":{"done":3,"total":5},"acceptance":{"done":4,"total":6},"placement":{"runner":"build-box","lane":"lane-a"},"agents":{"total":3,"runs":1,"sessions":1,"subagents":1},"needs":[{"kind":"permission","run_id":"run-7","request_id":"req-3","tool":"Edit"}],"tone":"amber","sessions":["session-7"],"links":{"task":"fnt://task/FNT-12","run":"fnt://run/run-7"},"at":"2026-11-09T10:00:00Z"}}"#;
    const DOC_REMOVE: &str = r#"{"v":2,"type":"task","op":"remove","source":"tracker","id":"fnt-12-example"}"#;

    fn example() -> TrackerTask {
        TrackerTask {
            id: "fnt-12-example".into(),
            short_id: "FNT-12".into(),
            title: "Example task".into(),
            status: "In progress".into(),
            phase: Some("reviewing".into()),
            review_round: Some(2),
            subtasks: Count { done: 3, total: 5 },
            acceptance: Count { done: 4, total: 6 },
            placement: Some(Placement {
                runner: Some("build-box".into()),
                lane: Some("lane-a".into()),
            }),
            agents: Agents {
                total: 3,
                runs: 1,
                sessions: 1,
                subagents: 1,
            },
            needs: vec![Need::Permission {
                run_id: Some("run-7".into()),
                request_id: Some("req-3".into()),
                tool: Some("Edit".into()),
            }],
            tone: Tone::Amber,
            sessions: vec!["session-7".into()],
            links: Links {
                task: Some("fnt://task/FNT-12".into()),
                run: Some("fnt://run/run-7".into()),
            },
            at: "2026-11-09T10:00:00Z".into(),
        }
    }

    #[test]
    fn the_docs_examples_read_as_meant() {
        assert_eq!(
            TaskMessage::parse(DOC_UPSERT),
            Ok(TaskMessage::Upsert(Box::new(example())))
        );
        assert_eq!(
            TaskMessage::parse(DOC_REMOVE),
            Ok(TaskMessage::Remove("fnt-12-example".into()))
        );
    }

    /// Written and read back, a message is the same message, and its JSON is the doc's (member order aside).
    #[test]
    fn messages_round_trip() {
        for (message, doc) in [
            (TaskMessage::Upsert(Box::new(example())), DOC_UPSERT),
            (TaskMessage::Remove("fnt-12-example".into()), DOC_REMOVE),
        ] {
            let line = message.to_line();
            assert_eq!(TaskMessage::parse(&line).as_ref(), Ok(&message));
            let ours: Value = serde_json::from_str(&line).unwrap();
            let theirs: Value = serde_json::from_str(doc).unwrap();
            assert_eq!(ours, theirs);
        }
    }

    #[test]
    fn a_newer_trackers_additions_are_ignored() {
        let line = r#"{"v":2,"type":"task","op":"upsert","source":"tracker","extra":1,"task":{"id":"t","colour":"teal","tone":"purple","needs":[{"kind":"meeting"},{"kind":"review"},{"kind":"decision","decision_id":"d1"}]}}"#;
        let Ok(TaskMessage::Upsert(task)) = TaskMessage::parse(line) else {
            panic!("not read");
        };
        assert_eq!(task.tone, Tone::Normal);
        assert_eq!(
            task.needs,
            [
                Need::Review,
                Need::Decision {
                    decision_id: Some("d1".into())
                }
            ]
        );
        assert_eq!(
            (task.subtasks, task.agents.total, task.phase),
            (Count::default(), 0, None)
        );
    }

    #[test]
    fn what_isnt_a_task_message_says_why() {
        for (line, why) in [
            ("[]", "not a JSON object"),
            (r#"{"v":1,"type":"task","op":"remove","id":"t"}"#, "not version 2"),
            (r#"{"v":2,"type":"event","payload":{}}"#, "not a task message"),
            (
                r#"{"v":2,"type":"task","op":"rename","id":"t"}"#,
                "an op other than upsert or remove",
            ),
            (r#"{"v":2,"type":"task","op":"upsert"}"#, "an upsert without its task"),
            (
                r#"{"v":2,"type":"task","op":"upsert","task":{"title":"x"}}"#,
                "a task without an id",
            ),
            (
                r#"{"v":2,"type":"task","op":"remove","id":""}"#,
                "a remove without an id",
            ),
            (
                r#"{"v":2,"type":"task","op":"upsert","task":{"id":"t","subtasks":{"done":-1}}}"#,
                "its task",
            ),
        ] {
            let error = TaskMessage::parse(line).unwrap_err();
            assert!(error.starts_with(why), "{line}: {error}");
        }
    }
}
