//! Structured Markdown for native, selectable chat content. HTML remains inert text.
use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, Options, Parser, Tag, TagEnd,
};
use std::{iter::Peekable, ops::Range};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub quoted: bool,
    pub reference: bool,
    pub link: Option<String>,
}
impl InlineStyle {
    /// Identifies text displayed inside an inline message bubble.
    pub fn is_bubble(&self) -> bool {
        self.code || self.quoted || self.reference || self.link.is_some()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: InlineStyle,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichText {
    pub text: String,
    pub spans: Vec<Span>,
}
impl RichText {
    fn push(&mut self, text: &str, style: InlineStyle) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        if let Some(last) = self.spans.last_mut()
            && last.style == style
        {
            last.range.end = self.text.len();
        } else {
            self.spans.push(Span {
                range: start..self.text.len(),
                style,
            });
        }
    }

    fn formatted(self) -> Self {
        // Match quotes across emphasis/link events, but keep literal code and unfinished
        // streaming quotes intact. All ranges below still refer to the parsed source.
        let mut quotes = Vec::new();
        let mut open = None;
        let mut style_index = 0;
        for (index, ch) in self.text.char_indices() {
            while self.spans[style_index].range.end <= index {
                style_index += 1;
            }
            if self.spans[style_index].style.code {
                continue;
            }
            if ch == '\n' {
                open = None;
            } else if let Some((start, content_start, closing)) = open {
                if ch == closing {
                    if !self.text[content_start..index].trim().is_empty() {
                        quotes.push((start..content_start, index..index + ch.len_utf8()));
                    }
                    open = None;
                }
            } else if matches!(ch, '"' | '“') {
                open = Some((
                    index,
                    index + ch.len_utf8(),
                    if ch == '“' { '”' } else { '"' },
                ));
            }
        }
        let mut boundaries = vec![0, self.text.len()];
        boundaries.extend(self.spans.iter().map(|span| span.range.end));
        for (open, close) in &quotes {
            boundaries.extend([open.start, open.end, close.start, close.end]);
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut result = Self::default();
        let mut span_index = 0;
        let mut quote_index = 0;
        for pair in boundaries.windows(2) {
            let range = pair[0]..pair[1];
            while self.spans[span_index].range.end <= range.start {
                span_index += 1;
            }
            while quotes
                .get(quote_index)
                .is_some_and(|(_, close)| close.end <= range.start)
            {
                quote_index += 1;
            }
            let mut style = self.spans[span_index].style.clone();
            if let Some((open, close)) = quotes.get(quote_index) {
                if open.contains(&range.start) || close.contains(&range.start) {
                    continue;
                }
                style.quoted |= range.start >= open.end && range.end <= close.start;
            }
            let text = &self.text[range];
            for token in text.split_inclusive(char::is_whitespace) {
                let reference = token
                    .trim()
                    .trim_start_matches(|ch: char| "()[]{}\"“”,;!?".contains(ch))
                    .trim_end_matches(|ch: char| "()[]{}\"“”,;!?.".contains(ch));
                if style.link.is_none()
                    && !style.code
                    && let Some(url) = bare_url(reference)
                {
                    let start = token.find(reference).unwrap();
                    result.push(&token[..start], style.clone());
                    result.push(
                        reference,
                        InlineStyle {
                            link: Some(url),
                            ..style.clone()
                        },
                    );
                    result.push(&token[start + reference.len()..], style.clone());
                } else if is_reference(reference) {
                    let start = token.find(reference).unwrap();
                    result.push(&token[..start], style.clone());
                    result.push(
                        reference,
                        InlineStyle {
                            reference: true,
                            ..style.clone()
                        },
                    );
                    result.push(&token[start + reference.len()..], style.clone());
                } else {
                    result.push(token, style.clone());
                }
            }
        }
        result
    }
}

// Recognizes a web address written without link syntax and returns the address to open.
fn bare_url(text: &str) -> Option<String> {
    let rest = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"));
    if let Some(rest) = rest {
        return (rest.contains('.') || rest.starts_with("localhost")).then(|| text.to_owned());
    }
    let host = text.strip_prefix("www.")?;
    (host.contains('.') && host.chars().all(|ch| !ch.is_whitespace()))
        .then(|| format!("https://{text}"))
}

fn is_reference(text: &str) -> bool {
    if let Some(name) = text.strip_prefix('@') {
        return !name.is_empty()
            && name
                .chars()
                .all(|ch| ch.is_alphanumeric() || "_-/.:".contains(ch));
    }
    if text.contains("://") || text.contains('@') {
        return false;
    }
    let path = text.split([':', '#']).next().unwrap_or(text);
    if path.contains('/') {
        let explicit_path = path.starts_with('/')
            || path.starts_with("./")
            || path.starts_with("../")
            || path.starts_with("~/")
            || path.ends_with('/');
        let file_name = path.rsplit('/').next().unwrap_or_default();
        return path.chars().any(char::is_alphabetic) && (explicit_path || is_reference(file_name));
    }
    if matches!(
        path,
        "Dockerfile" | "Makefile" | "Justfile" | "LICENSE" | "README"
    ) || path.starts_with('.') && path.len() > 1
    {
        return true;
    }
    path.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty()
            && matches!(
                extension.to_ascii_lowercase().as_str(),
                "rs" | "toml"
                    | "lock"
                    | "md"
                    | "txt"
                    | "json"
                    | "jsonl"
                    | "yaml"
                    | "yml"
                    | "ts"
                    | "tsx"
                    | "js"
                    | "jsx"
                    | "html"
                    | "css"
                    | "scss"
                    | "svg"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "webp"
                    | "gif"
                    | "py"
                    | "go"
                    | "java"
                    | "kt"
                    | "swift"
                    | "c"
                    | "h"
                    | "cc"
                    | "cpp"
                    | "hpp"
                    | "cs"
                    | "sh"
                    | "bash"
                    | "zsh"
                    | "sql"
                    | "xml"
                    | "csv"
                    | "pdf"
                    | "unity"
                    | "prefab"
                    | "asset"
                    | "meta"
            )
    })
}
/// A GitHub alert kind, shown as a labeled callout instead of a plain quotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callout {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}
impl Callout {
    /// Returns the label shown at the top of the callout.
    pub fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }
}
#[derive(Clone, Debug)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}
#[derive(Clone, Debug)]
pub enum Block {
    Text(RichText),
    Heading {
        level: u8,
        text: RichText,
    },
    Code {
        language: String,
        text: String,
    },
    Quote {
        /// The kind of a GitHub alert such as `> [!NOTE]`; absent for a plain quotation.
        callout: Option<Callout>,
        blocks: Vec<Block>,
    },
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Table {
        alignments: Vec<Alignment>,
        header: Vec<RichText>,
        rows: Vec<Vec<RichText>>,
    },
    Rule,
}

#[derive(Default)]
struct InlineBuilder {
    content: RichText,
    styles: Vec<InlineStyle>,
}
impl InlineBuilder {
    fn push(&mut self, text: &str, code: bool) {
        let mut style = self.styles.last().cloned().unwrap_or_default();
        style.code |= code;
        self.content.push(text, style);
    }
    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Text(s) | Event::Html(s) | Event::InlineHtml(s) => self.push(&s, false),
            Event::Code(s) => self.push(&s, true),
            Event::SoftBreak => self.push(" ", false),
            Event::HardBreak => self.push("\n", false),
            Event::Start(
                tag @ (Tag::Strong
                | Tag::Emphasis
                | Tag::Strikethrough
                | Tag::Link { .. }
                | Tag::Image { .. }),
            ) => {
                let mut style = self.styles.last().cloned().unwrap_or_default();
                match tag {
                    Tag::Strong => style.bold = true,
                    Tag::Emphasis => style.italic = true,
                    Tag::Strikethrough => style.strike = true,
                    Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                        style.link = Some(dest_url.into_string())
                    }
                    _ => {}
                }
                self.styles.push(style);
            }
            Event::End(
                TagEnd::Strong
                | TagEnd::Emphasis
                | TagEnd::Strikethrough
                | TagEnd::Link
                | TagEnd::Image,
            ) => {
                self.styles.pop();
            }
            _ => {}
        }
    }
    fn flush(&mut self, blocks: &mut Vec<Block>) {
        if !self.content.text.is_empty() {
            blocks.push(Block::Text(std::mem::take(&mut self.content).formatted()));
        }
    }
}

/// Parsed messages kept per generation before the older generation is dropped.
const CACHED_MESSAGES: usize = 192;

#[derive(Default)]
struct BlockCache {
    recent: std::collections::HashMap<(u64, usize), Vec<Block>>,
    older: std::collections::HashMap<(u64, usize), Vec<Block>>,
}

thread_local! {
    static BLOCK_CACHE: std::cell::RefCell<BlockCache> = std::cell::RefCell::default();
}

/// Parses message Markdown while retaining nested blocks and inline UTF-8 style ranges.
///
/// A conversation renders its visible messages again on every frame, so the blocks of the
/// messages seen lately are kept and only changed text is parsed.
pub fn blocks(text: &str) -> Vec<Block> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    let key = (hasher.finish(), text.len());
    BLOCK_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(blocks) = cache.recent.get(&key) {
            return blocks.clone();
        }
        let blocks = match cache.older.remove(&key) {
            Some(blocks) => blocks,
            None => parse_blocks(text),
        };
        if cache.recent.len() >= CACHED_MESSAGES {
            cache.older = std::mem::take(&mut cache.recent);
        }
        cache.recent.insert(key, blocks.clone());
        blocks
    })
}

fn parse_blocks(text: &str) -> Vec<Block> {
    let mut events = Parser::new_ext(
        text,
        Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_GFM,
    )
    .peekable();
    read_blocks(&mut events, None).blocks
}

fn read_blocks<'a>(
    events: &mut Peekable<impl Iterator<Item = Event<'a>>>,
    end: Option<TagEnd>,
) -> ListItem {
    let mut blocks = Vec::new();
    let mut checked = None;
    let mut inline = InlineBuilder::default();
    while let Some(event) = events.next() {
        if matches!(&event, Event::End(tag) if Some(*tag) == end) {
            break;
        }
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                inline.flush(&mut blocks);
                blocks.push(Block::Heading {
                    level: level as u8,
                    text: read_inline(events, TagEnd::Heading(level)),
                });
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                inline.flush(&mut blocks);
                let language = match kind {
                    CodeBlockKind::Fenced(s) => {
                        s.split_whitespace().next().unwrap_or_default().to_owned()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                let mut text = String::new();
                for event in events.by_ref() {
                    match event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(s) => text.push_str(&s),
                        _ => {}
                    }
                }
                blocks.push(Block::Code { language, text });
            }
            Event::Start(Tag::BlockQuote(kind)) => {
                inline.flush(&mut blocks);
                blocks.push(Block::Quote {
                    callout: kind.map(|kind| match kind {
                        BlockQuoteKind::Note => Callout::Note,
                        BlockQuoteKind::Tip => Callout::Tip,
                        BlockQuoteKind::Important => Callout::Important,
                        BlockQuoteKind::Warning => Callout::Warning,
                        BlockQuoteKind::Caution => Callout::Caution,
                    }),
                    blocks: read_blocks(events, Some(TagEnd::BlockQuote(kind))).blocks,
                });
            }
            Event::Start(Tag::List(start)) => {
                inline.flush(&mut blocks);
                let mut items = Vec::new();
                while let Some(event) = events.next() {
                    match event {
                        Event::Start(Tag::Item) => {
                            items.push(read_blocks(events, Some(TagEnd::Item)))
                        }
                        Event::End(TagEnd::List(_)) => break,
                        _ => {}
                    }
                }
                blocks.push(Block::List { start, items });
            }
            Event::Start(Tag::Table(alignments)) => {
                inline.flush(&mut blocks);
                let mut header = Vec::new();
                let mut rows = Vec::new();
                let mut row = Vec::new();
                while let Some(event) = events.next() {
                    match event {
                        Event::Start(Tag::TableCell) => {
                            row.push(read_inline(events, TagEnd::TableCell))
                        }
                        Event::End(TagEnd::TableHead) => header = std::mem::take(&mut row),
                        Event::End(TagEnd::TableRow) => rows.push(std::mem::take(&mut row)),
                        Event::End(TagEnd::Table) => break,
                        _ => {}
                    }
                }
                blocks.push(Block::Table {
                    alignments,
                    header,
                    rows,
                });
            }
            Event::Rule => {
                inline.flush(&mut blocks);
                blocks.push(Block::Rule);
            }
            Event::TaskListMarker(done) => checked = Some(done),
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph | TagEnd::HtmlBlock) => {
                inline.flush(&mut blocks)
            }
            other => inline.event(other),
        }
    }
    inline.flush(&mut blocks);
    ListItem { checked, blocks }
}
fn read_inline<'a>(events: &mut impl Iterator<Item = Event<'a>>, end: TagEnd) -> RichText {
    let mut inline = InlineBuilder::default();
    for event in events.by_ref() {
        if matches!(&event, Event::End(tag) if *tag == end) {
            break;
        }
        inline.event(event);
    }
    inline.content.formatted()
}
