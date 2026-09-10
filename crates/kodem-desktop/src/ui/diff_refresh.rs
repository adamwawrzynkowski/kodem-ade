//! Refresh scheduling and snapshot replacement for live repository comparisons.
use super::*;
use std::time::Instant;

#[derive(Default)]
pub(super) struct DiffRefresh {
    pending: usize,
    requested_at: Option<Instant>,
}
impl DiffRefresh {
    /// Allows one periodic refresh at a time, no more often than every two seconds.
    pub(super) fn due(&self) -> bool {
        self.pending == 0
            && self
                .requested_at
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(2))
    }
    /// Releases an outstanding capture even when its task or scope is no longer selected.
    pub(super) fn completed(&mut self) {
        self.pending = self.pending.saturating_sub(1);
    }
}

/// Rejects replies for an old scope or a live capture superseded by opening a saved snapshot.
pub(super) fn accepts_reply(
    request: &core::Request,
    task: Option<&str>,
    comparison: &Comparison,
    historical: bool,
) -> bool {
    match request {
        core::Request::CaptureDiff {
            task_id,
            comparison: requested,
            ..
        } => !historical && task == Some(task_id.as_str()) && requested == comparison,
        core::Request::LoadSnapshot { .. } => historical,
        _ => true,
    }
}

impl Shell {
    /// Refreshes prompt totals independently of the review panel's current scope.
    pub(super) fn refresh_task_changes(&mut self) {
        let Some(task_id) = self.selected.clone() else {
            return;
        };
        if self
            .tx
            .send(bridge::Command::Request(core::Request::TaskChanges {
                task_id,
            }))
            .is_ok()
        {
            self.changes_refresh.pending += 1;
            self.changes_refresh.requested_at = Some(Instant::now());
        }
    }

    /// Requests current repository content, giving the host the latest known snapshot to reuse.
    pub(super) fn refresh_diff(&mut self) {
        let Some(task_id) = self.selected.clone() else {
            return;
        };
        self.historical_diff = false;
        let previous = self
            .new_diff
            .as_ref()
            .or(self.diff.as_ref())
            .filter(|diff| diff.task_id == task_id && diff.comparison == self.comparison)
            .map(|diff| diff.id.clone());
        let request = core::Request::CaptureDiff {
            task_id,
            comparison: self.comparison.clone(),
            previous,
        };
        if self.tx.send(bridge::Command::Request(request)).is_ok() {
            self.diff_refresh.pending += 1;
            self.diff_refresh.requested_at = Some(Instant::now());
        }
    }

    /// Replaces a displayed snapshot while keeping the same selected file whenever it still exists.
    pub(super) fn install_diff(&mut self, diff: DiffSnapshot) {
        let selected_path = self
            .diff
            .as_ref()
            .and_then(|old| old.files.get(self.file))
            .map(|file| &file.path);
        if self.reviewing_file {
            if let Some(index) = diff
                .files
                .iter()
                .position(|file| Some(&file.path) == selected_path)
            {
                self.file = index;
            } else {
                self.file = 0;
                self.reviewing_file = false;
            }
        }
        self.diff = Some(diff);
        self.new_diff = None;
        self.diff_view = None;
        self.diff_subscription = None;
        self.text_views.retain(|key, _| !key.starts_with("diff:"));
    }
}
