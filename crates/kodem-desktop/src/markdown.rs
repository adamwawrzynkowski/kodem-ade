//! Structured Markdown for native, selectable chat content. HTML remains inert text.
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::{iter::Peekable, ops::Range};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
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
    Quote(Vec<Block>),
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
        let start = self.content.text.len();
        self.content.text.push_str(text);
        if start == self.content.text.len() {
            return;
        }
        let mut style = self.styles.last().cloned().unwrap_or_default();
        style.code |= code;
        if let Some(last) = self.content.spans.last_mut()
            && last.style == style
            && last.range.end == start
        {
            last.range.end = self.content.text.len();
        } else {
            self.content.spans.push(Span {
                range: start..self.content.text.len(),
                style,
            });
        }
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
            blocks.push(Block::Text(std::mem::take(&mut self.content)));
        }
    }
}

/// Parses message Markdown while retaining nested blocks and inline UTF-8 style ranges.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut events = Parser::new_ext(
        text,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
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
                blocks.push(Block::Quote(
                    read_blocks(events, Some(TagEnd::BlockQuote(kind))).blocks,
                ));
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
    inline.content
}
