//! Lightweight, stable rows of the thread navigation.
//!
//! Every task of a project lives in its project's group, so that a row always shows where it
//! belongs. Inside a group, what needs a person leads: tasks waiting for input or blocked by a
//! failure, then running tasks, then finished results that were not opened yet. Everything else
//! follows in the chosen order. Chats have no project, so their feed leads with sections of
//! those states instead. Settled work leaves the feed for one list of its own, and a project
//! without open tasks has no group. A feed holds the tasks of one workspace: agentic coding,
//! chats or reviews.
use kodem_core::{Project, Settings, Task, TaskKind, Workspace};
use std::collections::HashMap;

/// A group of tasks that share a state the user acts on, from the most urgent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
        attention: Attention,
    },
    Task {
        index: usize,
        /// Index of the owning project; absent for a chat, which has none.
        project: Option<usize>,
        id: String,
        /// The row sits in the group of its project, whose header names the project for it.
        grouped: bool,
        /// The row is the last of its group, which closes under it.
        last: bool,
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

/// How many tasks of a project group are in each state that the user acts on. The header of a
/// folded group still shows them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attention {
    pub needs_you: usize,
    pub working: usize,
    pub ready: usize,
}

pub struct Filter<'a> {
    /// The workspace whose tasks are listed.
    pub kind: TaskKind,
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

type Entry<'a> = (usize, Option<usize>, &'a Task);

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
            if task.kind != filter.kind {
                return None;
            }
            let project = projects.get(task.project_id.as_str());
            // A task of a removed project is hidden with it. A chat has no project.
            if filter.kind.has_project() && project.is_none() {
                return None;
            }
            let project_filter = filter.project.filter(|_| filter.kind.has_project());
            (task.archived == filter.archived
                && project_filter.is_none_or(|id| id == task.project_id)
                && (filter.query.is_empty()
                    || task.title.to_lowercase().contains(filter.query)
                    || task.branch.to_lowercase().contains(filter.query)
                    || project.is_some_and(|(_, name)| name.contains(filter.query))))
            .then_some((index, project.map(|(index, _)| *index), task))
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
    let (open, settled): (Vec<_>, Vec<_>) = tasks.into_iter().partition(|(_, _, task)| {
        Section::of(task, settings) != Section::Threads || !collapses(task)
    });
    if filter.kind.has_project() {
        project_groups(&mut rows, workspace, settings, filter.query, open);
    } else {
        status_sections(&mut rows, settings, open);
    }
    if rows.is_empty() {
        rows.push(Row::Empty(
            if filter.kind.has_project() && workspace.projects.is_empty() {
                match filter.kind {
                    TaskKind::Review => "Open a repository to review its changes.",
                    TaskKind::Coding | TaskKind::Chat => "Open a repository to start a task.",
                }
            } else if !settled.is_empty() {
                match filter.kind {
                    TaskKind::Coding => "No open tasks.",
                    TaskKind::Chat => "No open chats.",
                    TaskKind::Review => "No open reviews.",
                }
            } else if !filter.query.is_empty() {
                match filter.kind {
                    TaskKind::Coding => "No matching tasks.",
                    TaskKind::Chat => "No matching chats.",
                    TaskKind::Review => "No matching reviews.",
                }
            } else {
                match filter.kind {
                    TaskKind::Coding => "No tasks here yet.",
                    TaskKind::Chat => "No chats yet. Ask anything to start one.",
                    TaskKind::Review => "No reviews yet. Pick a pull request or a branch.",
                }
            },
        ));
    }
    Feed {
        open: rows,
        settled: settled
            .into_iter()
            .map(|entry| task_row(entry, false, false))
            .collect(),
    }
}

// A settled task leaves the open list unless the user pinned it without settling it by hand.
fn collapses(task: &Task) -> bool {
    task.is_settled() && !(task.settled.is_none() && task.pinned)
}

/// Returns the sort key of a project's group: the place the user gave it, then pinned projects
/// first, then by name. A project that the user never placed follows the placed ones.
pub fn project_rank(project: &Project, settings: &Settings) -> (usize, bool, String) {
    let placed = settings
        .project_order
        .iter()
        .position(|id| *id == project.id)
        .unwrap_or(usize::MAX);
    (placed, !project.pinned, project.name.to_lowercase())
}

// One group per project with open tasks, in the order of `project_rank`. A folded group keeps
// its header, unless a search unfolds it.
fn project_groups<'a>(
    rows: &mut Vec<Row>,
    workspace: &Workspace,
    settings: &Settings,
    query: &str,
    open: Vec<Entry<'a>>,
) {
    let mut groups: HashMap<usize, Vec<Entry<'a>>> = HashMap::new();
    for entry in open {
        if let Some(project) = entry.1 {
            groups.entry(project).or_default().push(entry);
        }
    }
    let mut order: Vec<_> = groups.keys().copied().collect();
    order.sort_by_key(|index| project_rank(&workspace.projects[*index], settings));
    for index in order {
        let Some(mut tasks) = groups.remove(&index) else {
            continue;
        };
        // The sort is stable, so the chosen order holds among tasks in one state.
        tasks.sort_by_key(|(_, _, task)| Section::of(task, settings));
        let mut attention = Attention::default();
        for (_, _, task) in &tasks {
            match Section::of(task, settings) {
                Section::NeedsYou => attention.needs_you += 1,
                Section::Working => attention.working += 1,
                Section::Ready => attention.ready += 1,
                Section::Threads => {}
            }
        }
        let project = &workspace.projects[index];
        rows.push(Row::Project {
            index,
            id: project.id.clone(),
            count: tasks.len(),
            attention,
        });
        if settings.collapsed_projects.contains(&project.id) && query.is_empty() {
            continue;
        }
        let last = tasks.len() - 1;
        rows.extend(
            tasks
                .into_iter()
                .enumerate()
                .map(|(order, entry)| task_row(entry, true, order == last)),
        );
    }
}

// The sections of a feed without projects: the states that need a person, then the rest.
fn status_sections(rows: &mut Vec<Row>, settings: &Settings, open: Vec<Entry<'_>>) {
    let mut sections: [Vec<Entry<'_>>; 4] = Default::default();
    for entry in open {
        sections[Section::of(entry.2, settings) as usize].push(entry);
    }
    let urgent = sections[..3].iter().any(|tasks| !tasks.is_empty());
    for (section, tasks) in [
        Section::NeedsYou,
        Section::Working,
        Section::Ready,
        Section::Threads,
    ]
    .into_iter()
    .zip(sections)
    {
        // The rest needs no header while nothing stands above it.
        if !tasks.is_empty() && (section != Section::Threads || urgent) {
            rows.push(Row::Section {
                section,
                count: tasks.len(),
            });
        }
        rows.extend(tasks.into_iter().map(|entry| task_row(entry, false, false)));
    }
}

fn task_row((index, project, task): Entry<'_>, grouped: bool, last: bool) -> Row {
    Row::Task {
        index,
        project,
        id: task.id.clone(),
        grouped,
        last,
    }
}
