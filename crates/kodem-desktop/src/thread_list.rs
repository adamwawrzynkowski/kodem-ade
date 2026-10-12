//! Lightweight, stable rows shared by flat and project-grouped thread navigation.
//!
//! The feed leads with what needs a person: tasks waiting for input or blocked by a failure,
//! then running tasks, then finished results that were not opened yet. Everything else follows
//! in the chosen order. Settled work leaves the feed for one list of its own, which is not
//! grouped by project, and a project without open tasks has no group.
use kodem_core::{Settings, Task, Workspace};
use std::collections::HashMap;

/// A group of tasks that share a state the user acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    NeedsYou,
    Working,
    Ready,
    Threads,
}
impl Section {
    /// Returns the header label of the section.
    pub fn label(self) -> &'static str {
        match self {
            Self::NeedsYou => "Needs you",
            Self::Working => "Working",
            Self::Ready => "Ready for review",
            Self::Threads => "Threads",
        }
    }
    /// Returns the section a task belongs to, from the most urgent state it is in.
    pub fn of(task: &Task, settings: &Settings) -> Self {
        if task.needs_input() || task.is_blocked() {
            Self::NeedsYou
        } else if task.state.active() {
            Self::Working
        } else if task.is_unread(settings.viewed.get(&task.id).copied()) {
            Self::Ready
        } else {
            Self::Threads
        }
    }
}

#[derive(Clone, Debug)]
pub enum Row {
    Section {
        section: Section,
        count: usize,
    },
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
    Empty(&'static str),
}
impl Row {
    /// Compares row identities independently of counts and host ordering.
    pub fn same_item(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Project { id: a, .. }, Self::Project { id: b, .. })
            | (Self::Task { id: a, .. }, Self::Task { id: b, .. }) => a == b,
            (Self::Section { section: a, .. }, Self::Section { section: b, .. }) => a == b,
            (Self::Empty(a), Self::Empty(b)) => a == b,
            _ => false,
        }
    }
    /// Returns the task this row opens, when it is a task row.
    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::Task { id, .. } => Some(id),
            _ => None,
        }
    }
}

pub struct Filter<'a> {
    pub project: Option<&'a str>,
    pub query: &'a str,
    pub archived: bool,
}

/// The rows of the thread feed and the settled tasks that are listed apart from it.
pub struct Feed {
    pub open: Vec<Row>,
    /// Task rows only, in the chosen order across all projects.
    pub settled: Vec<Row>,
}

type Entry<'a> = (usize, usize, &'a Task);

/// Filters and sorts task identities without cloning drafts, questions or captured context.
pub fn rows(workspace: &Workspace, settings: &Settings, filter: Filter<'_>) -> Feed {
    let projects: HashMap<_, _> = workspace
        .projects
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), (i, p.name.to_lowercase())))
        .collect();
    let mut tasks: Vec<Entry<'_>> = workspace
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
                kodem_core::Sort::Recent | kodem_core::Sort::Attention => b
                    .last_started
                    .unwrap_or(b.updated)
                    .cmp(&a.last_started.unwrap_or(a.updated)),
                kodem_core::Sort::Created => b.created.cmp(&a.created),
                kodem_core::Sort::Name => a.title.cmp(&b.title),
            })
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut rows = Vec::new();
    let mut rest = Vec::new();
    let mut urgent: [Vec<Entry<'_>>; 3] = Default::default();
    for entry in tasks {
        match Section::of(entry.2, settings) {
            Section::NeedsYou => urgent[0].push(entry),
            Section::Working => urgent[1].push(entry),
            Section::Ready => urgent[2].push(entry),
            Section::Threads => rest.push(entry),
        }
    }
    let has_urgent = urgent.iter().any(|group| !group.is_empty());
    for (section, group) in [Section::NeedsYou, Section::Working, Section::Ready]
        .into_iter()
        .zip(urgent)
    {
        if group.is_empty() {
            continue;
        }
        rows.push(Row::Section {
            section,
            count: group.len(),
        });
        rows.extend(group.into_iter().map(task_row));
    }
    let (rest, settled): (Vec<_>, Vec<_>) =
        rest.into_iter().partition(|(_, _, task)| !collapses(task));
    if settings.group_tasks_by_project {
        let mut groups: HashMap<usize, Vec<Entry<'_>>> = HashMap::new();
        for task in rest {
            groups.entry(task.1).or_default().push(task);
        }
        let mut project_order: Vec<_> = groups.keys().copied().collect();
        project_order.sort_by_key(|index| workspace.projects[*index].name.to_lowercase());
        for index in project_order {
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
            rows.extend(tasks.into_iter().map(task_row));
        }
    } else {
        if has_urgent && !rest.is_empty() {
            rows.push(Row::Section {
                section: Section::Threads,
                count: rest.len(),
            });
        }
        rows.extend(rest.into_iter().map(task_row));
    }
    if rows.is_empty() {
        rows.push(Row::Empty(if workspace.projects.is_empty() {
            "Open a repository to start a task."
        } else if !settled.is_empty() {
            "No open tasks."
        } else if !filter.query.is_empty() {
            "No matching tasks."
        } else {
            "No tasks here yet."
        }));
    }
    Feed {
        open: rows,
        settled: settled.into_iter().map(task_row).collect(),
    }
}

// A settled task leaves the open list unless the user pinned it without settling it by hand.
fn collapses(task: &Task) -> bool {
    task.is_settled() && !(task.settled.is_none() && task.pinned)
}

fn task_row((index, project, task): Entry<'_>) -> Row {
    Row::Task {
        index,
        project,
        id: task.id.clone(),
    }
}
