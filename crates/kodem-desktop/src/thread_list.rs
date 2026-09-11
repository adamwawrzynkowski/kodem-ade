//! Lightweight, stable rows shared by flat and project-grouped thread navigation.
use kodem_core::{RunState, Settings, Task, Workspace};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum Row {
    Project {
        index: usize,
        id: String,
        count: usize,
    },
    Task {
        index: usize,
        project: usize,
        id: String,
    },
    Settled {
        project: Option<String>,
        count: usize,
    },
    Empty(&'static str),
}
impl Row {
    /// Compares row identities independently of counts and host ordering.
    pub fn same_item(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Project { id: a, .. }, Self::Project { id: b, .. })
            | (Self::Task { id: a, .. }, Self::Task { id: b, .. }) => a == b,
            (Self::Settled { project: a, .. }, Self::Settled { project: b, .. }) => a == b,
            (Self::Empty(a), Self::Empty(b)) => a == b,
            _ => false,
        }
    }
}

pub struct Filter<'a> {
    pub project: Option<&'a str>,
    pub query: &'a str,
    pub archived: bool,
    pub settled_collapsed: bool,
    pub settled_projects: &'a std::collections::HashSet<String>,
}

/// Filters and sorts task identities without cloning drafts, questions or captured context.
pub fn rows(workspace: &Workspace, settings: &Settings, filter: Filter<'_>) -> Vec<Row> {
    let projects: HashMap<_, _> = workspace
        .projects
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), (i, p.name.to_lowercase())))
        .collect();
    let mut tasks: Vec<_> = workspace
        .tasks
        .iter()
        .enumerate()
        .filter_map(|(index, task)| {
            let (project, name) = projects.get(task.project_id.as_str())?;
            (task.archived == filter.archived
                && filter.project.is_none_or(|id| id == task.project_id)
                && (filter.query.is_empty()
                    || task.title.to_lowercase().contains(filter.query)
                    || task.branch.to_lowercase().contains(filter.query)
                    || name.contains(filter.query)))
            .then_some((index, *project, task))
        })
        .collect();
    tasks.sort_by(|(_, _, a), (_, _, b)| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| match settings.sort {
                kodem_core::Sort::Recent => b
                    .last_started
                    .unwrap_or(b.updated)
                    .cmp(&a.last_started.unwrap_or(a.updated)),
                kodem_core::Sort::Created => b.created.cmp(&a.created),
                kodem_core::Sort::Name => a.title.cmp(&b.title),
                kodem_core::Sort::Attention => rank(a).cmp(&rank(b)),
            })
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut rows = Vec::new();
    if settings.group_tasks_by_project {
        let mut groups: HashMap<usize, Vec<_>> = HashMap::new();
        for task in tasks {
            groups.entry(task.1).or_default().push(task);
        }
        // Running projects lead the feed; the latest start wins without selection affecting order.
        let mut project_order: Vec<_> = groups
            .iter()
            .map(|(&index, tasks)| {
                let started = tasks
                    .iter()
                    .filter(|(_, _, task)| task.state.active())
                    .map(|(_, _, task)| task.last_started.unwrap_or(task.updated))
                    .max();
                (index, started)
            })
            .collect();
        project_order.sort_by(|(a, started_a), (b, started_b)| {
            started_b.cmp(started_a).then_with(|| a.cmp(b))
        });
        for (index, _) in project_order {
            let project = &workspace.projects[index];
            let Some(tasks) = groups.remove(&index) else {
                continue;
            };
            rows.push(Row::Project {
                index,
                id: project.id.clone(),
                count: tasks.len(),
            });
            if settings.collapsed_projects.contains(&project.id) && filter.query.is_empty() {
                continue;
            }
            append_tasks(
                &mut rows,
                tasks,
                Some(project.id.clone()),
                !filter.settled_projects.contains(&project.id) || !filter.query.is_empty(),
            );
        }
    } else {
        append_tasks(
            &mut rows,
            tasks,
            None,
            !filter.settled_collapsed || !filter.query.is_empty(),
        );
    }
    if rows.is_empty() {
        rows.push(Row::Empty(if workspace.projects.is_empty() {
            "Open a repository to start a task."
        } else if !filter.query.is_empty() {
            "No matching tasks."
        } else {
            "No tasks here yet. Start a new task above."
        }));
    }
    rows
}

fn rank(task: &Task) -> u8 {
    if task.needs_input() {
        return 0;
    }
    match task.state {
        RunState::Waiting => 0,
        RunState::Failed | RunState::DeliveryUnknown => 1,
        RunState::Running | RunState::Starting => 2,
        RunState::Idle => 3,
        _ => 4,
    }
}

fn append_tasks(
    rows: &mut Vec<Row>,
    tasks: Vec<(usize, usize, &Task)>,
    project: Option<String>,
    show_settled: bool,
) {
    let (open, settled): (Vec<_>, Vec<_>) = tasks
        .into_iter()
        .partition(|(_, _, task)| !task.is_settled() || (task.settled.is_none() && task.pinned));
    let row = |(index, project, task): (usize, usize, &Task)| Row::Task {
        index,
        project,
        id: task.id.clone(),
    };
    rows.extend(open.into_iter().map(row));
    if !settled.is_empty() {
        rows.push(Row::Settled {
            project,
            count: settled.len(),
        });
        if show_settled {
            rows.extend(settled.into_iter().map(row));
        }
    }
}
