//! Presentation of the host's saved tool transcripts, without changing their source text.
#[derive(Debug, PartialEq)]
pub enum ToolKind {
    Command,
    Files,
    Other,
}
#[derive(Debug)]
pub struct ToolOutput<'a> {
    pub kind: ToolKind,
    pub title: &'a str,
    pub body: &'a str,
    pub status: Option<&'a str>,
    pub exit: Option<i64>,
}
/// Separates the host's command, file-change and generic activity records for display.
pub fn parse(text: &str, complete: bool) -> ToolOutput<'_> {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    if let Some(command) = first.strip_prefix("$ ") {
        let trimmed = rest.trim_end_matches('\n');
        let (body, last) = trimmed.rsplit_once('\n').unwrap_or(("", trimmed));
        let exit = complete
            .then_some(last)
            .and_then(|last| last.strip_prefix("Exit "))
            .and_then(|code| code.parse().ok());
        return ToolOutput {
            kind: ToolKind::Command,
            title: command,
            body: if exit.is_some() { body } else { rest },
            status: None,
            exit,
        };
    }
    if let Some(status) = first.strip_prefix("File changes · ") {
        return ToolOutput {
            kind: ToolKind::Files,
            title: "File changes",
            body: rest,
            status: Some(status),
            exit: None,
        };
    }
    let (title, status) = first
        .split_once(" · ")
        .map_or((first, None), |(title, status)| (title, Some(status)));
    ToolOutput {
        kind: ToolKind::Other,
        title: if title.is_empty() {
            "Tool activity"
        } else {
            title
        },
        body: rest,
        status,
        exit: None,
    }
}
/// Removes terminal escape sequences from display text; copying the original remains available.
pub fn display_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => match chars.next() {
                Some('[') => {
                    for ch in chars.by_ref() {
                        if ('@'..='~').contains(&ch) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => result.push('\n'),
            ch if !ch.is_control() || ch == '\n' || ch == '\t' => result.push(ch),
            _ => {}
        }
    }
    result
}
/// Returns a bounded UTF-8 preview and reports whether more captured output is available.
pub fn preview(text: &str) -> (String, bool) {
    let mut end = 0;
    let mut lines = 0;
    for (characters, (index, ch)) in text.char_indices().enumerate() {
        if characters == 1800 || lines == 12 {
            break;
        }
        end = index + ch.len_utf8();
        if ch == '\n' {
            lines += 1;
        }
    }
    (
        text[..end].trim_end_matches('\n').to_owned(),
        end < text.len(),
    )
}
