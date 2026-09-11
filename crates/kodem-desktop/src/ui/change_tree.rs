//! Folder-first changes navigation. Leaves retain their immutable snapshot index.
use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct Folder {
    folders: BTreeMap<String, Folder>,
    files: Vec<(String, usize)>,
    count: usize,
}
impl Folder {
    fn insert(&mut self, path: &str, index: usize) {
        self.count += 1;
        if let Some((name, rest)) = path.split_once('/') {
            self.folders
                .entry(name.into())
                .or_default()
                .insert(rest, index);
        } else {
            self.files.push((path.into(), index));
        }
    }
    fn rows(&self, path: &str, depth: usize, collapsed: &HashSet<String>, rows: &mut Vec<TreeRow>) {
        for (name, folder) in &self.folders {
            let path = format!("{path}{name}/");
            rows.push(TreeRow {
                name: name.clone(),
                path: path.clone(),
                depth,
                file: None,
                count: folder.count,
                repository: None,
            });
            if !collapsed.contains(&path) {
                folder.rows(&path, depth + 1, collapsed, rows);
            }
        }
        let mut files: Vec<_> = self.files.iter().collect();
        files.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        for (name, index) in files {
            rows.push(TreeRow {
                name: name.clone(),
                path: format!("{path}{name}"),
                depth,
                file: Some(*index),
                count: 0,
                repository: None,
            });
        }
    }
}
#[derive(Clone)]
struct TreeRow {
    name: String,
    path: String,
    depth: usize,
    file: Option<usize>,
    count: usize,
    repository: Option<std::path::PathBuf>,
}
impl Shell {
    /// Groups changes by owning checkout, then renders foldable folders and file totals.
    pub(super) fn changes_tree(
        &self,
        snapshot: &DiffSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut groups = BTreeMap::<_, Folder>::new();
        for (i, file) in snapshot.files.iter().enumerate() {
            groups
                .entry(file.repository.clone())
                .or_default()
                .insert(&file.label, i);
        }
        let mut rows = Vec::new();
        for (repository, folder) in groups {
            if let Some(root) = repository {
                let name = snapshot
                    .repositories
                    .iter()
                    .find(|repo| repo.root == root)
                    .map_or_else(|| root.display().to_string(), |repo| repo.label.clone());
                let path = format!("repo:{root:?}/");
                rows.push(TreeRow {
                    name,
                    path: path.clone(),
                    depth: 0,
                    file: None,
                    count: folder.count,
                    repository: Some(root),
                });
                if !self.collapsed_changes.contains(&path) {
                    folder.rows(&path, 1, &self.collapsed_changes, &mut rows);
                }
            } else {
                folder.rows("", 0, &self.collapsed_changes, &mut rows);
            }
        }
        let entity = cx.entity();
        uniform_list("changed-files", rows.len(), move |range, _, cx| {
            entity.update(cx, |s, cx| {
                range
                    .map(|index| s.change_tree_row(index, rows[index].clone(), cx))
                    .collect::<Vec<_>>()
            })
        })
        .w_full()
        .flex_1()
        .min_h_0()
        .py_1()
        .px_1()
        .into_any_element()
    }

    fn change_tree_row(
        &self,
        row_index: usize,
        row: TreeRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let Some(snapshot) = &self.diff else {
            return div().into_any_element();
        };
        let mut entry = div()
            .id(("change-tree-row", row_index))
            .w_full()
            .min_w_0()
            .h(px(28.))
            .flex_shrink_0()
            .pl(px(8. + row.depth as f32 * 14.))
            .pr_2()
            .rounded(px(5.))
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .text_size(px(11.))
            .hover(move |d| d.bg(t.raised));
        if let Some(i) = row.file {
            let file = &snapshot.files[i];
            let added = file
                .lines
                .iter()
                .filter(|l| l.kind == LineKind::Added)
                .count();
            let removed = file
                .lines
                .iter()
                .filter(|l| l.kind == LineKind::Removed)
                .count();
            let (glyph, color) = match row.name.rsplit('.').next().unwrap_or("") {
                "rs" | "swift" | "ts" | "tsx" | "js" | "cs" | "cpp" | "h" => ("code", t.accent),
                "sh" | "command" => ("square-terminal", t.added),
                "toml" | "json" | "yaml" | "yml" => ("settings", t.muted),
                _ => ("file", t.muted),
            };
            let status = if file.status.contains('?') {
                "A".into()
            } else {
                file.status.trim().to_owned()
            };
            let status_color = if status.contains('D') {
                t.removed
            } else if status.contains('A') {
                t.added
            } else {
                t.accent
            };
            entry = entry
                .when(self.reviewing_file && self.file == i, |d| {
                    d.bg(t.selected).shadow(t.elevation())
                })
                .child(div().w(px(12.)).flex_shrink_0())
                .child(icon(glyph).size(px(14.)).text_color(color))
                .child(div().flex_1().min_w_0().text_ellipsis().child(row.name))
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(t.added)
                        .child(format!("+{added}")),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(t.removed)
                        .child(format!("−{removed}")),
                )
                .child(
                    div()
                        .w(px(18.))
                        .text_center()
                        .text_size(px(10.))
                        .text_color(status_color)
                        .child(status),
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.file = i;
                    s.reviewing_file = true;
                    cx.notify();
                }));
        } else {
            let collapsed = self.collapsed_changes.contains(&row.path);
            if let Some(root) = &row.repository {
                let hint = format!("Repository: {}", root.display());
                entry = entry.tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: hint.clone(),
                        theme: t,
                    })
                    .into()
                });
            }
            entry = entry
                .text_color(t.muted)
                .child(
                    icon(if collapsed {
                        "chevron-right"
                    } else {
                        "chevron-down"
                    })
                    .size(px(12.)),
                )
                .child(
                    icon(if row.repository.is_some() {
                        "branch"
                    } else if collapsed {
                        "folder-closed"
                    } else {
                        "folder-open"
                    })
                    .size(px(14.))
                    .text_color(t.accent),
                )
                .child(div().flex_1().min_w_0().text_ellipsis().child(row.name))
                .child(
                    div()
                        .rounded(px(4.))
                        .px_1()
                        .bg(t.sidebar)
                        .text_size(px(10.))
                        .text_color(t.subtle)
                        .child(row.count.to_string()),
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    if !s.collapsed_changes.remove(&row.path) {
                        s.collapsed_changes.insert(row.path.clone());
                    }
                    cx.notify();
                }));
        }
        entry.into_any_element()
    }
}
