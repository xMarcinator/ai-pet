//! "Import defaults…" in Settings reads a team's preset file: Jira and GitHub settings, never credentials. Each case
//! of the C#'s `tests/AiPet.Tests/PresetTests.cs`, with its files and answers; the refusals are checked whole, and
//! the line a file that isn't JSON is refused at is the one .NET 10 names for the same file.

use aipet_core::config::{DEFAULT_JQL, GitHubSettings, JiraSettings};
use aipet_core::presets::Preset;

const FULL: &str = r#"{
  "version": 1,
  "jira": { "site": " team.atlassian.net ", "jql": "Reviewer = currentUser() AND status = Review", "enabled": true },
  "github": { "host": "github.example.com", "orgs": ["acme", " tools ", ""], "jiraProjects": ["ABC"], "enabled": true }
}"#;

fn mine() -> JiraSettings {
    JiraSettings {
        enabled: false,
        site: Some("old.atlassian.net".into()),
        email: Some("me@example.com".into()),
        jql: Some("project = X".into()),
        poll_seconds: 300,
    }
}

fn my_github() -> GitHubSettings {
    GitHubSettings {
        enabled: false,
        host: Some("github.com".into()),
        orgs: names(&["mine"]),
        jira_projects: names(&["OLD"]),
        poll_seconds: 600,
    }
}

fn names(list: &[&str]) -> Option<Vec<Option<String>>> {
    Some(list.iter().map(|name| Some(name.to_string())).collect())
}

fn text(s: &str) -> Option<String> {
    Some(s.into())
}

#[test]
fn a_full_preset_sets_every_field_and_keeps_the_users_own() {
    let p = Preset::parse(FULL).unwrap();
    assert!(p.has_jira());
    assert!(p.has_github());

    let j = p.apply_to_jira(&mine());
    assert_eq!(
        (j.enabled, j.site, j.jql),
        (
            true,
            text("team.atlassian.net"),
            text("Reviewer = currentUser() AND status = Review")
        )
    );
    assert_eq!((j.email, j.poll_seconds), (text("me@example.com"), 300));

    let g = p.apply_to_github(&my_github());
    assert_eq!((g.enabled, g.host), (true, text("github.example.com")));
    assert_eq!(g.orgs, names(&["acme", "tools"]));
    assert_eq!(g.jira_projects, names(&["ABC"]));
    assert_eq!(g.poll_seconds, 600);
    assert_eq!(
        p.describe(),
        "Jira (site, search, on) and GitHub (host, 2 organisations, 1 Jira project, on)"
    );
}

#[test]
fn a_partial_preset_changes_only_what_it_has() {
    let p = Preset::parse(r#"{"version":1,"github":{"orgs":["acme"]},"jira":null}"#).unwrap();
    assert!(!p.has_jira());
    assert!(p.has_github());
    let g = p.apply_to_github(&my_github());
    assert_eq!((g.enabled, g.host, g.poll_seconds), (false, text("github.com"), 600));
    assert_eq!(g.orgs, names(&["acme"]));
    assert_eq!(g.jira_projects, names(&["OLD"]));
    assert_eq!(p.describe(), "GitHub (1 organisation)");

    let j = Preset::parse(r#"{"version":1,"jira":{"enabled":false}}"#)
        .unwrap()
        .apply_to_jira(&mine());
    assert_eq!(
        (j.enabled, j.site, j.email, j.jql),
        (
            false,
            text("old.atlassian.net"),
            text("me@example.com"),
            text("project = X")
        )
    );
}

#[test]
fn blank_fields_mean_the_defaults_as_settings_save_does() {
    let p = Preset::parse(r#"{"version":1,"jira":{"jql":"  "},"github":{"host":""}}"#).unwrap();
    assert_eq!(p.apply_to_jira(&mine()).jql, text(DEFAULT_JQL));
    let ghe = GitHubSettings {
        host: text("ghe.example.com"),
        ..GitHubSettings::default()
    };
    assert_eq!(p.apply_to_github(&ghe).host, text("github.com"));
}

#[test]
fn no_sections_is_a_valid_preset_with_nothing_in_it() {
    let p = Preset::parse(r#"{"version":1,"jira":{},"theme":"dark"}"#).unwrap();
    assert!(!(p.has_jira() || p.has_github()));
    assert_eq!(p.describe(), "nothing");
}

#[test]
fn tokens_and_email_are_never_read() {
    // even of a type that would be an error for a field that is read
    let p = Preset::parse(
        r#"
        {"version":1,"token":"t0",
         "jira":{"site":"team.atlassian.net","email":"boss@example.com","token":"t1","apiToken":42},
         "github":{"token":"ghp_x","accessToken":["x"],"orgs":[]}}
        "#,
    )
    .unwrap();
    let j = p.apply_to_jira(&mine());
    assert_eq!(j.email, text("me@example.com"));
    assert_eq!(p.apply_to_github(&my_github()).orgs, Some(Vec::new()));
    // nothing in a preset could hold one
    let fields = format!("{:?}", Preset::default()).to_lowercase();
    for word in ["token", "email", "secret"] {
        assert!(!fields.contains(word), "{fields}");
    }
}

#[test]
fn a_file_that_isnt_a_preset_is_refused_with_a_reason() {
    for (json, reason) in [
        (
            r#"{"jira":{"site":"x"}}"#,
            r#"The file isn't an AiPet preset: it has no "version"."#,
        ),
        (
            r#"{"version":2}"#,
            "The preset is version 2; this AiPet reads version 1.",
        ),
        (r#"{"version":"1"}"#, r#""version" must be a whole number, e.g. 1."#),
        (r#"{"version":1.5}"#, r#""version" must be a whole number, e.g. 1."#),
        ("[1]", "The file isn't an AiPet preset."),
        (r#"{"version":1,"#, "The file isn't valid JSON (line 1)."),
        (
            r#"{"version":1,"jira":[]}"#,
            r#""jira" must be a section in braces ({ … })."#,
        ),
        (
            r#"{"version":1,"jira":{"site":5}}"#,
            r#""jira.site" must be text in quotes."#,
        ),
        (
            r#"{"version":1,"jira":{"enabled":"yes"}}"#,
            r#""jira.enabled" must be true or false."#,
        ),
        (
            r#"{"version":1,"github":{"orgs":"acme, tools"}}"#,
            r#""github.orgs" must be a list of names in quotes, e.g. ["one", "two"]."#,
        ),
        (
            r#"{"version":1,"github":{"jiraProjects":["ABC",7]}}"#,
            r#""github.jiraProjects" must be a list of names in quotes, e.g. ["one", "two"]."#,
        ),
        (
            r#"{"version":1,"github":{"host":true}}"#,
            r#""github.host" must be text in quotes."#,
        ),
        // beyond the C#'s cases: what it says for these too
        (r#"{"version":null}"#, r#""version" must be a whole number, e.g. 1."#),
        (
            r#"{"version":3000000000}"#,
            r#""version" must be a whole number, e.g. 1."#,
        ),
        (
            r#"{"version":1,"github":[]}"#,
            r#""github" must be a section in braces ({ … })."#,
        ),
    ] {
        assert_eq!(Preset::parse(json).unwrap_err(), reason, "{json}");
    }
}

#[test]
fn json_is_read_as_the_csharp_reads_it() {
    // comments, trailing commas and a byte order mark are fine
    let p = Preset::parse(
        "\u{FEFF}{\"version\":1, /* the team's */ \"jira\":{\"site\":\"a.example\",},\n// GitHub\n\"github\":{\"orgs\":[\"x\",],},}",
    )
    .unwrap();
    assert_eq!(
        (p.jira_site.as_deref(), p.github_orgs),
        (Some("a.example"), Some(vec!["x".to_string()]))
    );
    // a file that isn't JSON: the line .NET 10's JsonDocument names for the same file
    let deep = format!("{}{}", "[".repeat(65), "]".repeat(65));
    for (json, line) in [
        ("{\"version\":1,\n", 2),
        ("{\n\"version\":1,,\n}", 2),
        ("[,]", 1),
        ("{,}", 1),
        ("[1 2]", 1),
        ("{\n /* x", 2),
        ("{\"a\":1}\n/* end", 2),
        ("{\n // c\n \"a\": tru\n}", 3),
        ("{\"a\":\"x\ny\"}", 1),
        ("\n\n{\"a\":1}}", 3),
        ("", 1),
        (deep.as_str(), 1),
    ] {
        let refused = format!("The file isn't valid JSON (line {line}).");
        assert_eq!(Preset::parse(json).unwrap_err(), refused, "{json:?}");
    }
}

// Settings removes a saved token before a preset points it at another site or host, so a shared file can't have
// your credentials sent where it likes.
#[test]
fn a_preset_that_points_jira_elsewhere_is_a_move() {
    for (json, moves) in [
        (r#"{"version":1,"jira":{"site":"evil.example"}}"#, true),
        (r#"{"version":1,"jira":{"site":"https://Old.Atlassian.net/"}}"#, false),
        (r#"{"version":1,"jira":{"site":" old.atlassian.net "}}"#, false),
        (r#"{"version":1,"jira":{"site":""}}"#, false),
        (r#"{"version":1,"jira":{"jql":"project = Y","enabled":true}}"#, false),
    ] {
        assert_eq!(Preset::parse(json).unwrap().moves_jira(&mine()), moves, "{json}");
    }
}

#[test]
fn a_preset_that_points_github_elsewhere_is_a_move() {
    for (json, moves) in [
        (r#"{"version":1,"github":{"host":"evil.example"}}"#, true),
        (r#"{"version":1,"github":{"host":"GitHub.com"}}"#, false),
        (r#"{"version":1,"github":{"host":""}}"#, false),
        (r#"{"version":1,"github":{"orgs":["acme"],"enabled":true}}"#, false),
    ] {
        assert_eq!(Preset::parse(json).unwrap().moves_github(&my_github()), moves, "{json}");
    }
}

#[test]
fn a_first_site_is_a_move_too() {
    // a token saved before any site was typed would otherwise go to whatever site the file names
    let p = Preset::parse(r#"{"version":1,"jira":{"site":"team.atlassian.net"}}"#).unwrap();
    assert!(p.moves_jira(&JiraSettings::default()));
}
