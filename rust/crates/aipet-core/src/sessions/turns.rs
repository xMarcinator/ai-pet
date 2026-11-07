//! Codex's turns, per thread of a chat, and matching a call's halves (`Turns`, `AgentSessions.cs:654-731`); which turn
//! came first by their UUIDv7 ids (`V7Before`, `:743-749`).

use serde_json::{Map, Value};

use super::claude::{call_key, is_guid_d};
use super::{Envelope, Thrown, str_of};

/// How far apart, in seconds, the hooks of a Codex call's PreToolUse and PermissionRequest can start: PowerShell
/// starts each 1-4 s late.
const HOOK_SPREAD: f64 = 5.0;

/// How many of a thread's turns before the current one are kept, for ids that don't sort.
const PAST: usize = 16;

/// How many of the current turn's tool calls are kept.
const CALLS: usize = 64;

/// One thread of a Codex chat, as far as its events have come (see `codex::order`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Turns {
    /// The current turn's id.
    pub current: String,
    /// The current turn's end is in.
    pub ended: bool,
    /// An event of the current turn is in.
    pub seen: bool,
    /// The turns before, for ids that don't sort (the latest few).
    past: Vec<String>,
    /// The current turn's tool calls.
    calls: Vec<Call>,
}

/// A tool call of the current turn.
#[derive(Clone, Debug, PartialEq)]
struct Call {
    /// Its tool_use_id; none for a PermissionRequest whose PreToolUse hasn't come.
    id: Option<String>,
    /// What it runs (`call_key`).
    what: u64,
    /// How far it has come: 1 PreToolUse, 2 PermissionRequest, 3 PostToolUse.
    step: u8,
    /// When its PreToolUse's hook started, else its first event's.
    at: f64,
}

impl Turns {
    /// A thread whose first turn is `turn`.
    pub fn new(turn: &str) -> Self {
        Turns {
            current: turn.into(),
            ended: false,
            seen: false,
            past: Vec::new(),
            calls: Vec::new(),
        }
    }

    /// Whether a turn other than the current one came before it.
    pub fn before(&self, turn: &str, now: f64) -> Result<bool, Thrown> {
        Ok(self.past.iter().any(|t| t == turn) || v7_before(turn, &self.current, now)?)
    }

    /// A later turn starts.
    pub fn next(&mut self, turn: &str) {
        let done = std::mem::replace(&mut self.current, turn.into());
        self.past.push(done);
        if self.past.len() > PAST {
            self.past.remove(0);
        }
        self.ended = false;
        self.seen = false;
        self.calls.clear();
    }

    /// Records a tool event of the current turn: true for a PreToolUse whose call has come further already, false for
    /// a PermissionRequest whose call's PreToolUse is the chat's `latest` event, else none.
    pub fn call(
        &mut self,
        ev: &str,
        p: &Map<String, Value>,
        envelope: &Envelope,
        at: f64,
        latest: f64,
    ) -> Option<bool> {
        let step = match ev {
            "PreToolUse" => 1,
            "PermissionRequest" => 2,
            "PostToolUse" => 3,
            _ => return None,
        };
        let id = str_of(Some(p), "tool_use_id").filter(|x| !x.is_empty());
        let what = call_key(p, envelope);
        // the call by its id; else one of the same tool and command within HOOK_SPREAD: a PermissionRequest takes the
        // latest still at its PreToolUse, a PreToolUse or PostToolUse the closest PermissionRequest waiting for its
        // PreToolUse
        let mut found = id.and_then(|id| self.calls.iter().rposition(|c| c.id.as_deref() == Some(id)));
        let by_id = found.is_some();
        if !by_id {
            for (j, c) in self.calls.iter().enumerate() {
                if c.what != what || (c.at - at).abs() > HOOK_SPREAD {
                    continue;
                }
                let better = if step == 2 {
                    c.step == 1 && found.is_none_or(|i| c.at >= self.calls[i].at)
                } else {
                    c.id.is_none() && found.is_none_or(|i| (c.at - at).abs() < (self.calls[i].at - at).abs())
                };
                if better {
                    found = Some(j);
                }
            }
        }
        let Some(i) = found else {
            self.add(id.map(Into::into), what, step, at);
            return None;
        };
        let call = self.calls[i].clone();
        self.calls[i] = Call {
            id: call.id.clone().or_else(|| id.map(Into::into)),
            what: call.what,
            step: call.step.max(step),
            at: if step == 1 { at } else { call.at },
        };
        match step {
            1 => (call.step > 1).then_some(true),
            3 => {
                // the call that asked is done: no rerun waits on its request
                if by_id && call.step == 2 {
                    self.calls
                        .retain(|c| !(c.id.is_none() && c.what == what && c.step == 2));
                }
                None
            }
            _ => {
                // it may be for a rerun whose PreToolUse is still to come: that one then comes late, like its own
                if !by_id {
                    self.add(None, what, 2, at);
                }
                (call.step == 1 && call.at == latest).then_some(false)
            }
        }
    }

    fn add(&mut self, id: Option<String>, what: u64, step: u8, at: f64) {
        self.calls.push(Call { id, what, step, at });
        if self.calls.len() > CALLS {
            self.calls.remove(0);
        }
    }
}

/// A UUIDv7 (xxxxxxxx-xxxx-7xxx-...), whose text sorts by when it was made: 36 characters with a 7 where the version
/// goes, which `Guid.TryParseExact(…, "D")` takes (ASCII only, so its bytes are its UTF-16 units).
fn is_v7(id: &str) -> bool {
    id.len() == 36 && id.as_bytes()[14] == b'7' && is_guid_d(id.as_bytes())
}

/// Whether UUIDv7 `a` was made before `b`. Not when `b`'s time is well past `now`: the clock was set back since, and
/// ids made after that sort before it. The time is read as the C#'s `Convert.ToInt64(…, 16)` reads it, which throws
/// on the forms with a `+` or `0x` inside that the Guid parser takes.
pub(super) fn v7_before(a: &str, b: &str, now: f64) -> Result<bool, Thrown> {
    if !is_v7(a) || !is_v7(b) {
        return Ok(false);
    }
    let made = hex_int64(&format!("{}{}", &b[..8], &b[9..13]))?;
    // StringComparison.OrdinalIgnoreCase, on ASCII
    let upper = |id: &str| id.bytes().map(|c| c.to_ascii_uppercase()).collect::<Vec<u8>>();
    Ok(made as f64 / 1000.0 <= now + 5.0 && upper(a) < upper(b))
}

/// `Convert.ToInt64(text, 16)`: hex digits after an optional `+` and then an optional `0x`, and nothing else, which
/// is a FormatException.
fn hex_int64(text: &str) -> Result<i64, Thrown> {
    let digits = text.strip_prefix('+').unwrap_or(text);
    let digits = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
        .unwrap_or(digits);
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(Thrown::FORMAT);
    }
    // at most 12 digits here, as the Guid's first two groups hold
    u64::from_str_radix(digits, 16)
        .ok()
        .and_then(|n| i64::try_from(n).ok())
        .ok_or(Thrown::FORMAT)
}
