//! The active tasks as the feed left them: a snapshot, then its changes in order (FNT-64's resume rule).

use std::collections::BTreeMap;

use super::api::{Change, FeedMessage, Point};
use super::wire::TrackerTask;

/// The active tasks by id, and the point of the feed they are at.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskView {
    tasks: BTreeMap<String, TrackerTask>,
    /// None until the first snapshot.
    at: Option<Point>,
}

/// What applying a message did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    /// The tasks changed.
    Changed,
    /// Nothing to do: a change already applied, or one that changes no task.
    Unchanged,
    /// A change of another feed (the tracker restarted, or another pod): the view needs a snapshot.
    Resync,
}

impl TaskView {
    /// Where to resume the feed from: none before the first snapshot.
    pub fn point(&self) -> Option<&Point> {
        self.at.as_ref()
    }

    pub fn tasks(&self) -> impl Iterator<Item = &TrackerTask> {
        self.tasks.values()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Forgets the feed point and keeps the tasks shown until a snapshot replaces them: for a feed that moved on.
    pub fn forget_point(&mut self) {
        self.at = None;
    }

    /// Forgets every task and the feed point: the next message must be a snapshot.
    pub fn clear(&mut self) {
        *self = TaskView::default();
    }

    pub fn apply(&mut self, message: FeedMessage) -> Applied {
        match message {
            FeedMessage::Snapshot { at, tasks } => {
                let tasks: BTreeMap<_, _> = tasks.into_iter().map(|t| (t.id.clone(), t)).collect();
                let changed = tasks != self.tasks;
                self.tasks = tasks;
                self.at = Some(at);
                if changed { Applied::Changed } else { Applied::Unchanged }
            }
            FeedMessage::Change { at, change } => {
                let Some(point) = self.at.as_mut() else {
                    return Applied::Resync;
                };
                if at.feed != point.feed {
                    return Applied::Resync;
                }
                if at.seq <= point.seq {
                    return Applied::Unchanged;
                }
                point.seq = at.seq;
                match change {
                    Change::Upsert(task) => {
                        let task = *task;
                        if self.tasks.get(&task.id) == Some(&task) {
                            return Applied::Unchanged;
                        }
                        self.tasks.insert(task.id.clone(), task);
                        Applied::Changed
                    }
                    Change::Remove(id) => match self.tasks.remove(&id) {
                        Some(_) => Applied::Changed,
                        None => Applied::Unchanged,
                    },
                    Change::Other(_) => Applied::Unchanged,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, phase: &str) -> TrackerTask {
        TrackerTask {
            id: id.into(),
            short_id: id.to_uppercase(),
            phase: Some(phase.into()),
            ..TrackerTask::default()
        }
    }

    fn at(feed: &str, seq: u64) -> Point {
        Point { feed: feed.into(), seq }
    }

    fn snapshot(feed: &str, seq: u64, tasks: Vec<TrackerTask>) -> FeedMessage {
        FeedMessage::Snapshot {
            at: at(feed, seq),
            tasks,
        }
    }

    fn change(feed: &str, seq: u64, change: Change) -> FeedMessage {
        FeedMessage::Change {
            at: at(feed, seq),
            change,
        }
    }

    fn phases(view: &TaskView) -> Vec<(String, String)> {
        view.tasks()
            .map(|t| (t.id.clone(), t.phase.clone().unwrap_or_default()))
            .collect()
    }

    #[test]
    fn a_snapshot_then_its_changes_in_order() {
        let mut view = TaskView::default();
        assert_eq!(view.point(), None);
        assert_eq!(
            view.apply(snapshot("f1", 3, vec![task("a", "planning")])),
            Applied::Changed
        );
        let up = |seq, phase| change("f1", seq, Change::Upsert(Box::new(task("a", phase))));
        assert_eq!(view.apply(up(4, "testing")), Applied::Changed);
        // an old or repeated change is skipped
        assert_eq!(view.apply(up(4, "fixing")), Applied::Unchanged);
        assert_eq!(view.apply(up(2, "fixing")), Applied::Unchanged);
        assert_eq!(phases(&view), [("a".into(), "testing".into())]);
        assert_eq!(
            view.apply(change("f1", 5, Change::Other("run.started".into()))),
            Applied::Unchanged
        );
        assert_eq!(view.point(), Some(&at("f1", 5)));
        assert_eq!(
            view.apply(change("f1", 6, Change::Remove("a".into()))),
            Applied::Changed
        );
        assert!(view.is_empty());
        assert_eq!(
            view.apply(change("f1", 7, Change::Remove("a".into()))),
            Applied::Unchanged
        );
    }

    #[test]
    fn another_feed_or_no_snapshot_asks_for_one() {
        let mut view = TaskView::default();
        let up = |feed| change(feed, 9, Change::Upsert(Box::new(task("a", "testing"))));
        assert_eq!(view.apply(up("f1")), Applied::Resync);
        view.apply(snapshot("f1", 3, vec![]));
        assert_eq!(view.apply(up("f2")), Applied::Resync);
        // a snapshot of the other feed replaces the view whole
        assert_eq!(
            view.apply(snapshot("f2", 1, vec![task("b", "planning")])),
            Applied::Changed
        );
        assert_eq!(phases(&view), [("b".into(), "planning".into())]);
        assert_eq!(
            view.apply(snapshot("f2", 1, vec![task("b", "planning")])),
            Applied::Unchanged
        );
        view.clear();
        assert!(view.is_empty() && view.point().is_none());
    }
}
