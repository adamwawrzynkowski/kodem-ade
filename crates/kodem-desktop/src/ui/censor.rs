//! Dithered censorship of the email addresses of signed-in accounts.
//!
//! The name before the `@` is covered by a still dot field until the user clicks the address;
//! the domain stays readable, so the cover reads as a hidden part of an address. A covered name
//! is not part of the element tree: a mask of the same length keeps its place, so nothing can be
//! read through the dots. Every address is covered again when a popup, a dialog or a settings
//! section opens, and no reveal is stored, so a new window starts with all addresses covered.
use super::*;
use std::collections::BTreeSet;

/// What is open above the workspace: the panel, the settings section, the agent popup and the
/// image preview.
type Surface = (Panel, usize, bool, bool);

#[derive(Default)]
pub(super) struct Censor {
    /// Accounts whose address the user uncovered on the open surface.
    revealed: BTreeSet<(Provider, String)>,
    surface: Option<Surface>,
}

impl Shell {
    /// Covers every address again when the surface above the workspace changes. Call it once
    /// per render.
    pub(super) fn sync_censor(&mut self) {
        let surface = (
            self.panel.clone(),
            self.settings_section,
            self.agent_popup.is_some(),
            self.image_preview.is_some(),
        );
        if self.censor.surface.as_ref() != Some(&surface) {
            self.censor.surface = Some(surface);
            self.censor.revealed.clear();
        }
    }

    /// Builds an account's email address with a dithered cover over the name before the `@`.
    /// A click lifts the cover and the next click puts it back.
    ///
    /// The address takes the font and the text color of its parent, which must be a mono font
    /// for the cover to have the width of the name.
    pub(super) fn censored_email(
        &self,
        provider: Provider,
        account: &str,
        email: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.theme;
        let key = (provider, account.to_owned());
        let shown = self.censor.revealed.contains(&key);
        // An address without a domain is covered as a whole.
        let (name, domain) = match email.rfind('@') {
            Some(at) if !shown => email.split_at(at),
            _ => (email, ""),
        };
        let name = if shown {
            name.to_owned()
        } else {
            "x".repeat(name.chars().count())
        };
        let hint = if shown { "Hide email" } else { "Show email" };
        // The outer row keeps the address as wide as its text inside a parent that stretches
        // its children.
        div().flex().min_w_0().child(
            div()
                .id((
                    ElementId::from(GROUP),
                    SharedString::from(format!("{provider:?}:{account}")),
                ))
                .group(GROUP)
                .flex()
                .min_w_0()
                .cursor_pointer()
                .hover(move |address| address.text_color(t.text))
                .child(
                    div()
                        .relative()
                        .min_w_0()
                        .child(
                            div()
                                .text_ellipsis()
                                .when(!shown, |mask| mask.invisible())
                                .child(name),
                        )
                        .when(!shown, |covered| {
                            // The cover takes the accent while the pointer is over the address.
                            covered.child(cover(t.muted)).child(
                                cover(t.accent)
                                    .invisible()
                                    .group_hover(GROUP, |lit| lit.visible()),
                            )
                        }),
                )
                .when(!domain.is_empty(), |address| {
                    address.child(div().min_w_0().text_ellipsis().child(domain.to_owned()))
                })
                .tooltip(move |_, cx| {
                    cx.new(|_| Hint {
                        text: hint.into(),
                        theme: t,
                    })
                    .into()
                })
                // A row that holds the address must not take the press as its own click.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |s, _, _, cx| {
                    if !s.censor.revealed.remove(&key) {
                        s.censor.revealed.insert(key.clone());
                    }
                    cx.notify();
                })),
        )
    }
}

/// Names the element and the hover group of one address.
const GROUP: &str = "email-censor";

// The still dot field over the hidden part of an address.
fn cover(color: Hsla) -> Div {
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(2.))
        .bottom(px(2.))
        .child(
            dither::Dither::new(dither::DitherField::Flow {
                scale: 0.5,
                speed: 0.,
                bias: 0.25,
            })
            .color(color)
            .grid(2., 1.)
            .toned(),
        )
}
