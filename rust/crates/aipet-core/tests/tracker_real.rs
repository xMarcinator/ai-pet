//! The tracker watcher against a real flow-next tracker, by hand (fnt-81's hands-on pass). Ignored unless run:
//!
//! ```text
//! AIPET_TRACKER_URL=http://127.0.0.1:3299 AIPET_TRACKER_CODE=<companion code> \
//!     cargo test -p aipet-core --test tracker_real -- --ignored --nocapture
//! ```
//!
//! It enrolls with the code, waits until it's connected, and prints what it follows for `AIPET_TRACKER_WATCH`
//! seconds (default 20): the status, and each task as its bubble would say it. With `AIPET_TRACKER_EXPECT_TASK` set
//! to a task id it fails unless that task shows up, and with `AIPET_TRACKER_EXPECT_REVOKED=1` unless the pet is
//! revoked before the time is up. With `AIPET_TRACKER_STOP` set to a run id, once connected with actions allowed it
//! asks the tracker to stop that run and prints what came back. The credential is kept in memory and the settings in
//! a folder of its own.

use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use aipet_core::board::{task_detail, task_name};
use aipet_core::secrets::{MemorySecrets, SecretStore};
use aipet_core::tracker::{Action, Event, Status, TrackerWatcher};

#[test]
#[ignore = "needs a real tracker: AIPET_TRACKER_URL and AIPET_TRACKER_CODE"]
fn follows_a_real_tracker() {
    let url = std::env::var("AIPET_TRACKER_URL").expect("AIPET_TRACKER_URL");
    let code = std::env::var("AIPET_TRACKER_CODE").expect("AIPET_TRACKER_CODE");
    let watch = std::env::var("AIPET_TRACKER_WATCH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let expect_task = std::env::var("AIPET_TRACKER_EXPECT_TASK").ok();
    let expect_revoked = std::env::var("AIPET_TRACKER_EXPECT_REVOKED").is_ok_and(|v| v == "1");
    let mut stop = std::env::var("AIPET_TRACKER_STOP").ok();
    let dir = std::env::temp_dir().join(format!("aipet-tracker-real-{}", std::process::id()));
    let (tx, _rx) = mpsc::channel::<Event>();
    let watcher = TrackerWatcher::new(&dir, Arc::new(MemorySecrets::default()) as Arc<dyn SecretStore>, tx);

    watcher.enroll(&url, &code).expect("enrolled");
    println!("enrolled at {url}");
    let deadline = Instant::now() + Duration::from_secs(watch);
    let (mut seen_task, mut revoked, mut last) = (false, false, String::new());
    while Instant::now() < deadline {
        let status = watcher.status();
        let tasks: Vec<String> = watcher
            .tasks()
            .iter()
            .map(|t| {
                seen_task |= expect_task.as_deref() == Some(t.id.as_str());
                format!(
                    "{} | {} | tone {:?} | subtasks {}/{} | criteria {}/{} | agents {} | sessions {:?}",
                    task_name(t),
                    task_detail(t),
                    t.tone,
                    t.subtasks.done,
                    t.subtasks.total,
                    t.acceptance.done,
                    t.acceptance.total,
                    t.agents.total,
                    t.sessions
                )
            })
            .collect();
        let now = format!("{status:?}\n  {}", tasks.join("\n  "));
        if now != last {
            println!("{now}");
            last = now;
        }
        revoked |= status == Status::Revoked;
        if status == (Status::Connected { actions_allowed: true })
            && let Some(run) = stop.take()
        {
            let outcome = watcher.act(&Action::Stop { run: run.clone() });
            println!("stop {run}: {outcome:?}");
        }
        // with something to expect, it stops once all of it came
        let expecting = expect_task.is_some() || expect_revoked;
        if expecting && (expect_task.is_none() || seen_task) && (!expect_revoked || revoked) {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    let _ = std::fs::remove_dir_all(dir);
    if let Some(task) = expect_task {
        assert!(seen_task, "{task} never showed up");
    }
    if expect_revoked {
        assert!(revoked, "the pet was never revoked");
    }
}
