//! Reads and checks `changelog/highlights.toml`, and writes the entries that this release shows
//! as Rust data for `release_notes.rs`. A mistake in the file stops the build with its reason.
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt::Write;

/// The file, relative to this crate.
const SOURCE: &str = "../../changelog/highlights.toml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    reviewed: String,
    #[serde(default)]
    headline: Vec<Headline>,
    #[serde(default)]
    highlight: Vec<Highlight>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Headline {
    id: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Highlight {
    id: String,
    title: String,
    text: String,
    #[serde(default)]
    wide: bool,
    #[serde(default)]
    tone: Option<Tone>,
    visual: Visual,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Tone {
    Accent,
    Green,
    Red,
    Amber,
    Blue,
    Muted,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum Visual {
    List {
        title: Option<String>,
        badge: Option<String>,
        items: Vec<Row>,
    },
    Chips {
        items: Vec<Chip>,
    },
    Prompt {
        text: String,
        #[serde(default)]
        chips: Vec<Chip>,
    },
    Bars {
        label: Option<String>,
        values: Vec<f32>,
        #[serde(default)]
        labels: Vec<String>,
        focus: Option<usize>,
    },
    Figure {
        value: String,
        label: String,
        detail: Option<String>,
    },
    Meter {
        segments: Vec<Segment>,
        caption: Option<String>,
    },
    Keys {
        keys: Vec<String>,
        label: Option<String>,
    },
    Glyph {
        icon: String,
    },
    Tiles {
        items: Vec<Tile>,
    },
    Toggles {
        items: Vec<Toggle>,
    },
    Sliders {
        items: Vec<Slider>,
    },
    Compare {
        before: Chip,
        after: Chip,
        label: Option<String>,
    },
    Menu {
        items: Vec<MenuItem>,
        focus: Option<usize>,
    },
    Window {
        area: Area,
        label: Option<String>,
    },
    Gauge {
        value: f32,
        text: Option<String>,
        label: String,
    },
    Steps {
        items: Vec<String>,
        current: usize,
    },
    Diff {
        file: Option<String>,
        lines: Vec<DiffLine>,
    },
    Notice {
        icon: String,
        title: String,
        text: Option<String>,
        #[serde(default)]
        actions: Vec<String>,
    },
    Choice {
        question: String,
        options: Vec<String>,
        chosen: Option<usize>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    text: String,
    state: RowState,
    tag: Option<String>,
    tone: Option<Tone>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum RowState {
    Done,
    Open,
    Dot,
    Faded,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Chip {
    text: String,
    icon: Option<String>,
    detail: Option<String>,
    added: Option<u32>,
    removed: Option<u32>,
    tone: Option<Tone>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Segment {
    label: String,
    weight: f32,
    tone: Option<Tone>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tile {
    text: String,
    icon: String,
    badge: Option<String>,
    tone: Option<Tone>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Toggle {
    text: String,
    on: bool,
    detail: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Slider {
    text: String,
    value: f32,
    readout: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MenuItem {
    text: String,
    icon: Option<String>,
    keys: Option<String>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Area {
    Titlebar,
    Sidebar,
    Chat,
    Composer,
    Terminal,
    Panel,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiffLine {
    text: String,
    change: Change,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Change {
    Added,
    Removed,
    Same,
}

/// The characters that the dot-matrix figures of `ui/whats_new.rs` can draw.
const FIGURE_CHARACTERS: &str = "0123456789.,%+-x:/";

/// Characters of any text that a visual shows, so that it fits the stage of a narrow card.
const VISUAL_TEXT: usize = 44;

/// Characters of a label under a tile or a step, which shares the stage's width with others.
const SHORT_TEXT: usize = 14;

/// Characters of the reading in the middle of a gauge's ring.
const READING: usize = 5;

/// How many highlights above a highlight must use other visual kinds, so that cards near each
/// other on the board look different.
const DISTINCT_ABOVE: usize = 3;

/// Checks the file and writes `highlights.rs` into `out`. `chosen` lists the ids of the entries
/// that this release shows; `None` shows every entry.
pub fn embed(out: &std::path::Path, chosen: Option<&str>) {
    println!("cargo:rerun-if-changed={SOURCE}");
    let source = std::fs::read_to_string(SOURCE).expect("Read changelog/highlights.toml");
    let file: File = toml::from_str(&source)
        .unwrap_or_else(|error| panic!("changelog/highlights.toml: {error}"));
    let mut problems = Vec::new();
    check(&file, &source, &mut problems);
    if !problems.is_empty() {
        panic!(
            "changelog/highlights.toml has mistakes:\n  {}",
            problems.join("\n  ")
        );
    }
    let chosen: Option<HashSet<&str>> = chosen.map(|ids| ids.split_whitespace().collect());
    let shown = |id: &str| chosen.as_ref().is_none_or(|ids| ids.contains(id));
    let mut code = String::from("// Generated by build.rs from changelog/highlights.toml.\n");
    let headline = file.headline.iter().find(|headline| shown(&headline.id));
    writeln!(
        code,
        "pub static HEADLINE: Option<&str> = {};",
        optional(headline.map(|headline| headline.text.as_str()))
    )
    .unwrap();
    code.push_str("pub static HIGHLIGHTS: &[Highlight] = &[\n");
    for highlight in file
        .highlight
        .iter()
        .filter(|highlight| shown(&highlight.id))
    {
        let tone = highlight.tone.unwrap_or(Tone::Accent);
        writeln!(
            code,
            "    Highlight {{ title: {}, text: {}, wide: {}, tone: {}, visual: {} }},",
            literal(&highlight.title),
            literal(&highlight.text),
            highlight.wide,
            tone_path(tone),
            visual(&highlight.visual, tone),
        )
        .unwrap();
    }
    code.push_str("];\n");
    std::fs::write(out.join("highlights.rs"), code).expect("Write the release highlights");
}

// Collects every mistake of the file, so one build reports them all.
fn check(file: &File, source: &str, problems: &mut Vec<String>) {
    if !file.reviewed.is_empty()
        && !(file.reviewed.len() == 40
            && file
                .reviewed
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    {
        problems.push("reviewed must be a full lowercase commit hash".into());
    }
    let ids: Vec<&str> = file
        .headline
        .iter()
        .map(|headline| headline.id.as_str())
        .chain(file.highlight.iter().map(|highlight| highlight.id.as_str()))
        .collect();
    let mut seen = HashSet::new();
    for id in &ids {
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            problems.push(format!(
                "id {id:?} may hold only lowercase letters, digits and hyphens"
            ));
        }
        if !seen.insert(*id) {
            problems.push(format!("id {id:?} is used twice"));
        }
        // The release scripts find the ids with a line pattern, so each must stand on its own
        // line exactly so.
        let line = format!("id = \"{id}\"");
        if !source.lines().any(|candidate| candidate == line) {
            problems.push(format!("write the id {id:?} on its own line as `{line}`"));
        }
    }
    let lines = source
        .lines()
        .filter(|line| line.starts_with("id = \""))
        .count();
    if lines != ids.len() {
        problems.push("every `id = \"…\"` line must start an entry's id, and only once".into());
    }
    for headline in &file.headline {
        limit(&headline.id, "text", &headline.text, 140, problems);
        version_number(&headline.id, &headline.text, problems);
    }
    for highlight in &file.highlight {
        let id = &highlight.id;
        limit(id, "title", &highlight.title, 40, problems);
        limit(id, "text", &highlight.text, 160, problems);
        check_visual(id, &highlight.visual, problems);
        let shown = visual_texts(&highlight.visual);
        for text in &shown {
            if text.chars().count() > VISUAL_TEXT {
                problems.push(format!(
                    "{id}: {text:?} is longer than the {VISUAL_TEXT} characters that a visual \
                     shows"
                ));
            }
        }
        for text in [highlight.title.as_str(), highlight.text.as_str()]
            .into_iter()
            .chain(shown)
        {
            version_number(id, text, problems);
        }
    }
    let kinds: Vec<&str> = file
        .highlight
        .iter()
        .map(|highlight| kind(&highlight.visual))
        .collect();
    for (index, highlight) in file.highlight.iter().enumerate() {
        if let Some(above) = (index.saturating_sub(DISTINCT_ABOVE)..index)
            .find(|&above| kinds[above] == kinds[index])
        {
            problems.push(format!(
                "{}: its visual kind {:?} repeats that of {}; a highlight takes a kind that none \
                 of the {DISTINCT_ABOVE} highlights above it uses",
                highlight.id, kinds[index], file.highlight[above].id
            ));
        }
    }
}

// Names the kind of a visual as the file writes it.
fn kind(visual: &Visual) -> &'static str {
    match visual {
        Visual::List { .. } => "list",
        Visual::Chips { .. } => "chips",
        Visual::Prompt { .. } => "prompt",
        Visual::Bars { .. } => "bars",
        Visual::Figure { .. } => "figure",
        Visual::Meter { .. } => "meter",
        Visual::Keys { .. } => "keys",
        Visual::Glyph { .. } => "glyph",
        Visual::Tiles { .. } => "tiles",
        Visual::Toggles { .. } => "toggles",
        Visual::Sliders { .. } => "sliders",
        Visual::Compare { .. } => "compare",
        Visual::Menu { .. } => "menu",
        Visual::Window { .. } => "window",
        Visual::Gauge { .. } => "gauge",
        Visual::Steps { .. } => "steps",
        Visual::Diff { .. } => "diff",
        Visual::Notice { .. } => "notice",
        Visual::Choice { .. } => "choice",
    }
}

fn check_visual(id: &str, visual: &Visual, problems: &mut Vec<String>) {
    let mut count = |what: &str, length: usize, range: std::ops::RangeInclusive<usize>| {
        if !range.contains(&length) {
            problems.push(format!(
                "{id}: a {what} holds {} to {} entries",
                range.start(),
                range.end()
            ));
        }
    };
    match visual {
        Visual::List { items, .. } => count("list", items.len(), 1..=4),
        Visual::Chips { items } => count("chips visual", items.len(), 1..=4),
        Visual::Prompt { chips, .. } => count("prompt's chips", chips.len(), 0..=3),
        Visual::Bars { values, .. } => count("bars visual", values.len(), 2..=8),
        Visual::Meter { segments, .. } => count("meter", segments.len(), 2..=4),
        Visual::Keys { keys, .. } => count("keys visual", keys.len(), 1..=4),
        Visual::Tiles { items } => count("tiles visual", items.len(), 1..=4),
        Visual::Toggles { items } => count("toggles visual", items.len(), 1..=3),
        Visual::Sliders { items } => count("sliders visual", items.len(), 1..=3),
        Visual::Menu { items, .. } => count("menu", items.len(), 2..=4),
        Visual::Steps { items, .. } => count("steps visual", items.len(), 2..=4),
        Visual::Diff { lines, .. } => count("diff", lines.len(), 1..=4),
        Visual::Notice { actions, .. } => count("notice's actions", actions.len(), 0..=2),
        Visual::Choice { options, .. } => count("choice", options.len(), 2..=3),
        Visual::Figure { .. }
        | Visual::Glyph { .. }
        | Visual::Compare { .. }
        | Visual::Window { .. }
        | Visual::Gauge { .. } => {}
    }
    let mut short = |what: &str, text: &str, most: usize| {
        if text.chars().count() > most {
            problems.push(format!(
                "{id}: the {what} {text:?} is longer than {most} characters"
            ));
        }
    };
    match visual {
        Visual::Tiles { items } => items
            .iter()
            .for_each(|tile| short("tile", &tile.text, SHORT_TEXT)),
        Visual::Steps { items, .. } => items
            .iter()
            .for_each(|step| short("step", step, SHORT_TEXT)),
        Visual::Gauge {
            text: Some(text), ..
        } => short("gauge's reading", text, READING),
        _ => {}
    }
    match visual {
        Visual::Chips { items } | Visual::Prompt { chips: items, .. } => {
            for chip in items {
                if let Some(icon) = &chip.icon {
                    check_icon(id, icon, problems);
                }
            }
        }
        Visual::Compare { before, after, .. } => {
            for icon in [&before.icon, &after.icon].into_iter().flatten() {
                check_icon(id, icon, problems);
            }
        }
        Visual::Tiles { items } => {
            for tile in items {
                check_icon(id, &tile.icon, problems);
            }
        }
        Visual::Menu { items, focus } => {
            for icon in items.iter().filter_map(|item| item.icon.as_ref()) {
                check_icon(id, icon, problems);
            }
            if focus.is_some_and(|focus| focus >= items.len()) {
                problems.push(format!("{id}: focus must be the index of a menu row"));
            }
        }
        Visual::Notice { icon, .. } => check_icon(id, icon, problems),
        Visual::Sliders { items } => {
            if items
                .iter()
                .any(|slider| !(0. ..=1.).contains(&slider.value))
            {
                problems.push(format!("{id}: a slider's value is from 0 to 1"));
            }
        }
        Visual::Gauge { value, .. } => {
            if !(0. ..=1.).contains(value) {
                problems.push(format!("{id}: a gauge's value is from 0 to 1"));
            }
        }
        Visual::Steps { items, current } => {
            if *current > items.len() {
                problems.push(format!(
                    "{id}: current is the index of a step, or the count of steps when all are done"
                ));
            }
        }
        Visual::Choice {
            options, chosen, ..
        } => {
            if chosen.is_some_and(|chosen| chosen >= options.len()) {
                problems.push(format!("{id}: chosen must be the index of an option"));
            }
        }
        Visual::Bars {
            values,
            labels,
            focus,
            ..
        } => {
            if values.iter().any(|value| !value.is_finite() || *value < 0.)
                || values.iter().all(|value| *value == 0.)
            {
                problems.push(format!(
                    "{id}: bar values must be zero or more, and one above zero"
                ));
            }
            if !labels.is_empty() && labels.len() != values.len() {
                problems.push(format!("{id}: give every bar a label, or none"));
            }
            if focus.is_some_and(|focus| focus >= values.len()) {
                problems.push(format!("{id}: focus must be the index of a bar"));
            }
        }
        Visual::Figure { value, .. } => {
            if value.is_empty()
                || value.chars().count() > 6
                || !value.chars().all(|c| FIGURE_CHARACTERS.contains(c))
            {
                problems.push(format!(
                    "{id}: a figure is 1 to 6 characters of {FIGURE_CHARACTERS}"
                ));
            }
        }
        Visual::Meter { segments, .. } => {
            if segments
                .iter()
                .any(|segment| !segment.weight.is_finite() || segment.weight <= 0.)
            {
                problems.push(format!("{id}: meter weights must be above zero"));
            }
        }
        Visual::Keys { keys, .. } => {
            if keys
                .iter()
                .any(|key| key.is_empty() || key.chars().count() > 5)
            {
                problems.push(format!("{id}: a key is 1 to 5 characters"));
            }
        }
        Visual::Glyph { icon } => check_icon(id, icon, problems),
        Visual::List { .. }
        | Visual::Toggles { .. }
        | Visual::Window { .. }
        | Visual::Diff { .. } => {}
    }
}

// Every text that a visual shows.
fn visual_texts(visual: &Visual) -> Vec<&str> {
    let mut texts = Vec::new();
    match visual {
        Visual::List {
            title,
            badge,
            items,
        } => {
            texts.extend(title.as_deref());
            texts.extend(badge.as_deref());
            for row in items {
                texts.push(row.text.as_str());
                texts.extend(row.tag.as_deref());
            }
        }
        Visual::Chips { items } => chip_texts(items, &mut texts),
        Visual::Prompt { text, chips } => {
            texts.push(text.as_str());
            chip_texts(chips, &mut texts);
        }
        Visual::Bars { label, labels, .. } => {
            texts.extend(label.as_deref());
            texts.extend(labels.iter().map(String::as_str));
        }
        Visual::Figure {
            value,
            label,
            detail,
        } => {
            texts.push(value.as_str());
            texts.push(label.as_str());
            texts.extend(detail.as_deref());
        }
        Visual::Meter { segments, caption } => {
            texts.extend(segments.iter().map(|segment| segment.label.as_str()));
            texts.extend(caption.as_deref());
        }
        Visual::Keys { keys, label } => {
            texts.extend(keys.iter().map(String::as_str));
            texts.extend(label.as_deref());
        }
        Visual::Glyph { .. } => {}
        Visual::Tiles { items } => {
            for tile in items {
                texts.push(tile.text.as_str());
                texts.extend(tile.badge.as_deref());
            }
        }
        Visual::Toggles { items } => {
            for toggle in items {
                texts.push(toggle.text.as_str());
                texts.extend(toggle.detail.as_deref());
            }
        }
        Visual::Sliders { items } => {
            for slider in items {
                texts.push(slider.text.as_str());
                texts.extend(slider.readout.as_deref());
            }
        }
        Visual::Compare {
            before,
            after,
            label,
        } => {
            chip_texts(std::slice::from_ref(before), &mut texts);
            chip_texts(std::slice::from_ref(after), &mut texts);
            texts.extend(label.as_deref());
        }
        Visual::Menu { items, .. } => {
            for item in items {
                texts.push(item.text.as_str());
                texts.extend(item.keys.as_deref());
            }
        }
        Visual::Window { label, .. } => texts.extend(label.as_deref()),
        Visual::Gauge { text, label, .. } => {
            texts.extend(text.as_deref());
            texts.push(label.as_str());
        }
        Visual::Steps { items, .. } => texts.extend(items.iter().map(String::as_str)),
        Visual::Diff { file, lines } => {
            texts.extend(file.as_deref());
            texts.extend(lines.iter().map(|line| line.text.as_str()));
        }
        Visual::Notice {
            title,
            text,
            actions,
            ..
        } => {
            texts.push(title.as_str());
            texts.extend(text.as_deref());
            texts.extend(actions.iter().map(String::as_str));
        }
        Visual::Choice {
            question, options, ..
        } => {
            texts.push(question.as_str());
            texts.extend(options.iter().map(String::as_str));
        }
    }
    texts
}

fn chip_texts<'a>(chips: &'a [Chip], texts: &mut Vec<&'a str>) {
    for chip in chips {
        texts.push(chip.text.as_str());
        texts.extend(chip.detail.as_deref());
    }
}

// Refuses version numbers such as 1.19.3: a card says what the user can do now, not which
// versions a change moves between.
fn version_number(id: &str, text: &str, problems: &mut Vec<String>) {
    let found = text
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|token| token.trim_matches('.'))
        .find(|token| {
            let parts: Vec<&str> = token.split('.').collect();
            parts.len() >= 3 && parts.iter().all(|part| !part.is_empty())
        });
    if let Some(number) = found {
        problems.push(format!(
            "{id}: {number:?} is a version number; say what the user can do now, not which \
             versions a change moves between"
        ));
    }
}

// Requires an icon that the app embeds.
fn check_icon(id: &str, icon: &str, problems: &mut Vec<String>) {
    let embedded = std::fs::read_to_string("src/icons.rs")
        .is_ok_and(|table| table.contains(&format!("\"{icon}\",")));
    if !embedded || !std::path::Path::new(&format!("../../assets/icons/{icon}.svg")).is_file() {
        problems.push(format!("{id}: icon {icon:?} is not in assets/icons"));
    }
}

fn limit(id: &str, field: &str, text: &str, most: usize, problems: &mut Vec<String>) {
    if text.trim().is_empty() || text.chars().count() > most {
        problems.push(format!("{id}: {field} must have 1 to {most} characters"));
    }
}

// Rust source for each part of an entry.

fn literal(text: &str) -> String {
    format!("{text:?}")
}

fn optional(text: Option<&str>) -> String {
    text.map_or_else(|| "None".into(), |text| format!("Some({text:?})"))
}

fn tone_path(tone: Tone) -> &'static str {
    match tone {
        Tone::Accent => "Tone::Accent",
        Tone::Green => "Tone::Green",
        Tone::Red => "Tone::Red",
        Tone::Amber => "Tone::Amber",
        Tone::Blue => "Tone::Blue",
        Tone::Muted => "Tone::Muted",
    }
}

// A capsule without its own tone takes `fallback`.
fn chip(chip: &Chip, fallback: Tone) -> String {
    format!(
        "Chip {{ text: {}, icon: {}, detail: {}, added: {:?}, removed: {:?}, tone: {} }}",
        literal(&chip.text),
        optional(chip.icon.as_deref()),
        optional(chip.detail.as_deref()),
        chip.added,
        chip.removed,
        tone_path(chip.tone.unwrap_or(fallback)),
    )
}

fn chips(chips: &[Chip]) -> String {
    let items: Vec<String> = chips.iter().map(|item| chip(item, Tone::Muted)).collect();
    format!("&[{}]", items.join(", "))
}

fn strings(texts: &[String]) -> String {
    let items: Vec<String> = texts.iter().map(|text| literal(text)).collect();
    format!("&[{}]", items.join(", "))
}

// A part without its own tone takes the card's.
fn visual(visual: &Visual, card: Tone) -> String {
    match visual {
        Visual::List {
            title,
            badge,
            items,
        } => {
            let rows: Vec<String> = items
                .iter()
                .map(|row| {
                    let state = match row.state {
                        RowState::Done => "RowState::Done",
                        RowState::Open => "RowState::Open",
                        RowState::Dot => "RowState::Dot",
                        RowState::Faded => "RowState::Faded",
                    };
                    format!(
                        "Row {{ text: {}, state: {state}, tag: {}, tone: {} }}",
                        literal(&row.text),
                        optional(row.tag.as_deref()),
                        tone_path(row.tone.unwrap_or(card)),
                    )
                })
                .collect();
            format!(
                "Visual::List {{ title: {}, badge: {}, items: &[{}] }}",
                optional(title.as_deref()),
                optional(badge.as_deref()),
                rows.join(", ")
            )
        }
        Visual::Chips { items } => format!("Visual::Chips({})", chips(items)),
        Visual::Prompt { text, chips: items } => format!(
            "Visual::Prompt {{ text: {}, chips: {} }}",
            literal(text),
            chips(items)
        ),
        Visual::Bars {
            label,
            values,
            labels,
            focus,
        } => {
            let bars: Vec<String> = values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    format!(
                        "Bar {{ value: {value:?}, label: {} }}",
                        optional(labels.get(index).map(String::as_str))
                    )
                })
                .collect();
            format!(
                "Visual::Bars {{ label: {}, bars: &[{}], focus: {focus:?} }}",
                optional(label.as_deref()),
                bars.join(", ")
            )
        }
        Visual::Figure {
            value,
            label,
            detail,
        } => format!(
            "Visual::Figure {{ value: {}, label: {}, detail: {} }}",
            literal(value),
            literal(label),
            optional(detail.as_deref())
        ),
        Visual::Meter { segments, caption } => {
            // Parts without a tone take the card's, then quieter tones in turn.
            let fallback = [card, Tone::Muted, Tone::Blue, Tone::Amber];
            let parts: Vec<String> = segments
                .iter()
                .enumerate()
                .map(|(index, segment)| {
                    format!(
                        "Segment {{ label: {}, weight: {:?}, tone: {} }}",
                        literal(&segment.label),
                        segment.weight,
                        tone_path(segment.tone.unwrap_or(fallback[index % fallback.len()])),
                    )
                })
                .collect();
            format!(
                "Visual::Meter {{ segments: &[{}], caption: {} }}",
                parts.join(", "),
                optional(caption.as_deref())
            )
        }
        Visual::Keys { keys, label } => {
            let keys: Vec<String> = keys.iter().map(|key| literal(key)).collect();
            format!(
                "Visual::Keys {{ keys: &[{}], label: {} }}",
                keys.join(", "),
                optional(label.as_deref())
            )
        }
        Visual::Glyph { icon } => format!("Visual::Glyph({})", literal(icon)),
        Visual::Tiles { items } => {
            let tiles: Vec<String> = items
                .iter()
                .map(|tile| {
                    format!(
                        "Tile {{ text: {}, icon: {}, badge: {}, tone: {} }}",
                        literal(&tile.text),
                        literal(&tile.icon),
                        optional(tile.badge.as_deref()),
                        tone_path(tile.tone.unwrap_or(card)),
                    )
                })
                .collect();
            format!("Visual::Tiles(&[{}])", tiles.join(", "))
        }
        Visual::Toggles { items } => {
            let rows: Vec<String> = items
                .iter()
                .map(|toggle| {
                    format!(
                        "Toggle {{ text: {}, on: {}, detail: {} }}",
                        literal(&toggle.text),
                        toggle.on,
                        optional(toggle.detail.as_deref()),
                    )
                })
                .collect();
            format!("Visual::Toggles(&[{}])", rows.join(", "))
        }
        Visual::Sliders { items } => {
            let rows: Vec<String> = items
                .iter()
                .map(|slider| {
                    format!(
                        "Slider {{ text: {}, value: {:?}, readout: {} }}",
                        literal(&slider.text),
                        slider.value,
                        optional(slider.readout.as_deref()),
                    )
                })
                .collect();
            format!("Visual::Sliders(&[{}])", rows.join(", "))
        }
        Visual::Compare {
            before,
            after,
            label,
        } => format!(
            "Visual::Compare {{ before: {}, after: {}, label: {} }}",
            chip(before, Tone::Muted),
            chip(after, card),
            optional(label.as_deref())
        ),
        Visual::Menu { items, focus } => {
            let rows: Vec<String> = items
                .iter()
                .map(|item| {
                    format!(
                        "MenuItem {{ text: {}, icon: {}, keys: {} }}",
                        literal(&item.text),
                        optional(item.icon.as_deref()),
                        optional(item.keys.as_deref()),
                    )
                })
                .collect();
            format!(
                "Visual::Menu {{ items: &[{}], focus: {focus:?} }}",
                rows.join(", ")
            )
        }
        Visual::Window { area, label } => {
            let area = match area {
                Area::Titlebar => "Area::Titlebar",
                Area::Sidebar => "Area::Sidebar",
                Area::Chat => "Area::Chat",
                Area::Composer => "Area::Composer",
                Area::Terminal => "Area::Terminal",
                Area::Panel => "Area::Panel",
            };
            format!(
                "Visual::Window {{ area: {area}, label: {} }}",
                optional(label.as_deref())
            )
        }
        Visual::Gauge { value, text, label } => format!(
            "Visual::Gauge {{ value: {value:?}, text: {}, label: {} }}",
            optional(text.as_deref()),
            literal(label)
        ),
        Visual::Steps { items, current } => format!(
            "Visual::Steps {{ items: {}, current: {current} }}",
            strings(items)
        ),
        Visual::Diff { file, lines } => {
            let rows: Vec<String> = lines
                .iter()
                .map(|line| {
                    let change = match line.change {
                        Change::Added => "Change::Added",
                        Change::Removed => "Change::Removed",
                        Change::Same => "Change::Same",
                    };
                    format!(
                        "DiffLine {{ text: {}, change: {change} }}",
                        literal(&line.text)
                    )
                })
                .collect();
            format!(
                "Visual::Diff {{ file: {}, lines: &[{}] }}",
                optional(file.as_deref()),
                rows.join(", ")
            )
        }
        Visual::Notice {
            icon,
            title,
            text,
            actions,
        } => format!(
            "Visual::Notice {{ icon: {}, title: {}, text: {}, actions: {} }}",
            literal(icon),
            literal(title),
            optional(text.as_deref()),
            strings(actions)
        ),
        Visual::Choice {
            question,
            options,
            chosen,
        } => format!(
            "Visual::Choice {{ question: {}, options: {}, chosen: {chosen:?} }}",
            literal(question),
            strings(options)
        ),
    }
}
