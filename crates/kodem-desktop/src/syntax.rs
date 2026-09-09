//! Syntax spans for captured code. Highlighting never changes the source bytes.
use std::{ops::Range, sync::LazyLock};
use syntect::{
    easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet, util::LinesWithEndings,
};
static SYNTAX: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);
/// Highlights code using a known file extension or language token.
pub fn spans(text: &str, extension: &str, light: bool) -> Vec<(Range<usize>, [u8; 3])> {
    let syntax = SYNTAX
        .find_syntax_by_token(extension)
        .unwrap_or_else(|| SYNTAX.find_syntax_plain_text());
    let theme = if light {
        "InspiredGitHub"
    } else {
        "base16-ocean.dark"
    };
    let Some(theme) = THEMES.themes.get(theme) else {
        return Vec::new();
    };
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut offset = 0;
    let mut result = Vec::new();
    for line in LinesWithEndings::from(text) {
        if let Ok(spans) = highlighter.highlight_line(line, &SYNTAX) {
            for (style, span) in spans {
                result.push((
                    offset..offset + span.len(),
                    [style.foreground.r, style.foreground.g, style.foreground.b],
                ));
                offset += span.len();
            }
        } else {
            offset += line.len();
        }
    }
    result
}
