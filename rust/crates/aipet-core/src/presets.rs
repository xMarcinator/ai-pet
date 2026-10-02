//! Import defaults: a team's preset file for the Jira and GitHub settings, which the user picks in Settings
//! (`src/AiPet.Core/Presets.cs`):
//!
//! ```text
//! {"version":1,"jira":{"site":…,"jql":…,"enabled":…},"github":{"host":…,"orgs":[…],"jiraProjects":[…],"enabled":…}}
//! ```
//!
//! Every section and field is optional, unknown fields are skipped, and comments and trailing commas are allowed, as
//! the C# reads the file. A file that isn't a preset this app reads is refused with the C#'s message for the user.
//!
//! A preset never carries credentials: tokens and the Jira email are each user's own, so no such field is read, even
//! when a file has one. A saved token goes wherever the settings point, so before a preset points Jira or GitHub at
//! another address, Settings forgets the token saved for the old one ([`Preset::moves_jira`],
//! [`Preset::moves_github`]).

use serde_json::{Map, Value};

use crate::config::{DEFAULT_JQL, GitHubSettings, JiraSettings};
use crate::http::{address, equals_ignoring_case};

/// What a preset file sets; `None` for what it leaves as it is. Nothing in it can hold a token or an email.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preset {
    pub jira_site: Option<String>,
    pub jira_jql: Option<String>,
    pub jira_enabled: Option<bool>,
    pub github_host: Option<String>,
    /// The names, trimmed, without empty ones.
    pub github_orgs: Option<Vec<String>>,
    /// The names, trimmed, without empty ones.
    pub github_jira_projects: Option<Vec<String>>,
    pub github_enabled: Option<bool>,
}

impl Preset {
    /// Reads a preset file's text. A file that isn't a preset this app reads gives the message for the user.
    pub fn parse(json: &str) -> Result<Preset, String> {
        let root = read(json)?;
        let Value::Object(root) = root else {
            return Err("The file isn't an AiPet preset.".into());
        };
        // `version` is read as it is, so `"version": null` is the wrong type rather than none
        let Some(version) = root.get("version") else {
            return Err("The file isn't an AiPet preset: it has no \"version\".".into());
        };
        let version = version
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .ok_or("\"version\" must be a whole number, e.g. 1.")?;
        if version != 1 {
            return Err(format!("The preset is version {version}; this AiPet reads version 1."));
        }

        let mut p = Preset::default();
        if let Some(jira) = section(&root, "jira")? {
            p.jira_site = text(jira, "jira", "site")?;
            p.jira_jql = text(jira, "jira", "jql")?;
            p.jira_enabled = switch(jira, "jira", "enabled")?;
        }
        if let Some(github) = section(&root, "github")? {
            p.github_host = text(github, "github", "host")?;
            p.github_orgs = names(github, "github", "orgs")?;
            p.github_jira_projects = names(github, "github", "jiraProjects")?;
            p.github_enabled = switch(github, "github", "enabled")?;
        }
        Ok(p)
    }

    pub fn has_jira(&self) -> bool {
        self.jira_site.is_some() || self.jira_jql.is_some() || self.jira_enabled.is_some()
    }

    pub fn has_github(&self) -> bool {
        self.github_host.is_some()
            || self.github_orgs.is_some()
            || self.github_jira_projects.is_some()
            || self.github_enabled.is_some()
    }

    /// The Jira settings with the preset's fields in place of the current ones, tidied as Settings' Save does. The
    /// email and the poll interval stay the user's.
    pub fn apply_to_jira(&self, current: &JiraSettings) -> JiraSettings {
        JiraSettings {
            enabled: self.jira_enabled.unwrap_or(current.enabled),
            site: match &self.jira_site {
                Some(site) => Some(site.trim().into()),
                None => current.site.clone(),
            },
            email: current.email.clone(),
            jql: match &self.jira_jql {
                None => current.jql.clone(),
                Some(jql) if jql.trim().is_empty() => Some(DEFAULT_JQL.into()),
                Some(jql) => Some(jql.trim().into()),
            },
            poll_seconds: current.poll_seconds,
        }
    }

    /// The GitHub settings with the preset's fields in place of the current ones, tidied as Settings' Save does.
    pub fn apply_to_github(&self, current: &GitHubSettings) -> GitHubSettings {
        let list = |names: &Option<Vec<String>>, current: &Option<Vec<Option<String>>>| match names {
            Some(names) => Some(names.iter().cloned().map(Some).collect()),
            None => current.clone(),
        };
        GitHubSettings {
            enabled: self.github_enabled.unwrap_or(current.enabled),
            host: match &self.github_host {
                None => current.host.clone(),
                Some(host) if host.trim().is_empty() => Some("github.com".into()),
                Some(host) => Some(host.trim().into()),
            },
            orgs: list(&self.github_orgs, &current.orgs),
            jira_projects: list(&self.github_jira_projects, &current.jira_projects),
            poll_seconds: current.poll_seconds,
        }
    }

    /// Whether the preset points Jira at another site than the saved one. A saved token goes wherever the settings
    /// point, so Settings removes it first rather than send it to a site the user never typed in.
    pub fn moves_jira(&self, current: &JiraSettings) -> bool {
        self.jira_site.is_some() && moves(&self.apply_to_jira(current).site, &current.site)
    }

    /// Whether the preset points GitHub at another host than the saved one (see [`Preset::moves_jira`]).
    pub fn moves_github(&self, current: &GitHubSettings) -> bool {
        self.github_host.is_some() && moves(&self.apply_to_github(current).host, &current.host)
    }

    /// What it sets, for the line Settings shows after an import, e.g. "Jira (site, search) and GitHub (2
    /// organisations)".
    pub fn describe(&self) -> String {
        fn count(n: usize, one: &str) -> String {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {one}s")
            }
        }
        fn on(v: Option<bool>) -> Option<String> {
            v.map(|on| if on { "on" } else { "off" }.into())
        }
        fn part(name: &str, fields: Vec<Option<String>>) -> Option<String> {
            let fields: Vec<String> = fields.into_iter().flatten().collect();
            (!fields.is_empty()).then(|| format!("{name} ({})", fields.join(", ")))
        }
        let jira = part(
            "Jira",
            vec![
                self.jira_site.as_ref().map(|_| "site".into()),
                self.jira_jql.as_ref().map(|_| "search".into()),
                on(self.jira_enabled),
            ],
        );
        let github = part(
            "GitHub",
            vec![
                self.github_host.as_ref().map(|_| "host".into()),
                self.github_orgs.as_ref().map(|o| count(o.len(), "organisation")),
                self.github_jira_projects
                    .as_ref()
                    .map(|p| count(p.len(), "Jira project")),
                on(self.github_enabled),
            ],
        );
        let parts: Vec<String> = [jira, github].into_iter().flatten().collect();
        if parts.is_empty() {
            "nothing".into()
        } else {
            parts.join(" and ")
        }
    }
}

/// Compared as the Jira watcher reads a site, ignoring case. No site at all sends a token nowhere, so that isn't a
/// move.
fn moves(to: &Option<String>, from: &Option<String>) -> bool {
    let to = address(to.as_deref().unwrap_or(""));
    let from = address(from.as_deref().unwrap_or(""));
    !to.is_empty() && !equals_ignoring_case(&to, &from)
}

// A null counts as left out, like a missing field.
fn get<'a>(o: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    o.get(name).filter(|v| !v.is_null())
}

fn section<'a>(root: &'a Map<String, Value>, name: &str) -> Result<Option<&'a Map<String, Value>>, String> {
    match get(root, name) {
        None => Ok(None),
        Some(Value::Object(section)) => Ok(Some(section)),
        Some(_) => Err(format!("\"{name}\" must be a section in braces ({{ … }}).")),
    }
}

fn text(o: &Map<String, Value>, section: &str, name: &str) -> Result<Option<String>, String> {
    match get(o, name) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("\"{section}.{name}\" must be text in quotes.")),
    }
}

fn switch(o: &Map<String, Value>, section: &str, name: &str) -> Result<Option<bool>, String> {
    match get(o, name) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(format!("\"{section}.{name}\" must be true or false.")),
    }
}

fn names(o: &Map<String, Value>, section: &str, name: &str) -> Result<Option<Vec<String>>, String> {
    let Some(value) = get(o, name) else {
        return Ok(None);
    };
    let wrong = || format!("\"{section}.{name}\" must be a list of names in quotes, e.g. [\"one\", \"two\"].");
    let Value::Array(items) = value else {
        return Err(wrong());
    };
    items
        .iter()
        .map(|item| item.as_str().map(str::trim).ok_or_else(wrong))
        .filter(|name| !matches!(name, Ok("")))
        .map(|name| name.map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

// ---------------------------------------------------------------------------------------------------------------
// The file, as `JsonDocument.Parse(json, { CommentHandling = Skip, AllowTrailingCommas = true })` reads it

/// The file's JSON, or the C#'s message with the line .NET names.
fn read(json: &str) -> Result<Value, String> {
    let invalid = |line: usize| format!("The file isn't valid JSON (line {line}).");
    // a StreamReader, which the C# reads the file with, drops the byte order mark
    let json = json.strip_prefix('\u{FEFF}').unwrap_or(json);
    let relaxed = relax(json).map_err(invalid)?;
    serde_json::from_str(&relaxed).map_err(|e| {
        // serde_json places an error it finds on a line break at the start of the next line; .NET on the line it
        // was reading (`tru` and a new line: the literal's line). The end of the text is where the text ends.
        let at_break = e.column() == 0 && e.line() > 1 && !e.is_eof();
        invalid(if at_break { e.line() - 1 } else { e.line() })
    })
}

/// The text with comments blanked and trailing commas dropped, so serde_json reads what .NET reads with comments
/// skipped and trailing commas allowed. Line breaks stay where they were, so an error's line is the file's. It fails,
/// with the line, where .NET does and serde_json wouldn't or would name another line: a comment that doesn't end, a
/// control character in a string, arrays and objects nested more than 64 deep.
fn relax(text: &str) -> Result<String, usize> {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut line, mut depth) = (0, 1, 0usize);
    // where a comma after a value was written: a closing bracket right after it makes it a trailing one
    let mut comma: Option<usize> = None;
    let mut after_value = false;
    while i < c.len() {
        match c[i] {
            '"' => {
                out.push('"');
                i += 1;
                while i < c.len() && c[i] != '"' {
                    if c[i] < ' ' {
                        return Err(line);
                    }
                    // an escape's second character is copied as it is, even a quote
                    let n = if c[i] == '\\' { 2 } else { 1 };
                    out.extend(c[i..(i + n).min(c.len())].iter());
                    i += n;
                }
                if i < c.len() {
                    out.push('"');
                    i += 1;
                }
                (comma, after_value) = (None, true);
                continue;
            }
            '/' if c.get(i + 1) == Some(&'/') => {
                while i < c.len() && c[i] != '\n' {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }
            '/' if c.get(i + 1) == Some(&'*') => {
                let Some(end) = c[i + 2..].windows(2).position(|w| w == ['*', '/']) else {
                    return Err(line + c[i..].iter().filter(|&&ch| ch == '\n').count());
                };
                for &ch in &c[i..i + end + 4] {
                    out.push(if ch == '\n' { '\n' } else { ' ' });
                    line += usize::from(ch == '\n');
                }
                i += end + 4;
                continue;
            }
            ',' => {
                comma = after_value.then_some(out.len());
                after_value = false;
            }
            '}' | ']' => {
                if let Some(at) = comma.take() {
                    out.replace_range(at..at + 1, " ");
                }
                depth = depth.saturating_sub(1);
                after_value = true;
            }
            '{' | '[' => {
                depth += 1;
                if depth > 64 {
                    return Err(line);
                }
                (comma, after_value) = (None, false);
            }
            ':' => (comma, after_value) = (None, false),
            '\n' => line += 1,
            ' ' | '\t' | '\r' => {}
            _ => (comma, after_value) = (None, true),
        }
        out.push(c[i]);
        i += 1;
    }
    Ok(out)
}
