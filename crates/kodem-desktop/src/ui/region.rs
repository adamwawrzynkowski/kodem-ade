//! Parts of the workspace that render as views of their own.
//!
//! GPUI builds and lays out a view again when it, or a view inside it, is notified. A scroll, a
//! hover style and an animation notify the view that painted them. In one large view, a scroll
//! of the preferences would build and lay out the whole conversation again, and so would each
//! step of a pulse in the sidebar. A region is such a part in a view of its own, cached in the
//! shell: what happens inside it renders the region and the shell around it again, and every
//! other region is painted again from its last frame. A change of the shell's state notifies the
//! shell, which renders every region again, as before.
use super::*;

/// A part of the workspace that renders as a view of its own, from the shell's state. Its
/// listeners act on the shell.
pub(super) struct Region {
    shell: WeakEntity<Shell>,
    build: fn(&mut Shell, &mut Window, &mut Context<Shell>) -> AnyElement,
}

impl Region {
    /// Creates a region that `build` renders from the shell. The element fills the region.
    pub(super) fn new(
        shell: WeakEntity<Shell>,
        build: fn(&mut Shell, &mut Window, &mut Context<Shell>) -> AnyElement,
    ) -> Self {
        Self { shell, build }
    }
}

impl Render for Region {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let build = self.build;
        self.shell
            .update(cx, |shell, cx| build(shell, window, cx))
            .unwrap_or_else(|_| div().into_any_element())
    }
}

/// Places a region in the shell, cached in a box of `style`. The region renders again only when
/// it, or the shell, is notified.
pub(super) fn cached(region: &Entity<Region>, style: StyleRefinement) -> AnyElement {
    AnyView::from(region.clone())
        .cached(style)
        .into_any_element()
}
