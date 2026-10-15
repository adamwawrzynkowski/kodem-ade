//! Syntax highlighting for diff lines. Each side of a diff keeps its own parser state, so a
//! comment or string that a removed line opens does not color the added lines. Known languages
//! parse with syntect's grammars; a few others use a small built-in tokenizer.
use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::LazyLock;

use gpui::{FontStyle, FontWeight, HighlightStyle, SharedString};
use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

use crate::theme::Theme;
use kodem_git::git::DiffLineKind;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// The role of a highlighted piece of code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntaxKind {
    Attribute,
    Boolean,
    Comment,
    Constant,
    Constructor,
    Emphasis,
    Function,
    Keyword,
    Number,
    Operator,
    Preprocessor,
    Property,
    Punctuation,
    Regex,
    String,
    Strong,
    Tag,
    Title,
    Type,
    Variable,
    VariableSpecial,
}

impl SyntaxKind {
    /// Returns the style of this kind of code in the tones of a Kodem theme, or `None` when it
    /// keeps the plain text color. Keywords take the accent, strings the addition green,
    /// literals the deletion tone, types the warning tone and calls the info tone.
    pub(crate) fn style(self, theme: &Theme) -> Option<HighlightStyle> {
        let color = |color| HighlightStyle {
            color: Some(color),
            ..HighlightStyle::default()
        };
        Some(match self {
            Self::Comment => color(theme.subtle),
            Self::Keyword | Self::Preprocessor | Self::Tag => color(theme.accent),
            Self::String | Self::Regex => color(theme.added),
            Self::Number | Self::Boolean | Self::Constant | Self::VariableSpecial => {
                color(theme.removed)
            }
            Self::Type | Self::Attribute => color(theme.warning),
            Self::Function | Self::Constructor | Self::Title => color(theme.info),
            Self::Operator | Self::Punctuation => color(theme.muted),
            Self::Emphasis => HighlightStyle {
                font_style: Some(FontStyle::Italic),
                ..HighlightStyle::default()
            },
            Self::Strong => HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..HighlightStyle::default()
            },
            Self::Property | Self::Variable => return None,
        })
    }
}

/// A highlighted byte range of one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyntaxSpan {
    /// The byte range in the line's text.
    pub(crate) range: Range<usize>,
    /// The role of the code in the range.
    pub(crate) kind: SyntaxKind,
}

#[derive(Clone, Copy)]
enum LanguageEngine {
    Syntect(&'static SyntaxReference),
    Generic(GenericLanguage),
}

impl LanguageEngine {
    fn display_name(self) -> &'static str {
        match self {
            Self::Syntect(syntax) => syntax.name.as_str(),
            Self::Generic(language) => language.display_name(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GenericLanguage {
    Swift,
    TypeScript,
    Tsx,
    Kotlin,
    CSharp,
    Go,
    Dart,
    Zig,
    Hcl,
    Vue,
    Svelte,
}

impl GenericLanguage {
    fn display_name(self) -> &'static str {
        match self {
            Self::Swift => "Swift",
            Self::TypeScript => "TypeScript",
            Self::Tsx => "TSX",
            Self::Kotlin => "Kotlin",
            Self::CSharp => "C#",
            Self::Go => "Go",
            Self::Dart => "Dart",
            Self::Zig => "Zig",
            Self::Hcl => "HCL",
            Self::Vue => "Vue",
            Self::Svelte => "Svelte",
        }
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Swift => &[
                "actor",
                "any",
                "as",
                "associatedtype",
                "async",
                "await",
                "borrowing",
                "break",
                "case",
                "catch",
                "class",
                "consume",
                "consuming",
                "continue",
                "convenience",
                "copy",
                "default",
                "defer",
                "deinit",
                "didSet",
                "distributed",
                "do",
                "dynamic",
                "else",
                "enum",
                "extension",
                "fallthrough",
                "fileprivate",
                "final",
                "for",
                "func",
                "get",
                "guard",
                "if",
                "import",
                "indirect",
                "in",
                "infix",
                "init",
                "inout",
                "internal",
                "isolated",
                "lazy",
                "let",
                "macro",
                "mutating",
                "nonisolated",
                "nonmutating",
                "open",
                "operator",
                "optional",
                "override",
                "package",
                "postfix",
                "precedencegroup",
                "prefix",
                "private",
                "protocol",
                "public",
                "repeat",
                "required",
                "rethrows",
                "return",
                "self",
                "set",
                "some",
                "static",
                "struct",
                "subscript",
                "super",
                "switch",
                "throws",
                "throw",
                "try",
                "typealias",
                "unowned",
                "var",
                "weak",
                "where",
                "while",
                "willSet",
            ],
            Self::TypeScript | Self::Tsx | Self::Vue | Self::Svelte => &[
                "abstract",
                "any",
                "as",
                "asserts",
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "constructor",
                "continue",
                "debugger",
                "declare",
                "default",
                "delete",
                "do",
                "else",
                "enum",
                "export",
                "extends",
                "finally",
                "for",
                "from",
                "function",
                "get",
                "if",
                "implements",
                "import",
                "in",
                "infer",
                "instanceof",
                "interface",
                "is",
                "keyof",
                "let",
                "module",
                "namespace",
                "never",
                "new",
                "of",
                "override",
                "private",
                "protected",
                "public",
                "readonly",
                "return",
                "satisfies",
                "set",
                "static",
                "super",
                "switch",
                "symbol",
                "this",
                "throw",
                "try",
                "type",
                "typeof",
                "undefined",
                "unique",
                "unknown",
                "using",
                "var",
                "void",
                "while",
                "with",
                "yield",
            ],
            Self::Kotlin => &[
                "abstract",
                "actual",
                "annotation",
                "as",
                "break",
                "by",
                "catch",
                "class",
                "companion",
                "const",
                "constructor",
                "continue",
                "crossinline",
                "data",
                "do",
                "dynamic",
                "else",
                "enum",
                "expect",
                "external",
                "field",
                "file",
                "final",
                "finally",
                "for",
                "fun",
                "get",
                "if",
                "import",
                "in",
                "infix",
                "init",
                "inline",
                "inner",
                "interface",
                "internal",
                "is",
                "lateinit",
                "noinline",
                "object",
                "open",
                "operator",
                "out",
                "override",
                "package",
                "private",
                "protected",
                "public",
                "reified",
                "return",
                "sealed",
                "set",
                "suspend",
                "tailrec",
                "this",
                "throw",
                "try",
                "typealias",
                "val",
                "var",
                "vararg",
                "when",
                "where",
                "while",
            ],
            Self::CSharp => &[
                "abstract",
                "as",
                "async",
                "await",
                "base",
                "break",
                "case",
                "catch",
                "checked",
                "class",
                "const",
                "continue",
                "default",
                "delegate",
                "do",
                "else",
                "enum",
                "event",
                "explicit",
                "extern",
                "finally",
                "fixed",
                "for",
                "foreach",
                "get",
                "global",
                "goto",
                "if",
                "implicit",
                "in",
                "init",
                "interface",
                "internal",
                "is",
                "lock",
                "namespace",
                "new",
                "operator",
                "out",
                "override",
                "params",
                "partial",
                "private",
                "protected",
                "public",
                "readonly",
                "record",
                "ref",
                "required",
                "return",
                "sealed",
                "set",
                "sizeof",
                "stackalloc",
                "static",
                "struct",
                "switch",
                "this",
                "throw",
                "try",
                "typeof",
                "unchecked",
                "unsafe",
                "using",
                "virtual",
                "void",
                "volatile",
                "while",
                "yield",
            ],
            Self::Go => &[
                "break",
                "case",
                "chan",
                "const",
                "continue",
                "default",
                "defer",
                "else",
                "fallthrough",
                "for",
                "func",
                "go",
                "goto",
                "if",
                "import",
                "interface",
                "map",
                "package",
                "range",
                "return",
                "select",
                "struct",
                "switch",
                "type",
                "var",
            ],
            Self::Dart => &[
                "abstract",
                "as",
                "assert",
                "async",
                "await",
                "base",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "covariant",
                "default",
                "deferred",
                "do",
                "dynamic",
                "else",
                "enum",
                "export",
                "extends",
                "extension",
                "external",
                "factory",
                "final",
                "finally",
                "for",
                "get",
                "hide",
                "if",
                "implements",
                "import",
                "in",
                "interface",
                "is",
                "late",
                "library",
                "mixin",
                "new",
                "on",
                "operator",
                "part",
                "required",
                "rethrow",
                "return",
                "sealed",
                "set",
                "show",
                "static",
                "super",
                "switch",
                "sync",
                "this",
                "throw",
                "try",
                "typedef",
                "var",
                "void",
                "when",
                "while",
                "with",
                "yield",
            ],
            Self::Zig => &[
                "addrspace",
                "align",
                "allowzero",
                "and",
                "anyframe",
                "anytype",
                "asm",
                "async",
                "await",
                "break",
                "callconv",
                "catch",
                "comptime",
                "const",
                "continue",
                "defer",
                "else",
                "enum",
                "errdefer",
                "error",
                "export",
                "extern",
                "fn",
                "for",
                "if",
                "inline",
                "linksection",
                "noalias",
                "nosuspend",
                "opaque",
                "or",
                "orelse",
                "packed",
                "pub",
                "resume",
                "return",
                "struct",
                "suspend",
                "switch",
                "test",
                "threadlocal",
                "try",
                "union",
                "unreachable",
                "usingnamespace",
                "var",
                "volatile",
                "while",
            ],
            Self::Hcl => &[
                "data",
                "dynamic",
                "for",
                "if",
                "in",
                "locals",
                "module",
                "output",
                "provider",
                "resource",
                "terraform",
                "variable",
            ],
        }
    }

    fn has_hash_comments(self) -> bool {
        matches!(self, Self::Hcl)
    }
}

/// Remembers the highlight kind of each scope stack. Scope names are
/// compared as strings, so this avoids building the strings again for
/// every token.
type ScopeKindCache = HashMap<Vec<Scope>, Option<SyntaxKind>>;

#[derive(Clone, PartialEq, Eq)]
struct SyntectLineParser {
    parse_state: ParseState,
    scope_stack: ScopeStack,
}

impl SyntectLineParser {
    fn new(syntax: &SyntaxReference) -> Self {
        Self {
            parse_state: ParseState::new(syntax),
            scope_stack: ScopeStack::new(),
        }
    }

    fn highlight_line(&mut self, text: &str, kinds: &mut ScopeKindCache) -> Vec<SyntaxSpan> {
        let mut source = String::with_capacity(text.len() + 1);
        source.push_str(text);
        if !source.ends_with('\n') {
            source.push('\n');
        }

        let Ok(operations) = self.parse_state.parse_line(&source, &SYNTAX_SET) else {
            return Vec::new();
        };
        let mut offset = 0;
        let mut spans = Vec::new();
        for (region, operation) in ScopeRegionIterator::new(&operations, &source) {
            if self.scope_stack.apply(operation).is_err() {
                return Vec::new();
            }
            let start = offset.min(text.len());
            offset += region.len();
            let end = offset.min(text.len());
            if start == end {
                continue;
            }
            let scopes = &self.scope_stack.scopes;
            let kind = match kinds.get(scopes) {
                Some(kind) => *kind,
                None => {
                    let kind = syntax_kind(scopes);
                    kinds.insert(scopes.clone(), kind);
                    kind
                }
            };
            if let Some(kind) = kind {
                push_span(&mut spans, start..end, kind);
            }
        }
        spans
    }
}

#[derive(Clone, PartialEq, Eq)]
struct GenericLineParser {
    language: GenericLanguage,
    block_comment_depth: usize,
    multiline_string: bool,
}

impl GenericLineParser {
    fn new(language: GenericLanguage) -> Self {
        Self {
            language,
            block_comment_depth: 0,
            multiline_string: false,
        }
    }

    fn highlight_line(&mut self, text: &str) -> Vec<SyntaxSpan> {
        let mut spans = Vec::new();
        let mut index = 0;
        while index < text.len() {
            if self.multiline_string {
                let start = index;
                if let Some(relative_end) = text[index..].find("\"\"\"") {
                    index += relative_end + 3;
                    self.multiline_string = false;
                } else {
                    index = text.len();
                }
                push_span(&mut spans, start..index, SyntaxKind::String);
                continue;
            }

            if self.block_comment_depth > 0 {
                let start = index;
                while index < text.len() && self.block_comment_depth > 0 {
                    if text[index..].starts_with("/*") {
                        self.block_comment_depth += 1;
                        index += 2;
                    } else if text[index..].starts_with("*/") {
                        self.block_comment_depth -= 1;
                        index += 2;
                    } else {
                        index = next_char_boundary(text, index);
                    }
                }
                push_span(&mut spans, start..index, SyntaxKind::Comment);
                continue;
            }

            if text[index..].starts_with("//")
                || (self.language.has_hash_comments() && text[index..].starts_with('#'))
            {
                push_span(&mut spans, index..text.len(), SyntaxKind::Comment);
                break;
            }
            if text[index..].starts_with("/*") {
                let start = index;
                self.block_comment_depth = 1;
                index += 2;
                while index < text.len() && self.block_comment_depth > 0 {
                    if text[index..].starts_with("/*") {
                        self.block_comment_depth += 1;
                        index += 2;
                    } else if text[index..].starts_with("*/") {
                        self.block_comment_depth -= 1;
                        index += 2;
                    } else {
                        index = next_char_boundary(text, index);
                    }
                }
                push_span(&mut spans, start..index, SyntaxKind::Comment);
                continue;
            }
            if text[index..].starts_with("\"\"\"") {
                let start = index;
                index += 3;
                if let Some(relative_end) = text[index..].find("\"\"\"") {
                    index += relative_end + 3;
                } else {
                    index = text.len();
                    self.multiline_string = true;
                }
                push_span(&mut spans, start..index, SyntaxKind::String);
                continue;
            }

            let character = text[index..]
                .chars()
                .next()
                .expect("valid character boundary");
            if matches!(character, '\"' | '\'' | '`') {
                let start = index;
                index = scan_quoted_string(text, index, character);
                push_span(&mut spans, start..index, SyntaxKind::String);
                continue;
            }
            if character == '@' {
                let end = scan_identifier(text, index + character.len_utf8());
                if end > index + character.len_utf8() {
                    push_span(&mut spans, index..end, SyntaxKind::Attribute);
                    index = end;
                    continue;
                }
            }
            if character == '#' {
                let end = scan_identifier(text, index + character.len_utf8());
                if end > index + character.len_utf8() {
                    push_span(&mut spans, index..end, SyntaxKind::Preprocessor);
                    index = end;
                    continue;
                }
            }
            if character.is_ascii_digit() {
                let start = index;
                index = scan_number(text, index);
                push_span(&mut spans, start..index, SyntaxKind::Number);
                continue;
            }
            if is_identifier_start(character) {
                let start = index;
                index = scan_identifier(text, index);
                let token = &text[start..index];
                let kind = if self.language.keywords().contains(&token) {
                    Some(SyntaxKind::Keyword)
                } else if matches!(token, "true" | "false") {
                    Some(SyntaxKind::Boolean)
                } else if matches!(token, "nil" | "null" | "None") {
                    Some(SyntaxKind::Constant)
                } else if previous_non_whitespace(text, start) == Some('.') {
                    Some(SyntaxKind::Property)
                } else if next_non_whitespace(text, index) == Some('(') {
                    Some(SyntaxKind::Function)
                } else if token.chars().next().is_some_and(char::is_uppercase) {
                    Some(SyntaxKind::Type)
                } else {
                    None
                };
                if let Some(kind) = kind {
                    push_span(&mut spans, start..index, kind);
                }
                continue;
            }
            if is_operator(character) {
                let start = index;
                index = scan_while(text, index, is_operator);
                push_span(&mut spans, start..index, SyntaxKind::Operator);
                continue;
            }
            if is_punctuation(character) {
                let end = index + character.len_utf8();
                push_span(&mut spans, index..end, SyntaxKind::Punctuation);
                index = end;
                continue;
            }
            index += character.len_utf8();
        }
        spans
    }
}

#[derive(Clone, PartialEq, Eq)]
enum LineParser {
    Syntect(SyntectLineParser),
    Generic(GenericLineParser),
}

impl LineParser {
    fn new(engine: LanguageEngine) -> Self {
        match engine {
            LanguageEngine::Syntect(syntax) => Self::Syntect(SyntectLineParser::new(syntax)),
            LanguageEngine::Generic(language) => Self::Generic(GenericLineParser::new(language)),
        }
    }

    fn highlight_line(&mut self, text: &str, kinds: &mut ScopeKindCache) -> Vec<SyntaxSpan> {
        match self {
            Self::Syntect(parser) => parser.highlight_line(text, kinds),
            Self::Generic(parser) => parser.highlight_line(text),
        }
    }
}

/// Highlights the lines of one file diff, keeping the parser state of the old and new sides
/// apart.
pub(crate) struct DiffSyntaxHighlighter {
    engine: LanguageEngine,
    language: SharedString,
    old: LineParser,
    new: LineParser,
    kinds: ScopeKindCache,
}

impl DiffSyntaxHighlighter {
    /// Detects the language from the file path or, for scripts, the first line. Returns `None`
    /// for a language without highlighting.
    pub(crate) fn detect(path: &str, first_line: Option<&str>) -> Option<Self> {
        let engine = detect_language(path, first_line)?;
        Some(Self {
            engine,
            language: engine.display_name().into(),
            old: LineParser::new(engine),
            new: LineParser::new(engine),
            kinds: ScopeKindCache::new(),
        })
    }

    /// Returns the display name of the detected language.
    pub(crate) fn language(&self) -> SharedString {
        self.language.clone()
    }

    /// Resets both sides at the start of a hunk, since the lines between hunks are unknown.
    pub(crate) fn begin_hunk(&mut self) {
        self.old = LineParser::new(self.engine);
        self.new = LineParser::new(self.engine);
    }

    /// Highlights one diff line with the parser state of its side. Context lines advance both
    /// sides.
    pub(crate) fn highlight_line(&mut self, kind: DiffLineKind, text: &str) -> Vec<SyntaxSpan> {
        match kind {
            DiffLineKind::Added => self.new.highlight_line(text, &mut self.kinds),
            DiffLineKind::Removed => self.old.highlight_line(text, &mut self.kinds),
            DiffLineKind::Context => {
                // Both sides parse the same text. When their states match,
                // one parse gives the result and the state for both sides.
                if self.old == self.new {
                    let spans = self.new.highlight_line(text, &mut self.kinds);
                    self.old.clone_from(&self.new);
                    return spans;
                }
                let old = self.old.highlight_line(text, &mut self.kinds);
                let new = self.new.highlight_line(text, &mut self.kinds);
                if new.is_empty() { old } else { new }
            }
        }
    }
}

fn detect_language(path: &str, first_line: Option<&str>) -> Option<LanguageEngine> {
    detect_syntect(path, first_line)
        .map(LanguageEngine::Syntect)
        .or_else(|| detect_generic(path).map(LanguageEngine::Generic))
}

fn detect_syntect(path: &str, first_line: Option<&str>) -> Option<&'static SyntaxReference> {
    let path = Path::new(path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    SYNTAX_SET
        .find_syntax_by_extension(file_name)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(extension))
        .or_else(|| first_line.and_then(|line| SYNTAX_SET.find_syntax_by_first_line(line)))
        .filter(|syntax| syntax.name != "Plain Text")
}

fn detect_generic(path: &str) -> Option<GenericLanguage> {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())?
        .to_ascii_lowercase();
    match extension.as_str() {
        "swift" => Some(GenericLanguage::Swift),
        "ts" | "mts" | "cts" => Some(GenericLanguage::TypeScript),
        "tsx" | "jsx" => Some(GenericLanguage::Tsx),
        "kt" | "kts" => Some(GenericLanguage::Kotlin),
        "cs" => Some(GenericLanguage::CSharp),
        "go" => Some(GenericLanguage::Go),
        "dart" => Some(GenericLanguage::Dart),
        "zig" => Some(GenericLanguage::Zig),
        "hcl" | "tf" | "tfvars" => Some(GenericLanguage::Hcl),
        "vue" => Some(GenericLanguage::Vue),
        "svelte" => Some(GenericLanguage::Svelte),
        _ => None,
    }
}

fn push_span(spans: &mut Vec<SyntaxSpan>, range: Range<usize>, kind: SyntaxKind) {
    if range.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.kind == kind
        && last.range.end == range.start
    {
        last.range.end = range.end;
    } else {
        spans.push(SyntaxSpan { range, kind });
    }
}

fn next_char_boundary(text: &str, index: usize) -> usize {
    index
        + text[index..]
            .chars()
            .next()
            .expect("index is inside text")
            .len_utf8()
}

fn scan_quoted_string(text: &str, start: usize, delimiter: char) -> usize {
    let mut escaped = false;
    let mut index = start + delimiter.len_utf8();
    while index < text.len() {
        let character = text[index..]
            .chars()
            .next()
            .expect("valid character boundary");
        index += character.len_utf8();
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == delimiter {
            break;
        }
    }
    index
}

fn scan_number(text: &str, start: usize) -> usize {
    scan_while(text, start, |character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '_')
    })
}

fn scan_identifier(text: &str, start: usize) -> usize {
    scan_while(text, start, is_identifier_continue)
}

fn scan_while(text: &str, start: usize, predicate: impl Fn(char) -> bool) -> usize {
    let mut end = start;
    for character in text[start..].chars() {
        if !predicate(character) {
            break;
        }
        end += character.len_utf8();
    }
    end
}

fn previous_non_whitespace(text: &str, index: usize) -> Option<char> {
    text[..index]
        .chars()
        .rev()
        .find(|character| !character.is_whitespace())
}

fn next_non_whitespace(text: &str, index: usize) -> Option<char> {
    text[index..]
        .chars()
        .find(|character| !character.is_whitespace())
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character == '$' || character.is_alphabetic()
}

fn is_identifier_continue(character: char) -> bool {
    is_identifier_start(character) || character.is_numeric()
}

fn is_operator(character: char) -> bool {
    matches!(
        character,
        '+' | '-' | '*' | '/' | '%' | '=' | '!' | '<' | '>' | '?' | '&' | '|' | '^' | '~'
    )
}

fn is_punctuation(character: char) -> bool {
    matches!(
        character,
        '(' | ')' | '{' | '}' | '[' | ']' | ',' | ':' | ';' | '.'
    )
}

fn syntax_kind(scopes: &[Scope]) -> Option<SyntaxKind> {
    let scopes = scopes
        .iter()
        .map(|scope| scope.build_string())
        .collect::<Vec<_>>();
    let has = |prefix: &str| scopes.iter().any(|scope| scope.starts_with(prefix));
    let contains = |part: &str| scopes.iter().any(|scope| scope.contains(part));

    if has("comment") {
        Some(SyntaxKind::Comment)
    } else if has("string.regexp") || has("string.regex") {
        Some(SyntaxKind::Regex)
    } else if has("string") {
        Some(SyntaxKind::String)
    } else if has("constant.numeric") {
        Some(SyntaxKind::Number)
    } else if has("constant.language.boolean") || contains(".boolean") {
        Some(SyntaxKind::Boolean)
    } else if has("constant") || has("support.constant") {
        Some(SyntaxKind::Constant)
    } else if has("meta.preprocessor") || has("keyword.control.import") {
        Some(SyntaxKind::Preprocessor)
    } else if has("keyword.operator") || has("punctuation.separator.key-value") {
        Some(SyntaxKind::Operator)
    } else if has("keyword") || has("storage") {
        Some(SyntaxKind::Keyword)
    } else if has("entity.name.function") || has("support.function") {
        Some(SyntaxKind::Function)
    } else if has("entity.name.constructor") {
        Some(SyntaxKind::Constructor)
    } else if has("entity.name.type")
        || has("entity.name.class")
        || has("entity.name.struct")
        || has("support.type")
        || has("support.class")
    {
        Some(SyntaxKind::Type)
    } else if has("entity.name.tag") {
        Some(SyntaxKind::Tag)
    } else if has("entity.other.attribute-name") {
        Some(SyntaxKind::Attribute)
    } else if has("variable.language") || has("variable.other.constant") {
        Some(SyntaxKind::VariableSpecial)
    } else if has("variable") {
        Some(SyntaxKind::Variable)
    } else if has("entity.name") || has("support.variable.property") {
        Some(SyntaxKind::Property)
    } else if has("markup.heading") {
        Some(SyntaxKind::Title)
    } else if has("markup.bold") {
        Some(SyntaxKind::Strong)
    } else if has("markup.italic") {
        Some(SyntaxKind::Emphasis)
    } else if has("punctuation") {
        Some(SyntaxKind::Punctuation)
    } else {
        None
    }
}
