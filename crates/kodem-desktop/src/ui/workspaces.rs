//! The workspace rail: a narrow column of icons that swaps the window between Agentic Coding,
//! Chat, Review and Git. Each task workspace lists its own tasks, and keeps the task that was
//! open in it and the unsent draft of its start screen. The Git workspace covers the task
//! workspace that was open and keeps its own repositories.
use super::*;
use crate::thread_list::Section;

/// Width of the workspace rail, in points.
pub(super) const RAIL: f32 = 48.;
/// Side of a workspace tile in the rail, in points.
const TILE: f32 = 34.;
/// Space between the workspace tiles, in points.
const TILE_GAP: f32 = 4.;
/// Height of the selection pill beside the open workspace's tile, in points.
const PILL: f32 = 18.;
/// How long the selection pill takes to slide to another tile.
const SLIDE: Duration = Duration::from_millis(380);

/// The tile that the rail's selection marks, and the tile that it last slid from.
#[derive(Default)]
pub(super) struct RailSlide {
    /// The slot that the last frame marked, once a frame has drawn the rail.
    shown: Option<usize>,
    /// The slot that the selection left last.
    from: Option<usize>,
    /// Counts the moves of the selection. Each move plays its animations under a new name.
    sequence: u64,
}

/// The unsent draft of a workspace's start screen, kept while another workspace is open.
#[derive(Default)]
pub(super) struct StartDraft {
    text: String,
    attachments: Vec<AttachmentSource>,
}

/// Returns the keys that open a workspace.
fn mode_keys(mode: TaskKind) -> &'static str {
    match mode {
        TaskKind::Coding => "⌥⌘1",
        TaskKind::Chat => "⌥⌘2",
        TaskKind::Review => "⌥⌘3",
    }
}

impl Shell {
    /// Builds the workspace rail. It also holds the quick actions while the sidebar is closed.
    pub(super) fn workspace_rail(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let mut rail = div()
            .id("workspace-rail")
            .relative()
            .w(px(RAIL))
            .flex_shrink_0()
            .h_full()
            .map(|rail| kit::panel_glass(rail, t))
            .flex()
            .flex_col()
            .items_center()
            .pt(px(10.))
            .pb(px(8.))
            .gap(px(4.));
        let mut tiles = div()
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(TILE_GAP));
        for (slot, mode) in TaskKind::ALL.into_iter().enumerate() {
            tiles = tiles.child(self.mode_button(slot, mode, cx));
        }
        rail = rail.child(
            tiles
                .child(self.git_button(TaskKind::ALL.len(), cx))
                .child(rail_pill(&self.rail_slide, self.rail_slot(), t.accent)),
        );
        if self.git_open
            && let Some(git) = self.git.clone()
        {
            // The Git workspace's repositories and actions take the rest of the rail.
            return rail
                .child(motion::stagger("rail-git-rule", rail_rule(t), 0))
                .child(motion::stagger(
                    "rail-git",
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .child(git.update(cx, |git, cx| git.render_rail(window, cx))),
                    1,
                ))
                .into_any_element();
        }
        if self.reveals.sidebar > 0. {
            return rail.into_any_element();
        }
        // The quick actions arrive one after another once the sidebar has folded away.
        rail.child(motion::stagger("rail-quick-rule", rail_rule(t), 0))
            .child(motion::stagger(
                "rail-quick-new",
                self.icon_button("rail-new", "edit", self.new_thread_hint())
                    .on_click(cx.listener(|s, _, w, cx| s.new_task(&NewTask, w, cx))),
                1,
            ))
            .child(motion::stagger(
                "rail-quick-search",
                self.icon_button("rail-search", "search", "Search tasks · ⌘F")
                    .on_click(cx.listener(|s, _, w, cx| s.open_search(w, cx))),
                2,
            ))
            .child(motion::stagger(
                "rail-quick-palette",
                self.icon_button("rail-palette", "command", "Command palette · ⌘K")
                    .on_click(cx.listener(|s, _, w, cx| s.open_palette(w, cx))),
                3,
            ))
            .child(motion::stagger(
                "rail-quick-open",
                self.icon_button("rail-open", "folder-open", "Open repository · ⌘O")
                    .on_click(cx.listener(|s, _, w, cx| s.open_project(&OpenProject, w, cx))),
                4,
            ))
            .child(div().flex_1())
            .child(self.update_control(true, cx))
            .child(self.notification_bell("rail-notifications", cx))
            .child(
                self.icon_button("rail-settings", "settings", "Settings · ⌘,")
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.panel = Panel::Settings;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    // One workspace of the rail at `slot`, with a dot for tasks that want attention.
    fn mode_button(&self, slot: usize, mode: TaskKind, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let chosen = self.rail_slot() == slot;
        let attention = self.mode_attention(mode);
        let hint = match &attention {
            Some((_, _, note)) => format!("{} · {note} · {}", mode.label(), mode_keys(mode)),
            None => format!("{} · {}", mode.label(), mode_keys(mode)),
        };
        let tile = self
            .rail_tile(slot, mode.icon(), mode.label(), hint, chosen)
            .on_click(cx.listener(move |s, _, _, cx| s.set_mode(mode, cx)));
        self.rail_cell(slot, tile, chosen)
            .when_some(attention.filter(|_| !chosen), |cell, (rank, tone, _)| {
                cell.child(attention_dot(
                    ElementId::NamedInteger(format!("rail-attention-{slot}").into(), rank),
                    tone,
                    t.sidebar,
                ))
            })
            .into_any_element()
    }

    // The tile of a workspace in the rail at `slot`. The open workspace's tile takes the accent,
    // lit from below, and its glyph springs into place as it opens. The others stay flat until
    // the pointer finds them.
    fn rail_tile(
        &self,
        slot: usize,
        glyph: &'static str,
        label: &'static str,
        hint: String,
        chosen: bool,
    ) -> Button {
        let t = self.theme;
        let tile = self
            .control(("workspace-tile", slot))
            .overflow_hidden()
            .size(px(TILE))
            .px_0()
            .justify_center()
            .rounded(px(crate::theme::RADIUS_SURFACE))
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .text_color(t.muted)
            .accessibility_label(label)
            .active(move |tile| tile.bg(t.control_active))
            .on_press(move |position, cx| fx::ripple(position, t.accent, cx))
            .tooltip(move |_, cx| {
                cx.new(|_| Hint {
                    text: hint.clone(),
                    theme: t,
                })
                .into()
            });
        if !chosen {
            return tile
                .layer(kit::glint(t.dot, dither::Edge::Bottom))
                .hover(move |tile| {
                    tile.bg(t.selected)
                        .border_color(t.border)
                        .text_color(t.text)
                })
                .child(icon(glyph).size(px(17.)));
        }
        tile.bg(kit::tint_fill(t.accent, 0.1))
            .shadow(kit::tint_light(t.accent))
            .border_color(t.accent.opacity(0.36))
            .layer(rail_bloom(t.accent))
            .child(
                svg()
                    .path(glyph)
                    .size(px(17.))
                    .flex_shrink_0()
                    .text_color(t.accent)
                    .with_animation(
                        ("rail-glyph", slot),
                        Animation::new(Duration::from_millis(420)),
                        |glyph, delta| {
                            let scale = 0.6 + 0.4 * fx::ease_out_back(delta);
                            glyph.with_transformation(Transformation::scale(size(scale, scale)))
                        },
                    ),
            )
    }

    // The row of the rail at `slot` that holds a workspace tile. The tile of the workspace that
    // just opened forms out of dither, and the tile that the selection just left lets its accent
    // fade away.
    fn rail_cell(&self, slot: usize, tile: Button, chosen: bool) -> Div {
        let t = self.theme;
        let left = !chosen && self.rail_slide.from == Some(slot);
        div()
            .relative()
            .w_full()
            .h(px(TILE))
            .flex()
            .justify_center()
            .when(left, |cell| {
                cell.child(rail_afterglow(self.rail_slide.sequence, t.accent))
            })
            .child(if chosen {
                motion::dissolve(
                    ("workspace-chosen", slot),
                    tile,
                    t.sidebar,
                    crate::theme::RADIUS_SURFACE,
                    0.,
                    2.,
                    240,
                )
            } else {
                tile.into_any_element()
            })
    }

    // The rail slot of the open workspace: the task workspaces in their order, then Git.
    fn rail_slot(&self) -> usize {
        if self.git_open {
            return TaskKind::ALL.len();
        }
        TaskKind::ALL
            .iter()
            .position(|mode| *mode == self.mode)
            .unwrap_or_default()
    }

    /// Starts the slide of the rail's selection when another workspace opened since the last
    /// frame. Call it once per render.
    pub(super) fn sync_rail_slide(&mut self) {
        let slot = self.rail_slot();
        let slide = &mut self.rail_slide;
        if let Some(shown) = slide.shown
            && shown != slot
        {
            slide.from = Some(shown);
            slide.sequence += 1;
        }
        slide.shown = Some(slot);
    }

    // Returns the most urgent state among a workspace's tasks: its rank, from the most urgent,
    // its tone and its wording.
    fn mode_attention(&self, mode: TaskKind) -> Option<(u64, Hsla, String)> {
        let t = self.theme;
        let workspace = self.workspace.as_ref()?;
        let mut counts = [0usize; 3];
        for task in workspace
            .tasks
            .iter()
            .filter(|task| task.kind == mode && !task.archived)
        {
            match Section::of(task, &self.settings) {
                Section::NeedsYou => counts[0] += 1,
                Section::Working => counts[1] += 1,
                Section::Ready => counts[2] += 1,
                Section::Threads => {}
            }
        }
        let [waiting, working, ready] = counts;
        if waiting > 0 {
            Some((
                0,
                t.warning,
                format!(
                    "{waiting} {} you",
                    if waiting == 1 { "needs" } else { "need" }
                ),
            ))
        } else if working > 0 {
            Some((1, t.accent, format!("{working} working")))
        } else if ready > 0 {
            Some((2, t.added, format!("{ready} ready")))
        } else {
            None
        }
    }

    /// Returns the tooltip of the control that starts a new thread in the open workspace.
    pub(super) fn new_thread_hint(&self) -> &'static str {
        match self.mode {
            TaskKind::Coding => "New task · ⌘N",
            TaskKind::Chat => "New chat · ⌘N",
            TaskKind::Review => "New review · ⌘N",
        }
    }

    /// Opens another workspace. The task that was open in it comes back; without one, its start
    /// screen shows.
    pub(super) fn set_mode(&mut self, mode: TaskKind, cx: &mut Context<Self>) {
        if self.creating_task || self.launch.is_some() {
            return;
        }
        if self.git_open {
            self.close_git(cx);
        }
        if self.mode == mode {
            return;
        }
        self.enter_mode(mode, cx);
        let remembered = self.mode_tasks.get(&mode).cloned().filter(|id| {
            self.workspace.as_ref().is_some_and(|workspace| {
                workspace
                    .tasks
                    .iter()
                    .any(|task| &task.id == id && task.kind == mode)
            })
        });
        match remembered {
            Some(id) => self.select_task(id, cx),
            None => self.save_settings(),
        }
        cx.notify();
    }

    /// Moves the window to another workspace with no task open in it.
    ///
    /// The task that was open and the draft of the start screen stay with the workspace that is
    /// left, and the draft of the entered workspace returns to the composer.
    pub(super) fn enter_mode(&mut self, mode: TaskKind, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        match &self.selected {
            Some(id) => self.mode_tasks.insert(self.mode, id.clone()),
            None => self.mode_tasks.remove(&self.mode),
        };
        // Closing the open task returns the composer to the start screen's draft.
        self.open_empty_chat(cx);
        let left = StartDraft {
            text: self.input.read(cx).text(),
            attachments: std::mem::take(&mut self.pending_attachments),
        };
        self.mode_drafts.insert(self.mode, left);
        let entered = self.mode_drafts.remove(&mode).unwrap_or_default();
        // The baseline of a review is the merge base of what it examines, so its changes panel
        // opens on that comparison. Leaving Review returns to the working tree.
        if mode == TaskKind::Review {
            self.comparison = Comparison::Baseline;
        } else if self.mode == TaskKind::Review {
            self.comparison = Comparison::WorkingTree;
        }
        self.mode = mode;
        self.settings.mode = mode;
        self.new_preferences = None;
        self.new_worktree_override = None;
        self.empty_draft = entered.text.clone();
        self.pending_attachments = entered.attachments;
        self.input
            .update(cx, |input, cx| input.set_text(entered.text, cx));
        self.draft_dirty = false;
        self.search_visible = false;
        self.search
            .update(cx, |input, cx| input.set_text(String::new(), cx));
        self.sidebar_dirty = true;
        self.sync_placeholder(cx);
        if mode == TaskKind::Review {
            self.load_review_targets(false);
        }
        if self.selected_runtime().models.is_empty() {
            self.connect_runtime();
        }
        cx.notify();
    }

    /// Sets the composer's hint to what the open workspace and task expect.
    pub(super) fn sync_placeholder(&mut self, cx: &mut Context<Self>) {
        let placeholder = match (self.mode, self.selected.is_some()) {
            (TaskKind::Coding, _) => "Describe what you want to build or change…",
            (TaskKind::Chat, _) => "Ask anything…",
            (TaskKind::Review, false) => "Tell the reviewer what to focus on (optional)…",
            (TaskKind::Review, true) => "Ask about this review…",
        };
        self.input
            .update(cx, |input, cx| input.set_placeholder(placeholder, cx));
    }
}

impl Shell {
    // The Git workspace's tile in the rail at `slot`: the branch glyph with a Beta tag hanging
    // from its lower edge.
    fn git_button(&self, slot: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let chosen = self.rail_slot() == slot;
        let tile = self
            .rail_tile(
                slot,
                "git-pull-request",
                "Git, beta",
                "Git · Beta · ⌥⌘4".into(),
                chosen,
            )
            .on_click(cx.listener(|s, _, _, cx| s.open_git(cx)));
        self.rail_cell(slot, tile, chosen)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(-4.))
                    .flex()
                    .justify_center()
                    .child(
                        git_beta_badge(t, chosen).relative().with_animation(
                            ("rail-beta", chosen as usize),
                            Animation::new(Duration::from_millis(260))
                                .with_easing(motion::ease_out_cubic),
                            |tag, delta| tag.top(px(3. * (1. - delta))).opacity(delta),
                        ),
                    ),
            )
            .into_any_element()
    }

    /// Returns the Git workspace, created the first time it is needed.
    pub(super) fn git_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<git::GitWorkspace> {
        if let Some(git) = &self.git {
            return git.clone();
        }
        let git = cx.new(|cx| git::GitWorkspace::new(window, cx));
        // The title bar, rail and sidebar draw parts of the Git workspace inside this view.
        self._subscriptions
            .push(cx.observe(&git, |_, _, cx| cx.notify()));
        self._subscriptions
            .push(cx.subscribe(&git, |s, _, event: &git::GitEvent, cx| {
                s.git_event(event, cx)
            }));
        self.git = Some(git.clone());
        self.sync_git_projects(cx);
        // The repositories that the workspace restored opened before this view listened.
        for path in git.read(cx).repository_paths(cx) {
            self.git_event(&git::GitEvent::RepositoryOpened(path), cx);
        }
        git
    }

    /// Opens the Git workspace over the task workspace.
    pub(super) fn open_git(&mut self, cx: &mut Context<Self>) {
        if self.git_open || self.creating_task {
            return;
        }
        self.git_open = true;
        self.settings.git_workspace = true;
        self.panel = Panel::None;
        self.search_visible = false;
        self.save_settings();
        cx.notify();
    }

    /// Returns to the task workspace that the Git workspace covered.
    pub(super) fn close_git(&mut self, cx: &mut Context<Self>) {
        if !self.git_open {
            return;
        }
        self.git_open = false;
        self.settings.git_workspace = false;
        if let Some(git) = &self.git {
            git.update(cx, |git, cx| git.dismiss_popovers(cx));
        }
        self.save_settings();
        cx.notify();
    }

    /// Gives the Git workspace Kodem's projects, which name and draw its repositories.
    pub(super) fn sync_git_projects(&mut self, cx: &mut Context<Self>) {
        let Some(git) = &self.git else {
            return;
        };
        let projects = self
            .workspace
            .as_ref()
            .map(|workspace| {
                workspace
                    .projects
                    .iter()
                    .filter(|project| !project.removed)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        git.update(cx, |git, cx| git.set_projects(projects, cx));
    }

    // Carries out what the Git workspace asks of the window.
    fn git_event(&mut self, event: &git::GitEvent, cx: &mut Context<Self>) {
        match event {
            git::GitEvent::RepositoryOpened(path) => {
                let known = self.workspace.as_ref().is_some_and(|workspace| {
                    workspace
                        .projects
                        .iter()
                        .any(|project| !project.removed && &project.path == path)
                });
                if !known {
                    self.git_registrations.insert(plain_path(path));
                    self.send(core::Request::OpenProject {
                        path: path.clone(),
                        missing_repository: MissingRepository::Ask,
                    });
                }
            }
            git::GitEvent::CustomizeRepository(path) => {
                let project = self.workspace.as_ref().and_then(|workspace| {
                    workspace
                        .projects
                        .iter()
                        .find(|project| !project.removed && &project.path == path)
                        .cloned()
                });
                if let Some(project) = project {
                    self.open_project_editor(project, cx);
                }
            }
        }
    }
}

/// Returns the Beta tag of the Git workspace's tile: a small mono pill in the accent, outlined
/// over the rail and filled while the Git workspace is open.
pub(super) fn git_beta_badge(theme: Theme, chosen: bool) -> Div {
    div()
        .h(px(10.))
        .px(px(3.))
        .flex()
        .items_center()
        .rounded(px(3.))
        .border_1()
        .border_color(if chosen {
            theme.accent
        } else {
            theme.accent.opacity(0.45)
        })
        .bg(if chosen { theme.accent } else { theme.sidebar })
        .text_color(if chosen {
            theme.on_accent
        } else {
            theme.accent
        })
        .font_family(crate::theme::mono())
        .text_size(text_px(6.))
        .line_height(text_px(8.))
        .font_weight(FontWeight::BOLD)
        .letter_spacing(text_px(0.4))
        .child("BETA")
}

// The dotted rule that parts the workspace tiles from the rest of the rail.
fn rail_rule(theme: Theme) -> Div {
    div()
        .relative()
        .w(px(22.))
        .py(px(4.))
        .child(dither::DottedRule::horizontal(theme.subtle.opacity(0.4)))
}

// Dots that light the open workspace's tile from below, densest under its glyph.
fn rail_bloom(tone: Hsla) -> dither::Dither {
    dither::Dither::new(dither::DitherField::Glow {
        x: 0.5,
        y: 1.1,
        radius: 0.95,
        strength: 0.75,
    })
    .color(tone.opacity(0.55))
    .grid(2., 1.)
}

// The top and bottom of the selection pill beside the tile at `slot`, in points from the top
// of the tiles.
fn pill_span(slot: usize) -> (f32, f32) {
    let top = slot as f32 * (TILE + TILE_GAP) + (TILE - PILL) / 2.;
    (top, top + PILL)
}

// The glowing selection pill on the rail's leading edge, between `top` and `bottom`.
fn pill(tone: Hsla, (top, bottom): (f32, f32)) -> Div {
    div()
        .absolute()
        .left_0()
        .top(px(top))
        .w(px(3.))
        .h(px(bottom - top))
        .rounded_r(px(2.))
        .bg(tone)
        .shadow(vec![BoxShadow {
            inset: false,
            color: tone.opacity(0.5),
            offset: point(px(0.), px(0.)),
            blur_radius: px(6.),
            spread_radius: px(0.),
        }])
}

// The selection pill beside the tile at `slot`. After another workspace opened, it slides there
// from the tile it left: its leading end springs ahead and its trailing end follows, so it
// stretches on the way and snaps back on arrival.
fn rail_pill(slide: &RailSlide, slot: usize, tone: Hsla) -> AnyElement {
    let to = pill_span(slot);
    let Some(from) = slide.from.filter(|from| *from != slot).map(pill_span) else {
        return pill(tone, to).into_any_element();
    };
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .with_animation(
            ElementId::NamedInteger("rail-slide".into(), slide.sequence),
            Animation::new(SLIDE),
            move |layer, delta| layer.child(pill(tone, fx::stretch_span(from, to, delta))),
        )
        .into_any_element()
}

// The accent of a tile that the selection just left, fading out behind its glyph.
fn rail_afterglow(sequence: u64, tone: Hsla) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left(px((RAIL - TILE) / 2.))
        .size(px(TILE))
        .rounded(px(crate::theme::RADIUS_SURFACE))
        .border_1()
        .border_color(tone.opacity(0.36))
        .bg(kit::tint_fill(tone, 0.1))
        .with_animation(
            ElementId::NamedInteger("rail-afterglow".into(), sequence),
            Animation::new(Duration::from_millis(280)).with_easing(motion::ease_out_cubic),
            |glow, delta| glow.opacity(1. - delta),
        )
        .into_any_element()
}

// Side of a workspace's attention dot, and of the ring that it sends out, in points.
const DOT: f32 = 7.;
const DOT_REACH: f32 = 18.;

// A dot on the corner of a tile in the tone of what its workspace's tasks want, cut out of the
// tile by a ring in the rail's tone. It springs in and sends out one fading ring each time its
// state changes.
fn attention_dot(id: ElementId, tone: Hsla, rail: Hsla) -> AnyElement {
    div()
        .absolute()
        .left(px((RAIL + TILE) / 2. - 4. - DOT_REACH / 2.))
        .top(px(5. - DOT_REACH / 2.))
        .size(px(DOT_REACH))
        .with_animation(
            id,
            Animation::new(Duration::from_millis(560)),
            move |spot, delta| {
                let ring = motion::ease_out_cubic(delta);
                spot.when(delta < 1., |spot| {
                    spot.child(
                        centered_disc(DOT + (DOT_REACH - DOT) * ring)
                            .border_1()
                            .border_color(tone.opacity(0.6 * (1. - ring))),
                    )
                })
                .child(
                    centered_disc(DOT * fx::ease_out_back((delta * 1.8).min(1.)))
                        .border_1()
                        .border_color(rail)
                        .bg(tone),
                )
            },
        )
        .into_any_element()
}

// A circle of `side` points in the middle of an attention dot's area.
fn centered_disc(side: f32) -> Div {
    let inset = (DOT_REACH - side) / 2.;
    div()
        .absolute()
        .left(px(inset))
        .top(px(inset))
        .size(px(side.max(0.)))
        .rounded_full()
}

/// Returns a folder in the form that the host records for a project, for comparisons.
pub(super) fn plain_path(path: &std::path::Path) -> PathBuf {
    path.canonicalize()
        .map(kodem_core::platform::plain_path)
        .unwrap_or_else(|_| path.to_path_buf())
}
