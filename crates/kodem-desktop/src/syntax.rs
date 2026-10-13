//! Syntax spans for captured code. Highlighting never changes the source bytes.
use kodem_core::ThemeId;
use std::{collections::HashMap, ops::Range, str::FromStr, sync::LazyLock};
use syntect::{
    easy::HighlightLines,
    highlighting::{
        Color, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSet, ThemeSettings,
    },
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
static SYNTAX: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);
static CLASSICS: LazyLock<HashMap<ThemeId, Theme>> = LazyLock::new(|| {
    ThemeId::ALL
        .into_iter()
        .filter_map(|id| Some((id, classic(code_colors(id)?))))
        .collect()
});

// The code colors of a classic theme: plain text, then one color for each kind of token.
struct CodeColors {
    text: u32,
    comment: u32,
    keyword: u32,
    /// Declaration keywords and built-in types, which share a scope in most grammars.
    storage: u32,
    string: u32,
    constant: u32,
    function: u32,
    kind: u32,
    parameter: u32,
}

impl CodeColors {
    // Takes the colors in the order of the fields. Declarations take the keyword color.
    fn new(
        [
            text,
            comment,
            keyword,
            string,
            constant,
            function,
            kind,
            parameter,
        ]: [u32; 8],
    ) -> Self {
        Self {
            text,
            comment,
            keyword,
            storage: keyword,
            string,
            constant,
            function,
            kind,
            parameter,
        }
    }
}

// Returns the code colors of a theme that brings its own. The other themes use a color theme
// that syntect bundles.
fn code_colors(id: ThemeId) -> Option<CodeColors> {
    let colors = CodeColors::new(match id {
        ThemeId::Monokai => [
            0xf8f8f2, 0x88846f, 0xf92672, 0xe6db74, 0xae81ff, 0xa6e22e, 0x66d9ef, 0xfd971f,
        ],
        ThemeId::Dracula => [
            0xf8f8f2, 0x6272a4, 0xff79c6, 0xf1fa8c, 0xbd93f9, 0x50fa7b, 0x8be9fd, 0xffb86c,
        ],
        ThemeId::OneDark => [
            0xabb2bf, 0x7f848e, 0xc678dd, 0x98c379, 0xd19a66, 0x61afef, 0xe5c07b, 0xe06c75,
        ],
        ThemeId::Nord => [
            0xd8dee9, 0x7b88a1, 0x81a1c1, 0xa3be8c, 0xb48ead, 0x88c0d0, 0x8fbcbb, 0xd8dee9,
        ],
        ThemeId::TokyoNight => [
            0xc0caf5, 0x737aa2, 0xbb9af7, 0x9ece6a, 0xff9e64, 0x7aa2f7, 0x2ac3de, 0xe0af68,
        ],
        ThemeId::GruvboxDark => [
            0xebdbb2, 0x928374, 0xfb4934, 0xb8bb26, 0xd3869b, 0x8ec07c, 0xfabd2f, 0x83a598,
        ],
        ThemeId::Mocha => [
            0xcdd6f4, 0x7f849c, 0xcba6f7, 0xa6e3a1, 0xfab387, 0x89b4fa, 0xf9e2af, 0xeba0ac,
        ],
        ThemeId::Latte => [
            0x4c4f69, 0x8c8fa1, 0x8839ef, 0x40a02b, 0xfe640b, 0x1e66f5, 0xdf8e1d, 0xe64553,
        ],
        ThemeId::GruvboxLight => [
            0x3c3836, 0x928374, 0x9d0006, 0x79740e, 0x8f3f71, 0x427b58, 0xb57614, 0x076678,
        ],
        ThemeId::OneLight => [
            0x383a42, 0xa0a1a7, 0xa626a4, 0x50a14f, 0x986801, 0x4078f2, 0xc18401, 0xe45649,
        ],
        ThemeId::Graphite | ThemeId::Paper | ThemeId::SolarizedDark | ThemeId::SolarizedLight => {
            return None;
        }
    });
    Some(match id {
        // Monokai sets declarations in the color of its types.
        ThemeId::Monokai => CodeColors {
            storage: colors.kind,
            ..colors
        },
        _ => colors,
    })
}

// Returns the name of the bundled color theme for a theme without code colors of its own.
fn bundled(id: ThemeId) -> &'static str {
    match id {
        ThemeId::SolarizedDark => "Solarized (dark)",
        ThemeId::SolarizedLight => "Solarized (light)",
        id if id.light() => "InspiredGitHub",
        _ => "base16-ocean.dark",
    }
}

// Builds a color theme from the code colors of a classic theme. The most specific scope wins,
// so a declaration takes its own color and not the keyword color.
fn classic(colors: CodeColors) -> Theme {
    let color = |hex: u32| Color {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
        a: 0xff,
    };
    let rules = [
        ("comment, punctuation.definition.comment", colors.comment),
        ("string, punctuation.definition.string", colors.string),
        (
            "constant.numeric, constant.language, constant.character, constant.other",
            colors.constant,
        ),
        (
            "keyword, storage, storage.modifier, entity.name.tag",
            colors.keyword,
        ),
        ("storage.type", colors.storage),
        ("keyword.operator, punctuation", colors.text),
        (
            "entity.name.type, entity.name.class, entity.other.inherited-class, support.type, support.class",
            colors.kind,
        ),
        (
            "entity.name.function, support.function, variable.function, entity.other.attribute-name",
            colors.function,
        ),
        ("variable.parameter", colors.parameter),
    ];
    Theme {
        settings: ThemeSettings {
            foreground: Some(color(colors.text)),
            ..ThemeSettings::default()
        },
        scopes: rules
            .into_iter()
            .filter_map(|(scopes, hex)| {
                Some(ThemeItem {
                    scope: ScopeSelectors::from_str(scopes).ok()?,
                    style: StyleModifier {
                        foreground: Some(color(hex)),
                        background: None,
                        font_style: None,
                    },
                })
            })
            .collect(),
        ..Theme::default()
    }
}

/// Highlights code using a known file extension or language token, in the code colors of the
/// workspace theme.
pub fn spans(text: &str, extension: &str, theme: ThemeId) -> Vec<(Range<usize>, [u8; 3])> {
    let syntax = SYNTAX
        .find_syntax_by_token(extension)
        .unwrap_or_else(|| SYNTAX.find_syntax_plain_text());
    let Some(theme) = CLASSICS
        .get(&theme)
        .or_else(|| THEMES.themes.get(bundled(theme)))
    else {
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
