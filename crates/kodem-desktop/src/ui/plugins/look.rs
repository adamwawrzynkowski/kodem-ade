//! The look of the Plugins workspace: the categories of servers, the icons of servers and skills,
//! and the cards, banners and sections of its pages.
use super::*;

/// Widest that the content of a Plugins page grows, in points. Wider windows center it.
pub(in crate::ui) const CONTENT_WIDTH: f32 = 1180.;
/// Space on each side of the content, in points.
pub(in crate::ui) const GUTTER: f32 = 24.;
/// Narrowest that a card may be before the grid takes one column less, in points.
pub(in crate::ui) const CARD_MIN: f32 = 270.;
/// Space between cards, in points.
pub(in crate::ui) const GAP: f32 = 12.;
/// Height of a card, in points.
pub(in crate::ui) const CARD_HEIGHT: f32 = 158.;
/// Height of a featured card, in points.
pub(in crate::ui) const FEATURED_HEIGHT: f32 = 178.;
/// Most columns of the grid.
const COLUMNS: usize = 4;
/// Most downloads of icons or descriptions that wait at once.
pub(in crate::ui) const QUEUE_LIMIT: usize = 48;

/// What a server is for, guessed from its name and description.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::ui) enum Category {
    #[default]
    All,
    Developer,
    Data,
    Web,
    Productivity,
    Cloud,
    Media,
    Finance,
    Communication,
    Other,
}

impl Category {
    /// Every category, in the order of the filter.
    pub(in crate::ui) const ALL: [Self; 10] = [
        Self::All,
        Self::Developer,
        Self::Data,
        Self::Web,
        Self::Productivity,
        Self::Cloud,
        Self::Media,
        Self::Finance,
        Self::Communication,
        Self::Other,
    ];

    /// The order in which a server is matched, the most specific first.
    const MATCHING: [Self; 8] = [
        Self::Finance,
        Self::Communication,
        Self::Data,
        Self::Cloud,
        Self::Web,
        Self::Productivity,
        Self::Developer,
        Self::Media,
    ];

    /// Returns the category's name.
    pub(in crate::ui) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Developer => "Developer tools",
            Self::Data => "Data",
            Self::Web => "Web & browser",
            Self::Productivity => "Productivity",
            Self::Cloud => "Cloud",
            Self::Media => "AI & media",
            Self::Finance => "Finance",
            Self::Communication => "Communication",
            Self::Other => "Other",
        }
    }

    /// Returns the category's glyph.
    pub(in crate::ui) fn glyph(self) -> &'static str {
        match self {
            Self::All => "layers",
            Self::Developer => "projects/code",
            Self::Data => "projects/database",
            Self::Web => "projects/globe",
            Self::Productivity => "projects/layout-dashboard",
            Self::Cloud => "projects/cloud",
            Self::Media => "projects/wand-sparkles",
            Self::Finance => "projects/banknote",
            Self::Communication => "projects/message-square",
            Self::Other => "puzzle",
        }
    }

    /// Returns the category's tone.
    pub(in crate::ui) fn tone(self, t: Theme) -> Hsla {
        match self {
            Self::All | Self::Developer => t.accent,
            Self::Data | Self::Cloud => t.info,
            Self::Web | Self::Finance => t.added,
            Self::Productivity | Self::Communication => t.warning,
            Self::Media => t.removed,
            Self::Other => t.muted,
        }
    }

    // The words that put a server in the category. A phrase matches anywhere; a word matches
    // a whole word.
    fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Developer => &[
                "github",
                "gitlab",
                "git",
                "code",
                "coding",
                "repository",
                "repo",
                "ci",
                "npm",
                "debug",
                "debugger",
                "ide",
                "sdk",
                "lint",
                "sentry",
                "logs",
                "testing",
                "tests",
                "developer",
                "developers",
                "devtools",
                "api",
                "openapi",
                "graphql",
                "jira",
                "pull request",
                "documentation",
                "docs",
                "compiler",
                "typescript",
                "python",
                "rust",
                "java",
            ],
            Self::Data => &[
                "database",
                "databases",
                "sql",
                "postgres",
                "postgresql",
                "mysql",
                "sqlite",
                "mongodb",
                "mongo",
                "redis",
                "supabase",
                "bigquery",
                "snowflake",
                "analytics",
                "data",
                "dataset",
                "vector",
                "elasticsearch",
                "csv",
                "spreadsheet",
                "warehouse",
                "query",
                "duckdb",
                "clickhouse",
                "neo4j",
            ],
            Self::Web => &[
                "browser",
                "playwright",
                "puppeteer",
                "scrape",
                "scraping",
                "crawl",
                "crawler",
                "search",
                "web",
                "fetch",
                "url",
                "seo",
                "website",
                "websites",
                "html",
            ],
            Self::Productivity => &[
                "notion",
                "linear",
                "asana",
                "trello",
                "calendar",
                "todo",
                "tasks",
                "task",
                "obsidian",
                "notes",
                "note",
                "figma",
                "project management",
                "google drive",
                "confluence",
                "documents",
                "workflow",
                "workflows",
                "productivity",
                "airtable",
                "clickup",
                "monday",
            ],
            Self::Cloud => &[
                "aws",
                "azure",
                "gcp",
                "cloudflare",
                "kubernetes",
                "k8s",
                "terraform",
                "vercel",
                "netlify",
                "infrastructure",
                "hosting",
                "deploy",
                "deployment",
                "devops",
                "docker",
                "container",
                "containers",
                "serverless",
                "cloud",
                "monitoring",
            ],
            Self::Media => &[
                "image",
                "images",
                "video",
                "videos",
                "audio",
                "speech",
                "voice",
                "llm",
                "model",
                "models",
                "ai",
                "generate",
                "generation",
                "music",
                "podcast",
                "podcasts",
                "tts",
                "3d",
                "photo",
                "photos",
                "design",
                "art",
            ],
            Self::Finance => &[
                "stripe",
                "payment",
                "payments",
                "invoice",
                "invoices",
                "finance",
                "financial",
                "bank",
                "banking",
                "crypto",
                "blockchain",
                "trading",
                "stock",
                "stocks",
                "accounting",
                "tax",
                "wallet",
                "defi",
                "price",
                "prices",
            ],
            Self::Communication => &[
                "slack",
                "email",
                "emails",
                "mail",
                "gmail",
                "discord",
                "telegram",
                "whatsapp",
                "sms",
                "chat",
                "outreach",
                "crm",
                "hubspot",
                "salesforce",
                "teams",
                "messages",
                "messaging",
                "support",
                "zendesk",
                "intercom",
            ],
            Self::All | Self::Other => &[],
        }
    }

    /// Returns the category of a server's name and description.
    pub(in crate::ui) fn of(text: &str) -> Self {
        let text = text.to_lowercase();
        let words: HashSet<&str> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect();
        Self::MATCHING
            .into_iter()
            .find(|category| {
                category.keywords().iter().any(|keyword| {
                    if keyword.contains(' ') {
                        text.contains(keyword)
                    } else {
                        words.contains(keyword)
                    }
                })
            })
            .unwrap_or(Self::Other)
    }
}

/// Returns the glyph of a skill, chosen by what its name and description mention.
pub(in crate::ui) fn skill_glyph(name: &str, description: &str) -> &'static str {
    let text = format!("{name} {description}").to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| text.contains(word));
    if has(&["pptx", "slide", "presentation", "deck"]) {
        "projects/presentation"
    } else if has(&["xlsx", "spreadsheet", "excel", "csv"]) {
        "projects/table"
    } else if has(&["pdf", "docx", "word document", "document"]) {
        "projects/file-text"
    } else if has(&["mcp"]) {
        "plug"
    } else if has(&["skill-creator", "create skills", "creating skills"]) {
        "projects/wand-sparkles"
    } else if has(&["art", "design", "canvas", "brand", "theme", "gif", "visual"]) {
        "projects/palette"
    } else if has(&["test", "playwright", "qa"]) {
        "projects/flask-conical"
    } else if has(&["git", "pull request", "commit", "review"]) {
        "projects/git-branch"
    } else if has(&["deploy", "vercel", "netlify", "cloudflare", "cloud"]) {
        "projects/cloud"
    } else if has(&["security", "secure", "vulnerab"]) {
        "projects/shield"
    } else if has(&["sql", "database", "data"]) {
        "projects/database"
    } else if has(&["web", "frontend", "html", "react", "css"]) {
        "projects/globe"
    } else if has(&["api", "webhook"]) {
        "projects/webhook"
    } else if has(&["write", "writing", "notes", "docs"]) {
        "projects/notebook"
    } else {
        "book"
    }
}

/// What a card shows.
pub(in crate::ui) struct CardContent {
    pub(in crate::ui) icon: AnyElement,
    pub(in crate::ui) title: String,
    pub(in crate::ui) subtitle: String,
    pub(in crate::ui) description: String,
    pub(in crate::ui) chips: Vec<(SharedString, Hsla)>,
    pub(in crate::ui) marks: Vec<(Provider, Mark)>,
    pub(in crate::ui) action: AnyElement,
    pub(in crate::ui) installed: bool,
    pub(in crate::ui) tone: Hsla,
    pub(in crate::ui) featured: bool,
}

impl Shell {
    /// Returns the width that the Plugins page has beside the rail and the sidebar, in points.
    pub(in crate::ui) fn plugins_area_width(&self) -> f32 {
        (self.window_width
            - workspaces::RAIL
            - 2.
            - self.settings.sidebar_width * self.reveals.sidebar)
            .max(320.)
    }

    /// Returns the width of the centered content of a Plugins page, in points.
    pub(in crate::ui) fn plugins_content_width(&self) -> f32 {
        self.plugins_area_width().min(CONTENT_WIDTH) - GUTTER * 2.
    }

    /// Returns how many cards a row of the grid holds.
    pub(in crate::ui) fn plugin_columns(&self) -> usize {
        (((self.plugins_content_width() + GAP) / (CARD_MIN + GAP)).floor() as usize)
            .clamp(1, COLUMNS)
    }

    /// Returns a downloaded image, and asks for it when it was not asked for yet. Downloads
    /// run a few at a time.
    pub(in crate::ui) fn plugin_image(
        &mut self,
        url: &str,
        cx: &mut Context<Self>,
    ) -> Option<PathBuf> {
        match self.plugins.images.get(url) {
            Some(Load::Ready(path)) => return Some(path.clone()),
            Some(_) => return None,
            None => {}
        }
        self.plugins.images.insert(url.to_owned(), Load::Idle);
        self.plugins.image_queue.push_back(url.to_owned());
        // Cards that scrolled away give up their place; they ask again when they show.
        while self.plugins.image_queue.len() > QUEUE_LIMIT {
            if let Some(dropped) = self.plugins.image_queue.pop_front() {
                self.plugins.images.remove(&dropped);
            }
        }
        self.pump_plugin_images(cx);
        None
    }

    // Starts downloads while fewer than six run and the workspace is open, newest request
    // first, so the cards in view load before those scrolled past.
    fn pump_plugin_images(&mut self, cx: &mut Context<Self>) {
        while self.plugins.open && self.plugins.image_jobs < 6 {
            let Some(url) = self.plugins.image_queue.pop_back() else {
                return;
            };
            self.plugins.image_jobs += 1;
            self.plugins.images.insert(url.clone(), Load::Loading);
            cx.spawn(async move |this, cx| {
                let asked = url.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { catalog::fetch_image(&asked) })
                    .await;
                let _ = this.update(cx, |s, cx| {
                    s.plugins.image_jobs = s.plugins.image_jobs.saturating_sub(1);
                    s.plugins.images.insert(
                        url,
                        match result {
                            Ok(path) => Load::Ready(path),
                            Err(error) => Load::Failed(format!("{error:#}")),
                        },
                    );
                    s.pump_plugin_images(cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// Returns the icon of a server or skill: its image once it is downloaded, else its glyph on
    /// the plate of an application icon in its tone.
    pub(in crate::ui) fn plugin_icon(
        &mut self,
        image: Option<String>,
        glyph: &'static str,
        tone: Hsla,
        size: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme;
        let glyph_size = (size * 0.5).round();
        let path = image.as_deref().and_then(|url| self.plugin_image(url, cx));
        let Some(path) = path else {
            return sidebar::glyph_tile(glyph, tone, size, glyph_size);
        };
        // An avatar fills its tile; an icon keeps a margin on a light plate.
        let avatar = image.is_some_and(|url| url.starts_with("https://github.com/"));
        let radius = (size * 0.27).round();
        div()
            .size(px(size))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius))
            .overflow_hidden()
            .border_1()
            .border_color(t.edge)
            .bg(if avatar {
                t.raised
            } else {
                gpui::white().opacity(0.92)
            })
            .child(
                img(path)
                    .size(px(if avatar { size } else { size * 0.72 }))
                    .object_fit(if avatar {
                        ObjectFit::Cover
                    } else {
                        ObjectFit::Contain
                    })
                    .with_fallback(move || sidebar::glyph_tile(glyph, tone, size, glyph_size)),
            )
            .into_any_element()
    }

    /// Builds a card of the grid: its icon, title and subtitle, a description of two or three
    /// lines, and a footer with badges, the providers that have it and its action. Every text
    /// is cut to its room, so the footer always keeps its place.
    pub(in crate::ui) fn plugin_card(
        &self,
        id: impl Into<ElementId>,
        card: CardContent,
    ) -> Stateful<Div> {
        let t = self.theme;
        let tone = card.tone;
        let height = if card.featured {
            FEATURED_HEIGHT
        } else {
            CARD_HEIGHT
        };
        div()
            .id(id)
            .relative()
            .flex_1()
            .min_w_0()
            .h(px(height))
            .overflow_hidden()
            .rounded(px(crate::theme::RADIUS_SURFACE))
            .border_1()
            .border_color(if card.installed {
                t.added.opacity(0.35)
            } else {
                t.border
            })
            .bg(t.raised)
            .shadow(kit::plate_light(t, true))
            .layer(dither::edge_shade(t.dot, 0.18))
            .cursor_pointer()
            .hover(move |card| card.border_color(tone.opacity(0.55)).bg(t.control_hover))
            .when(card.featured, |c| {
                c.child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .w(px(220.))
                        .h(px(120.))
                        .child(
                            dither::Dither::new(
                                dither::DitherField::Glow {
                                    x: 1.,
                                    y: 0.,
                                    radius: 0.9,
                                    strength: 0.9,
                                }
                                .times(dither::DitherField::Fade {
                                    from: dither::Edge::Top,
                                    start: 1.,
                                    end: 0.,
                                }),
                            )
                            .color(tone.opacity(0.45))
                            .grid(3., 2.)
                            .toned()
                            .layer(),
                        ),
                )
            })
            .child(
                div()
                    .relative()
                    .size_full()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(11.))
                            .min_w_0()
                            .child(card.icon)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(text_px(if card.featured {
                                                14.5
                                            } else {
                                                13.5
                                            }))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(t.text)
                                            .child(card.title),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .font_family(crate::theme::mono())
                                            .text_size(text_px(10.5))
                                            .text_color(t.subtle)
                                            .child(card.subtitle),
                                    ),
                            )
                            .when(card.installed, |row| {
                                row.child(icon("circle-check").size(px(15.)).text_color(t.added))
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .text_size(text_px(12.))
                            .line_height(relative(1.45))
                            .text_color(t.muted)
                            .line_clamp(if card.featured { 3 } else { 2 })
                            .child(if card.description.is_empty() {
                                "No description".to_owned()
                            } else {
                                card.description
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .h(px(26.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .min_w_0()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .children(
                                        card.chips
                                            .into_iter()
                                            .take(2)
                                            .map(|(label, tone)| kit::chip(label, tone)),
                                    ),
                            )
                            .when(!card.marks.is_empty(), |row| {
                                row.child(self.provider_marks(&card.marks))
                            })
                            .child(div().flex_shrink_0().child(card.action)),
                    ),
            )
    }

    /// Lays out the cards of one row of the grid at equal widths. A short row keeps the widths
    /// of a full one.
    pub(in crate::ui) fn card_row(&self, cards: Vec<AnyElement>, columns: usize) -> Div {
        let missing = columns.saturating_sub(cards.len());
        div()
            .w_full()
            .flex()
            .gap(px(GAP))
            .children(cards)
            .children((0..missing).map(|_| div().flex_1().min_w_0()))
    }

    /// Centers one row of a Plugins page in the content's width.
    pub(in crate::ui) fn centered(&self, content: impl IntoElement) -> Div {
        div().w_full().flex().justify_center().child(
            div()
                .w_full()
                .max_w(px(CONTENT_WIDTH))
                .px(px(GUTTER))
                .child(content),
        )
    }

    /// Builds the banner at the top of a tab: a large tile over a dithered glow, a title, one
    /// line about the catalog, and figures.
    pub(in crate::ui) fn plugin_banner(
        &self,
        glyph: &'static str,
        title: &'static str,
        detail: String,
        figures: Vec<(String, &'static str)>,
    ) -> Div {
        let t = self.theme;
        let mut numbers = div().flex().flex_shrink_0().items_center().gap(px(22.));
        for (value, label) in figures {
            numbers = numbers.child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(2.))
                    .child(
                        div()
                            .font_family(crate::theme::mono())
                            .text_size(text_px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.text)
                            .child(value),
                    )
                    .child(caps(label, t)),
            );
        }
        kit::card(t)
            .w_full()
            .h(px(108.))
            .child(
                div().absolute().inset_0().child(
                    dither::Dither::new(
                        dither::DitherField::Glow {
                            x: 0.,
                            y: 0.5,
                            radius: 0.75,
                            strength: 0.9,
                        }
                        .times(dither::DitherField::Fade {
                            from: dither::Edge::Left,
                            start: 1.,
                            end: 0.,
                        }),
                    )
                    .color(t.accent.opacity(0.4))
                    .grid(3., 2.)
                    .toned()
                    .layer(),
                ),
            )
            .child(
                div()
                    .relative()
                    .size_full()
                    .px(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(sidebar::glyph_tile(glyph, t.accent, 52., 26.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(text_px(18.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.text)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(text_px(12.5))
                                    .line_height(relative(1.45))
                                    .text_color(t.muted)
                                    .line_clamp(2)
                                    .child(detail),
                            ),
                    )
                    .when(self.plugins_content_width() > 640., |row| {
                        row.child(numbers)
                    }),
            )
    }

    /// Builds the header of a section of a page: its label, a count, a dotted rule, and a note.
    pub(in crate::ui) fn plugin_section(
        &self,
        label: &str,
        count: Option<usize>,
        note: Option<AnyElement>,
    ) -> Div {
        div()
            .w_full()
            .h(px(40.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(kit::section_header(label, count, self.theme)),
            )
            .when_some(note, |row, note| row.child(note))
    }

    /// Wraps the parts of a detail page in a card with a block header.
    pub(in crate::ui) fn detail_card(
        &self,
        label: &str,
        tone: Hsla,
        body: impl IntoElement,
    ) -> Div {
        let t = self.theme;
        div()
            .w_full()
            .min_w_0()
            .rounded(px(crate::theme::RADIUS_SURFACE))
            .border_1()
            .border_color(t.border)
            .bg(t.raised)
            .shadow(kit::plate_light(t, true))
            .overflow_hidden()
            .child(kit::block_header(label, tone, t))
            .child(div().p(px(14.)).child(body))
    }

    /// Builds the head of a detail page: a large icon over a dithered glow in the tone, the
    /// title, a line of facts, badges and actions.
    pub(in crate::ui) fn detail_hero(
        &self,
        icon: AnyElement,
        title: String,
        facts: String,
        chips: Vec<(SharedString, Hsla)>,
        actions: Div,
        tone: Hsla,
    ) -> Div {
        let t = self.theme;
        kit::card(t)
            .w_full()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .w(px(420.))
                    .h_full()
                    .child(
                        dither::Dither::new(
                            dither::DitherField::Glow {
                                x: 1.,
                                y: 0.,
                                radius: 0.85,
                                strength: 0.95,
                            }
                            .times(dither::DitherField::Fade {
                                from: dither::Edge::Top,
                                start: 1.,
                                end: 0.2,
                            }),
                        )
                        .color(tone.opacity(0.45))
                        .grid(3., 2.)
                        .toned()
                        .layer(),
                    ),
            )
            .child(
                div()
                    .relative()
                    .p(px(20.))
                    .flex()
                    .items_start()
                    .gap(px(18.))
                    .child(icon)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(text_px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.text)
                                    .line_clamp(2)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .font_family(crate::theme::mono())
                                    .text_size(text_px(11.))
                                    .text_color(t.subtle)
                                    .child(facts),
                            )
                            .when(!chips.is_empty(), |d| {
                                d.child(
                                    div().pt(px(2.)).flex().flex_wrap().gap(px(5.)).children(
                                        chips
                                            .into_iter()
                                            .map(|(label, tone)| kit::chip(label, tone)),
                                    ),
                                )
                            })
                            .child(div().pt(px(6.)).child(actions.flex_wrap())),
                    ),
            )
    }
}
