//! Observes merged branches without blocking workspace polling or modifying repository refs.
use super::*;

// Only a coding task has a branch of its own that can merge.
fn eligible(task: &Task) -> bool {
    task.kind == TaskKind::Coding
        && !task.archived
        && task.settled.is_none()
        && !task.state.active()
        && !task.needs_input()
        && task.state != RunState::DeliveryUnknown
}

/// Refreshes automatic settlement from Git, preserving edits made during the observation.
pub(super) fn refresh(host: &Shared) -> Result<()> {
    let (projects, tasks) = {
        let h = host.lock().unwrap();
        (h.store.projects()?, h.store.tasks()?)
    };
    for task in tasks.into_iter().filter(eligible) {
        let Some(project) = projects.iter().find(|p| p.id == task.project_id) else {
            continue;
        };
        let merged = git::branch_merged(&project.path, &task);
        let h = host.lock().unwrap();
        let Ok(mut current) = h.store.task(&task.id) else {
            continue;
        };
        if eligible(&current)
            && current.updated == task.updated
            && current.last_started == task.last_started
            && current.branch == task.branch
            && current.cwd == task.cwd
            && current.baseline == task.baseline
            && current.branch_merged != merged
        {
            current.branch_merged = merged;
            h.store.save_task(&current)?;
        }
    }
    Ok(())
}
