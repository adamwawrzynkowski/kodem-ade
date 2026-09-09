//! Native Markdown block parsing. Raw HTML never becomes executable UI.
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
#[derive(Clone)]
pub enum Block {
    Text(String),
    Heading(String),
    Code { language: String, text: String },
    Quote(String),
    Table(String),
}
/// Parses a single message into native, selectable blocks.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut result = Vec::new();
    let mut buffer = String::new();
    let mut code = None;
    let mut heading = false;
    let mut quote = false;
    let mut table = false;
    for event in Parser::new_ext(
        text,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    ) {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                flush(&mut result, &mut buffer, heading, quote, table);
                code = Some(match kind {
                    CodeBlockKind::Fenced(s) => s.to_string(),
                    _ => String::new(),
                });
            }
            Event::End(TagEnd::CodeBlock) => {
                result.push(Block::Code {
                    language: code.take().unwrap_or_default(),
                    text: std::mem::take(&mut buffer),
                });
            }
            Event::Start(Tag::Heading { .. }) => heading = true,
            Event::End(TagEnd::Heading(_)) => {
                flush(&mut result, &mut buffer, heading, quote, table);
                heading = false
            }
            Event::Start(Tag::BlockQuote(_)) => quote = true,
            Event::End(TagEnd::BlockQuote(_)) => {
                flush(&mut result, &mut buffer, heading, quote, table);
                quote = false
            }
            Event::Start(Tag::Table(_)) => table = true,
            Event::End(TagEnd::Table) => {
                flush(&mut result, &mut buffer, heading, quote, table);
                table = false
            }
            Event::End(TagEnd::TableCell) => buffer.push_str("    │    "),
            Event::End(TagEnd::TableRow) => buffer.push('\n'),
            Event::Start(Tag::Item) => buffer.push_str("•  "),
            Event::End(TagEnd::Item) => flush(&mut result, &mut buffer, heading, quote, table),
            Event::Text(s) | Event::Code(s) => buffer.push_str(&s),
            Event::SoftBreak | Event::HardBreak => buffer.push('\n'),
            Event::End(TagEnd::Paragraph) if code.is_none() => {
                flush(&mut result, &mut buffer, heading, quote, table)
            }
            Event::TaskListMarker(done) => buffer.push_str(if done { "[x] " } else { "[ ] " }),
            Event::Start(Tag::Link { dest_url, .. })
                if (dest_url.starts_with("http://") || dest_url.starts_with("https://")) =>
            {
                buffer.push_str(&format!("{dest_url} "));
            }
            _ => {}
        }
    }
    if let Some(language) = code {
        result.push(Block::Code {
            language,
            text: buffer,
        })
    } else {
        flush(&mut result, &mut buffer, heading, quote, table)
    }
    result
}
fn flush(result: &mut Vec<Block>, buffer: &mut String, heading: bool, quote: bool, table: bool) {
    if !buffer.trim().is_empty() {
        let text = std::mem::take(buffer);
        result.push(if heading {
            Block::Heading(text)
        } else if quote {
            Block::Quote(text)
        } else if table {
            Block::Table(text)
        } else {
            Block::Text(text)
        });
    }
}
