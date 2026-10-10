//! The engine's song lexer and parser behind the Language Server Protocol
//! (#482): semantic tokens from `song::lex`, diagnostics from `Song::parse`.
//! An editor colours and checks a song exactly as the web app does, because
//! the engine is the one that knows the notation (ADR-0001).
//!
//! Positions are UTF-16, LSP's default and what `song::lex` counts in.

use algo_dsp::song::Song;
use algo_dsp::song::lex::{Class, lex};
use lsp_types::{
    Diagnostic, DiagnosticSeverity, Position, Range, SemanticToken, SemanticTokenType,
    SemanticTokensLegend,
};

/// The token types the server reports, standard ones so any theme colours them.
pub const TYPES: [SemanticTokenType; 10] = [
    SemanticTokenType::KEYWORD,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::NUMBER,
    SemanticTokenType::STRING,
    SemanticTokenType::TYPE,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::COMMENT,
];

/// The legend the server announces: `TYPES`, no modifiers.
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TYPES.to_vec(),
        token_modifiers: Vec::new(),
    }
}

/// A class's index in `TYPES`. A rest is dimmed like a comment, as the web
/// editor dims it; a pitch is a string, a pad a type, a step an enum member.
pub fn type_of(class: Class) -> u32 {
    match class {
        Class::Keyword => 0,
        Class::Name => 1,
        Class::Number => 2,
        Class::Note => 3,
        Class::Pad => 4,
        Class::Step => 5,
        Class::Call => 6,
        Class::Param => 7,
        Class::Punct => 8,
        Class::Rest | Class::Comment => 9,
    }
}

/// A class's name in the web editor (`CLASSES` in `web/src/audio/lex.ts`).
pub fn name_of(class: Class) -> &'static str {
    match class {
        Class::Keyword => "kw",
        Class::Name => "name",
        Class::Number => "num",
        Class::Note => "note",
        Class::Pad => "pad",
        Class::Step => "step",
        Class::Rest => "rest",
        Class::Call => "call",
        Class::Param => "param",
        Class::Punct => "punct",
        Class::Comment => "comment",
    }
}

/// A span of the song as a line, a column and a length in UTF-16 units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub class: Class,
}

/// The engine's spans of `text`, by line. A span never crosses a line.
pub fn tokens(text: &str) -> Vec<Token> {
    // The UTF-16 offset each line starts at.
    let mut starts = vec![0u32];
    let mut at = 0u32;
    for c in text.chars() {
        at += c.len_utf16() as u32;
        if c == '\n' {
            starts.push(at);
        }
    }
    lex(text)
        .into_iter()
        .map(|s| {
            let line = starts.partition_point(|&l| l <= s.start).saturating_sub(1);
            let first = starts.get(line).copied().unwrap_or(0);
            Token {
                line: line as u32,
                col: s.start - first,
                len: s.len,
                class: s.class,
            }
        })
        .collect()
}

/// The semantic tokens of `text`, each relative to the one before.
pub fn semantic_tokens(text: &str) -> Vec<SemanticToken> {
    let mut prev = (0, 0);
    tokens(text)
        .into_iter()
        .map(|t| {
            let delta_line = t.line - prev.0;
            let delta_start = if delta_line == 0 {
                t.col - prev.1
            } else {
                t.col
            };
            prev = (t.line, t.col);
            SemanticToken {
                delta_line,
                delta_start,
                length: t.len,
                token_type: type_of(t.class),
                token_modifiers_bitset: 0,
            }
        })
        .collect()
}

/// Why `text` is not a song, if it is not: the parser's first error, over
/// the word it points at.
pub fn diagnostics(text: &str) -> Vec<Diagnostic> {
    let Err(e) = Song::parse(text) else {
        return Vec::new();
    };
    // The parser counts lines and chars from 1, over `str::lines`.
    let line = e.line.saturating_sub(1);
    let chars: Vec<char> = text.lines().nth(line).unwrap_or("").chars().collect();
    let start = e.col.saturating_sub(1).min(chars.len());
    let end = chars
        .iter()
        .skip(start)
        .position(|c| c.is_whitespace())
        .map_or(chars.len(), |n| start + n);
    let utf16 = |k: usize| chars.iter().take(k).map(|c| c.len_utf16() as u32).sum();
    let at = |k: usize| Position::new(line as u32, utf16(k));
    vec![Diagnostic {
        range: Range::new(at(start), at(end)),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("song".to_string()),
        message: e.msg.to_string(),
        ..Diagnostic::default()
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SONG: &str = "tempo 124 # fast
track kit drums
clip beat = kit /16
  bd x...X...
";

    #[test]
    fn tokens_are_the_engines_spans_by_line() {
        let t = tokens(SONG);
        let at = |line, col| t.iter().find(|t| t.line == line && t.col == col);
        assert_eq!(
            at(0, 0).map(|t| (t.len, t.class)),
            Some((5, Class::Keyword))
        );
        assert_eq!(
            at(0, 10).map(|t| (t.len, t.class)),
            Some((6, Class::Comment))
        );
        assert_eq!(at(3, 2).map(|t| (t.len, t.class)), Some((2, Class::Pad)));
        assert_eq!(t.len(), lex(SONG).len());
    }

    #[test]
    fn semantic_tokens_are_relative() {
        let s = semantic_tokens(SONG);
        // `tempo`, `124`, `# fast`, then `track` on the next line.
        let first: Vec<_> = s
            .iter()
            .take(4)
            .map(|t| (t.delta_line, t.delta_start))
            .collect();
        assert_eq!(first, [(0, 0), (0, 6), (0, 4), (1, 0)]);
        assert_eq!(s.first().map(|t| t.token_type), Some(0));
    }

    #[test]
    fn columns_count_utf16_and_survive_crlf() {
        let t = tokens("# 🎹\r\ntempo 120\r\n");
        let tempo = t.iter().find(|t| t.class == Class::Keyword);
        assert_eq!(tempo.map(|t| (t.line, t.col)), Some((1, 0)));
        let comment = t.iter().find(|t| t.class == Class::Comment);
        assert_eq!(comment.map(|t| t.len), Some(4));
    }

    #[test]
    fn every_type_is_in_the_legend() {
        for c in [
            Class::Keyword,
            Class::Name,
            Class::Number,
            Class::Note,
            Class::Pad,
            Class::Step,
            Class::Rest,
            Class::Call,
            Class::Param,
            Class::Punct,
            Class::Comment,
        ] {
            assert!((type_of(c) as usize) < legend().token_types.len());
        }
    }

    #[test]
    fn a_song_has_no_diagnostics() {
        assert!(diagnostics(SONG).is_empty());
    }

    #[test]
    fn an_error_covers_its_word() {
        let d = diagnostics("tempo 120\nbogus 1\n");
        let d = d.first().map(|d| (d.range, d.severity));
        let range = Range::new(Position::new(1, 0), Position::new(1, 5));
        assert_eq!(d, Some((range, Some(DiagnosticSeverity::ERROR))));
    }

    #[test]
    fn an_error_column_counts_utf16() {
        let d = diagnostics("tempo 🎹\n");
        let start = d.first().map(|d| d.range.start);
        assert_eq!(start, Some(Position::new(0, 6)));
    }

    /// The examples are songs: an editor opening one shows no error.
    #[test]
    fn the_examples_have_no_diagnostics() -> std::io::Result<()> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let mut seen = 0;
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "song") {
                let text = std::fs::read_to_string(&path)?;
                assert_eq!(diagnostics(&text), [], "{}", path.display());
                seen += 1;
            }
        }
        assert!(seen > 0);
        Ok(())
    }
}
