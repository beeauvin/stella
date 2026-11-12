// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the tokens lint. They're a crate of their own, so the compiler lets them reach
//! only what the crates make public: every source goes through the text lint's `lint`, then the
//! lexer's `lex`, and then `lint_tokens`.
//!
//! The rules are tried on tables of sources, each with the problem it should get, or none. Where
//! a rule is about a small set, the set is tried whole: every pair of tokens that can share a
//! line, with every gap up to two spaces, and every kind of line at every depth up to three,
//! indented every way up to four levels. A seeded fuzz test checks random programs, laid out the
//! one way and then changed, against an oracle. The oracle states the layout page as plainly as it
//! can, from each line's text rather than the tokens' positions: it writes each line the one way
//! and finds the first character that differs. Every example checks the oracle as well.

use std::cell::RefCell;
use std::cmp::Ordering;

use stella_lexer::TokenKind::{
    self, BraceClose, BraceOpen, Function, Name, ParenthesisClose, ParenthesisOpen, Returns,
    TypeName,
};
use stella_lexer::{TokenBuffer, lex};
use stella_linter_text::{LEVEL_SPACES, lint};
use stella_linter_tokens::Kind::{
    BraceCloseNotAlone, BraceClosesNothing, BraceNeverClosed, BraceOpenNotLast,
    BraceOpenStartsLine, IndentationNotDepth, LineEndsWithoutBrace, SpaceMissing, SpaceUnwanted,
    SpacesExtra,
};
use stella_linter_tokens::{Kind, Neighbors, Problem, lint_tokens};
use stella_source::{SOURCE_BYTES_MAX, u32_from_usize, usize_from_u32};
use stella_test_support::{
    Example, Examples, Rng, excerpt, indent, joined, nested_blocks, slice, source_of,
};

/// An expected refusal: the problem's kind, and its line and column.
type Refusal = (Kind, u32, u32);

/// What the tokens lint should make of a source: nothing, when it's laid out the one way, or its
/// first problem.
type Expected = Result<(), Refusal>;

/// This is the smallest program, laid out the one way.
const SMALLEST: &str = "function main() returns Void {\n}\n";

thread_local! {
    /// Each test lexes every source it tries into this one buffer, as the command does.
    static TOKEN_BUFFER: RefCell<TokenBuffer> = RefCell::new(TokenBuffer::new());
}

/// The tokens that can come first of two on a line, once the braces are where they go, each
/// written as code writes it.
const FIRSTS: [(TokenKind, &str); 6] = [
    (Function, "function"),
    (Returns, "returns"),
    (Name, "main"),
    (TypeName, "Void"),
    (ParenthesisOpen, "("),
    (ParenthesisClose, ")"),
];

/// The tokens that can come second: the same ones, and `{`.
const SECONDS: [(TokenKind, &str); 7] = [
    (Function, "function"),
    (Returns, "returns"),
    (Name, "main"),
    (TypeName, "Void"),
    (ParenthesisOpen, "("),
    (ParenthesisClose, ")"),
    (BraceOpen, "{"),
];

/// How many spaces go between each first token, by row, and each second, by column, as the
/// layout page says: one, except none after `(`, and none before `(` or `)`.
#[rustfmt::skip]
const SPACES: [[usize; 7]; 6] = [
    // function, returns, main, Void, (, ), {
    [1, 1, 1, 1, 0, 0, 1], // function
    [1, 1, 1, 1, 0, 0, 1], // returns
    [1, 1, 1, 1, 0, 0, 1], // main
    [1, 1, 1, 1, 0, 0, 1], // Void
    [0, 0, 0, 0, 0, 0, 0], // (
    [1, 1, 1, 1, 0, 0, 1], // )
];

/// These are what a random line of code is made of, before the `{` it ends with.
const PIECES: &[&str] = &["function", "returns", "main", "Void", "x", "MyType", "(", ")", "(", ")"];

/// These follow a comment's `//`.
const COMMENT_TEXTS: &[&str] = &["", " a", " café", " { } ( )", " function main() returns Void {"];

/// A line's kind says what the line holds, as the oracle sees it.
enum LineKind<'source> {
    /// Nothing: the line is blank.
    Blank,
    /// A comment, from its `//` to the end of the line.
    Comment(&'source str),
    /// Code, as its tokens: each one's first byte in the line, and its text.
    Code(Vec<(usize, &'source str)>),
}

/// A line of the source, as the oracle sees it.
struct Line<'source> {
    /// The number is the line's, from 1.
    number: u32,
    /// The content is the line, without its `\n`.
    content: &'source str,
    /// The indentation is how many spaces the line starts with.
    indentation: usize,
    /// The kind says what's on the line.
    kind: LineKind<'source>,
}

/// Returns the two kinds as neighbors, `before` first.
// TS-DISTINCT-ARGS: the two kinds are in the order the source has them, and an expectation with
// them swapped fails its test.
const fn between(before: TokenKind, after: TokenKind) -> Neighbors {
    Neighbors { before, after }
}

/// Lints and lexes `source`, which must pass both, and then lints its tokens. Returns nothing when
/// the tokens lint passes them, or its problem. A layout that passes must hold the very tokens it
/// was given.
fn checked(source: &str) -> Expected {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    TOKEN_BUFFER.with_borrow_mut(|token_buffer| {
        let lexed = lex(text, token_buffer);
        assert!(lexed.is_ok(), "{source:?} should lex: {:?}", lexed.as_ref().err());
        let tokens = lexed.expect("it lexed, as just asserted");
        match lint_tokens(tokens) {
            Ok(layout) => {
                let (given, back) = (tokens.as_slice(), layout.tokens().as_slice());
                assert!(std::ptr::eq(given, back), "the layout holds the tokens it was given");
                Ok(())
            }
            Err(Problem { kind, position }) => Err((kind, position.line(), position.column())),
        }
    })
}

/// Says whether `source` passes the text lint and the lexer, so it has tokens to lint.
fn has_tokens(source: &str) -> bool {
    let Ok(text) = lint(source.as_bytes()) else {
        return false;
    };
    TOKEN_BUFFER.with_borrow_mut(|token_buffer| lex(text, token_buffer).is_ok())
}

/// Checks that every example passes the text lint and the lexer, as the oracle assumes, and
/// agrees with the oracle. Then checks every one against the tokens lint. So every example checks
/// the oracle before the tokens lint sees any, even while the tokens lint can't run.
fn check(examples: Examples<Expected>) {
    let examples = examples.take();
    for Example { source, expected, at } in &examples {
        let excerpt = excerpt(source);
        assert!(has_tokens(source), "{at}: {excerpt} should pass the text lint and the lexer");
        assert_eq!(oracle(source), *expected, "{at}: the oracle, for {excerpt}");
    }
    for Example { source, expected, at } in &examples {
        assert_eq!(checked(source), *expected, "{at}: for {}", excerpt(source));
    }
}

/// Returns the column of a line's first token when it's indented `levels` levels.
const fn column_at(levels: u32) -> u32 {
    levels * LEVEL_SPACES + 1
}

/// Says whether a token of `kind` is a word, which runs into a word right after it.
const fn is_word(kind: TokenKind) -> bool {
    matches!(kind, Function | Returns | Name | TypeName)
}

/// Returns the lines of blocks opened `depth` deep, each line laid out the one way, then `line`,
/// then a `}` for every block still open, none of them indented. Nothing past the first problem
/// is looked at, so a source that's refused at `line` needn't indent them.
fn opened_then(depth: u32, line: &str) -> String {
    let mut lines: Vec<String> = (0..depth).map(|level| format!("{}({{", indent(level))).collect();
    lines.push(String::from(line));
    let still_open = depth + u32::from(line.ends_with('{'));
    lines.extend(std::iter::repeat_n(String::from("}"), usize_from_u32(still_open)));
    source_of(&lines)
}

/// Returns the column of the byte at `index` on a line, where everything before it is ASCII.
fn column(index: usize) -> u32 {
    u32_from_usize(index) + 1
}

/// Returns `source`'s lines, as the oracle sees them.
fn lines_of(source: &str) -> Vec<Line<'_>> {
    source
        .split_terminator('\n')
        .zip(1_u32..)
        .map(|(content, number)| line_of(content, number))
        .collect()
}

/// Returns the line `number`, which is `content`, as the oracle sees it.
fn line_of(content: &str, number: u32) -> Line<'_> {
    let code = content.trim_start_matches(' ');
    let indentation = content.len() - code.len();
    let kind = if code.is_empty() {
        LineKind::Blank
    } else if code.starts_with("//") {
        LineKind::Comment(code)
    } else {
        LineKind::Code(split(content, indentation))
    };
    Line { number, content, indentation, kind }
}

/// Splits a line of code into its tokens, starting at byte `from`: each of `(`, `)`, `{` and `}`
/// is one, a run of letters, digits and `_` is one, and spaces are between them.
fn split(content: &str, from: usize) -> Vec<(usize, &str)> {
    let bytes = content.as_bytes();
    let mut tokens = Vec::new();
    let mut index = from;
    while index < bytes.len() {
        let in_word = |byte: &&u8| byte.is_ascii_alphanumeric() || **byte == b'_';
        let length = match bytes[index] {
            b' ' => {
                index += 1;
                continue;
            }
            b'(' | b')' | b'{' | b'}' => 1,
            _ => bytes[index..].iter().take_while(in_word).count(),
        };
        assert!(length > 0, "the lexer passed this line");
        tokens.push((index, slice(content, index..index + length)));
        index += length;
    }
    tokens
}

/// Returns the kind of the token written `text`.
fn kind_of(text: &str) -> TokenKind {
    match text {
        "function" => Function,
        "returns" => Returns,
        "(" => ParenthesisOpen,
        ")" => ParenthesisClose,
        "{" => BraceOpen,
        "}" => BraceClose,
        _ if text.starts_with(|character: char| character.is_ascii_uppercase()) => TypeName,
        _ => Name,
    }
}

/// Returns what the tokens lint should make of `source`, which passed the text lint and the
/// lexer, worked out the way the layout page words it: nothing, or its first problem.
fn oracle(source: &str) -> Expected {
    let lines = lines_of(source);
    oracle_lines(&lines)?;
    oracle_braces(&lines)?;
    oracle_within_lines(&lines)
}

/// The first step, from each line's text: a `{` is at the end of its line and not at its start, a
/// `}` is all of its line, and a line of code ends in `{` or `}`. A brace out of place is found
/// before a line that ends some other way, since it's before the line's end. Of two problems at
/// the same `{`, the first listed wins.
fn oracle_lines(lines: &[Line<'_>]) -> Result<(), Refusal> {
    for line in lines {
        let LineKind::Code(_) = line.kind else {
            continue;
        };
        let code = line.content.trim_start_matches(' ');
        let open = code.find('{').filter(|&index| index + 1 < code.len());
        let close = code.find('}').filter(|_| code != "}");
        let out_of_place = [
            open.map(|index| (BraceOpenNotLast, index)),
            close.map(|index| (BraceCloseNotAlone, index)),
            code.starts_with('{').then_some((BraceOpenStartsLine, 0)),
        ];
        let first = out_of_place.into_iter().flatten().min_by_key(|&(_, index)| index);
        if let Some((kind, index)) = first {
            return Err((kind, line.number, column(line.indentation + index)));
        }
        if !code.ends_with(['{', '}']) {
            return Err((LineEndsWithoutBrace, line.number, column(line.content.len())));
        }
    }
    Ok(())
}

/// The second step: each `}` closes the `{` most recently opened, and none is left open. The
/// braces still open are kept as a stack of where each one is.
fn oracle_braces(lines: &[Line<'_>]) -> Result<(), Refusal> {
    let mut open: Vec<(u32, u32)> = Vec::new();
    for line in lines {
        let LineKind::Code(tokens) = &line.kind else {
            continue;
        };
        for &(index, text) in tokens {
            if text == "{" {
                open.push((line.number, column(index)));
            }
            if text == "}" {
                let closed = open.pop();
                if closed.is_none() {
                    return Err((BraceClosesNothing, line.number, column(index)));
                }
            }
        }
    }
    match open.first() {
        Some(&(line, column)) => Err((BraceNeverClosed, line, column)),
        None => Ok(()),
    }
}

/// The last step: each line is written the one way, indented one level for each `{` still open
/// on it, with its tokens joined the one way. The first character that differs is the problem.
fn oracle_within_lines(lines: &[Line<'_>]) -> Result<(), Refusal> {
    let mut depth: u32 = 0;
    for line in lines {
        let (level, written) = match &line.kind {
            LineKind::Blank => continue,
            LineKind::Comment(comment) => (depth, String::from(*comment)),
            LineKind::Code(tokens) => {
                let texts: Vec<&str> = tokens.iter().map(|&(_, text)| text).collect();
                // A line that starts with `}` counts its own brace as closed.
                let level = if texts[0] == "}" { depth - 1 } else { depth };
                depth = level + u32::from(texts.last() == Some(&"{"));
                (level, joined(&texts))
            }
        };
        oracle_line(line, level, &format!("{}{written}", indent(level)))?;
    }
    Ok(())
}

/// Returns the problem with `line`, which the one layout writes as `wanted`, indented `level`
/// levels: none when they're the same, or the first character that differs.
fn oracle_line(line: &Line<'_>, level: u32, wanted: &str) -> Result<(), Refusal> {
    let (bytes, wanted_bytes) = (line.content.as_bytes(), wanted.as_bytes());
    let shorter = bytes.len().min(wanted_bytes.len());
    let Some(index) = (0..shorter).find(|&index| bytes[index] != wanted_bytes[index]) else {
        // The tokens are the same, so either a character differs or nothing does.
        assert_eq!(line.content, wanted, "a line differs from the one layout by a character");
        return Ok(());
    };
    if index < line.indentation.max(indent(level).len()) {
        return Err((IndentationNotDepth(level), line.number, column(index)));
    }
    // Only a line of code can differ past its indentation, and only in its spaces, so the
    // character that differs is in a gap between two tokens.
    let LineKind::Code(tokens) = &line.kind else {
        unreachable!("a comment is the same text, however it's indented");
    };
    let second = tokens.iter().position(|&(start, _)| start >= index).expect("a token follows");
    assert!(second > 0, "the first token is placed by the indentation");
    let pair = (kind_of(tokens[second - 1].1), kind_of(tokens[second].1));
    let kind = match (bytes[index], wanted_bytes[index]) {
        (b' ', _) if bytes[index - 1] == b' ' => SpacesExtra(between(pair.0, pair.1)),
        (b' ', _) => SpaceUnwanted(between(pair.0, pair.1)),
        (_, b' ') => SpaceMissing(between(pair.0, pair.1)),
        _ => unreachable!("the same tokens differ only in their spaces"),
    };
    Err((kind, line.number, column(index)))
}

/// Returns a random program laid out the one way, as its lines: blank lines, comments, and blocks
/// nested up to five deep, each opened by a few random tokens and `{`, and closed by `}`.
fn random_program(rng: &mut Rng) -> Vec<String> {
    let mut lines = Vec::new();
    let mut depth: u32 = 0;
    for _ in 0..=rng.below(12) {
        match rng.below(6) {
            0 => lines.push(String::new()),
            1 => lines.push(format!("{}//{}", indent(depth), rng.pick(COMMENT_TEXTS))),
            2 | 3 if depth > 0 => {
                depth -= 1;
                lines.push(format!("{}}}", indent(depth)));
            }
            _ if depth < 5 => {
                let heads = 1 + rng.below(4);
                let mut texts: Vec<&str> =
                    std::iter::repeat_with(|| rng.pick(PIECES)).take(heads).collect();
                texts.push("{");
                lines.push(format!("{}{}", indent(depth), joined(&texts)));
                depth += 1;
            }
            _ => lines.push(format!("{}// deep", indent(depth))),
        }
    }
    lines.extend((0..depth).rev().map(|level| format!("{}}}", indent(level))));
    // A source can't end with a blank line, or be empty.
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push(String::from("// empty"));
    }
    lines
}

/// Changes `lines` in one random way that can break the layout: a space added beside a symbol,
/// taken away or doubled; a line indented a level more or less; two lines joined or one split; a
/// brace taken away; the tokens before a line's `{` taken away; a line or a token added; or, now
/// and then, a gap or an indentation widened by up to 300 spaces, or up to 300 blank lines
/// added, so widths and line numbers go past what a byte counts. Spaces are only moved on lines
/// of code, past their indentation, where the text lint lets them be.
fn mutate(rng: &mut Rng, lines: &mut Vec<String>) {
    let index = rng.below(lines.len());
    let line = lines[index].clone();
    let code = line.trim_start_matches(' ');
    let indentation = line.len() - code.len();
    let in_code = |at: &usize| *at > indentation && !code.starts_with("//");
    let spaces: Vec<usize> = line.match_indices(' ').map(|(at, _)| at).filter(in_code).collect();
    let symbols: Vec<usize> = line.match_indices(['(', ')', '{', '}']).map(|(at, _)| at).collect();
    match rng.below(19) {
        0..=4 if !symbols.is_empty() && !code.starts_with("//") => {
            let at = symbols[rng.below(symbols.len())] + rng.below(2);
            if at > indentation && at < line.len() {
                lines[index].insert(at, ' ');
            }
        }
        5 | 6 if !spaces.is_empty() => {
            lines[index].remove(spaces[rng.below(spaces.len())]);
        }
        7 if !spaces.is_empty() => lines[index].insert(spaces[rng.below(spaces.len())], ' '),
        8 => lines[index].insert_str(0, &indent(1)),
        9 if line.starts_with(&indent(1)) => {
            lines[index].drain(..usize_from_u32(LEVEL_SPACES));
        }
        10 if index + 1 < lines.len() => {
            let next = lines.remove(index + 1);
            lines[index] = format!("{line}{}{}", rng.pick(&["", " "]), next.trim_start());
        }
        11 if !spaces.is_empty() => {
            let at = spaces[rng.below(spaces.len())];
            let levels = u32::try_from(rng.below(6)).expect("a few levels");
            lines[index] = String::from(slice(&line, 0..at));
            let rest = slice(&line, at + 1..line.len());
            lines.insert(index + 1, format!("{}{rest}", indent(levels)));
        }
        12 => {
            if let Some(at) = line.rfind(['{', '}']) {
                let (before, after) = (slice(&line, 0..at), slice(&line, at + 1..line.len()));
                lines[index] = format!("{}{after}", before.trim_end());
            }
        }
        13 => {
            let levels = u32::try_from(rng.below(6)).expect("a few levels");
            lines.insert(
                index,
                format!("{}{}", indent(levels), rng.pick(&["}", "{", "a {", "// c"])),
            );
        }
        14 if !spaces.is_empty() => {
            let at = spaces[rng.below(spaces.len())];
            lines[index].insert_str(at, &" ".repeat(rng.below(300)));
        }
        15 => {
            let levels = u32::try_from(rng.below(75)).expect("a few levels");
            lines[index].insert_str(0, &indent(levels));
        }
        16 => {
            for _ in 0..rng.below(300) {
                lines.insert(index, String::new());
            }
        }
        17 if code.ends_with('{') && code != "{" && !code.starts_with("//") => {
            lines[index] = format!("{}{{", slice(&line, 0..indentation));
        }
        _ => lines[index].push_str(rng.pick(&[" x", "(", ")", " {", "{", " }", "}"])),
    }
}

/// Returns the name of `kind`'s variant, without what it holds.
fn variant(kind: Kind) -> String {
    let name = format!("{kind:?}");
    String::from(name.split('(').next().expect("a name"))
}

#[test]
fn the_smallest_program() {
    // The smallest program is laid out the one way, and so are comments and blank lines around
    // it, and a source of nothing but comments.
    let mut examples = Examples::new();
    examples.add(SMALLEST, Ok(()));
    // Comments and blank lines can go anywhere, and a comment is indented like any other line.
    examples.add(
        "// above\n\nfunction main() returns Void {\n    // inside\n\n    //\n}\n\n// below\n",
        Ok(()),
    );
    // A source of only comments has no code to lay out.
    examples.add("// only a comment\n", Ok(()));
    examples.add("//\n\n// and another\n", Ok(()));
    check(examples);
}

#[test]
fn lines() {
    // The first step: where each line ends, and where a brace sits on its line.
    let mut examples = Examples::new();
    // `{` ends its line, even for an empty body.
    examples.add("function main() returns Void {}\n", Err((BraceOpenNotLast, 1, 30)));
    examples.add("function main() returns Void { }\n", Err((BraceOpenNotLast, 1, 30)));
    examples.add("a { b\n}\n", Err((BraceOpenNotLast, 1, 3)));
    examples.add("a { {\n    }\n}\n", Err((BraceOpenNotLast, 1, 3)));
    // `}` is the only token on its line.
    examples.add("a {\nb }\n", Err((BraceCloseNotAlone, 2, 3)));
    examples.add("a {\n} b\n", Err((BraceCloseNotAlone, 2, 1)));
    examples.add("a {\n    b {\n    }}\n", Err((BraceCloseNotAlone, 3, 5)));
    // A line of code ends with `{` or `}`, so no other token can end one. So a function's first
    // line is never split, wherever it would break, and can't leave its `{` to the next line.
    for (_, text) in FIRSTS {
        let after = u32::try_from(text.len()).expect("a short word") + 1;
        examples.add(&format!("{text}\n"), Err((LineEndsWithoutBrace, 1, after)));
    }
    examples.add("function main() returns Void\n{\n}\n", Err((LineEndsWithoutBrace, 1, 29)));
    examples.add("function main() returns\nVoid {\n}\n", Err((LineEndsWithoutBrace, 1, 24)));
    examples.add("function main()\nreturns Void {\n}\n", Err((LineEndsWithoutBrace, 1, 16)));
    examples.add("function main(\n) returns Void {\n}\n", Err((LineEndsWithoutBrace, 1, 15)));
    examples.add("function main\n() returns Void {\n}\n", Err((LineEndsWithoutBrace, 1, 14)));
    examples.add("function\nmain() returns Void {\n}\n", Err((LineEndsWithoutBrace, 1, 9)));
    examples
        .add("function main() returns Void {\n    main()\n}\n", Err((LineEndsWithoutBrace, 2, 11)));
    examples.add("main\n", Err((LineEndsWithoutBrace, 1, 5)));
    examples.add("(\n", Err((LineEndsWithoutBrace, 1, 2)));
    // A comment is a whole line, so it ends a line of its own, and a blank line has no code.
    examples.add("function main() returns Void {\n    // a comment ends here\n\n}\n", Ok(()));
    // A `{` can't start a line: some token comes before it on its line.
    examples.add("{\n}\n", Err((BraceOpenStartsLine, 1, 1)));
    examples.add("a {\n    {\n    }\n}\n", Err((BraceOpenStartsLine, 2, 5)));
    examples.add("a {\n}\n{\n}\n", Err((BraceOpenStartsLine, 3, 1)));
    examples.add("// a\n{\n}\n", Err((BraceOpenStartsLine, 2, 1)));
    // However far down it is, after blank lines too.
    examples.add("a {\n\n\n    {\n    }\n}\n", Err((BraceOpenStartsLine, 4, 5)));
    let far_down = format!("a {{\n{}    {{\n    }}\n}}\n", "\n".repeat(255));
    examples.add(&far_down, Err((BraceOpenStartsLine, 257, 5)));
    // Which tokens can come before it is for the grammar to say, so for the layout, any that
    // can share its line can.
    for (_, text) in FIRSTS {
        examples.add(&format!("{text}{}{{\n}}\n", if text == "(" { "" } else { " " }), Ok(()));
    }
    check(examples);
}

#[test]
fn braces() {
    // The second step: whether every `}` closes a `{`, and every `{` is closed.
    let mut examples = Examples::new();
    // Every `}` closes a `{` before it.
    examples.add("}\n", Err((BraceClosesNothing, 1, 1)));
    examples.add("a {\n}\n}\n", Err((BraceClosesNothing, 3, 1)));
    examples.add("// a\n}\na {\n", Err((BraceClosesNothing, 2, 1)));
    // Every `{` is closed. The one found is the outermost left open.
    examples.add("a {\n", Err((BraceNeverClosed, 1, 3)));
    examples.add("function main() returns Void {\n", Err((BraceNeverClosed, 1, 30)));
    examples.add("a {\n    b {\n    }\n", Err((BraceNeverClosed, 1, 3)));
    examples.add("a {\n    b {\n", Err((BraceNeverClosed, 1, 3)));
    examples.add("a {\n}\nb {\n", Err((BraceNeverClosed, 3, 3)));
    examples.add("a {\n}\nb {\n    c {\n    }\n", Err((BraceNeverClosed, 3, 3)));
    // Braces that pair up pass, one inside another or one after another.
    examples.add("a {\n    b {\n        c {\n        }\n    }\n    d {\n    }\n}\n", Ok(()));
    examples.add("a {\n}\nb {\n}\n", Ok(()));
    check(examples);
}

#[test]
fn indentation() {
    // The last step, for a line's first token: its line is indented one level for each `{` still
    // open on it.
    let mut examples = Examples::new();
    // A line is indented one level for each `{` still open on it.
    examples.add("function main() returns Void {\n// hi\n}\n", Err((IndentationNotDepth(1), 2, 1)));
    examples.add("    function main() returns Void {\n}\n", Err((IndentationNotDepth(0), 1, 1)));
    // Too deep, the first space past the right indentation is at fault. Too shallow, the line's
    // first token is.
    examples.add(
        "function main() returns Void {\n        // hi\n}\n",
        Err((IndentationNotDepth(1), 2, 5)),
    );
    examples.add("a {\n    b {\n    // hi\n    }\n}\n", Err((IndentationNotDepth(2), 3, 5)));
    // A line that starts with `}` counts its own brace as closed.
    examples.add("function main() returns Void {\n    }\n", Err((IndentationNotDepth(0), 2, 1)));
    examples.add("a {\n    b {\n}\n}\n", Err((IndentationNotDepth(1), 3, 1)));
    examples.add("a {\n    b {\n            }\n    }\n", Err((IndentationNotDepth(1), 3, 5)));
    // Code and comments alike, at every depth.
    examples
        .add("a {\n    // one\n    b {\n        // two\n    }\n    // one\n}\n// none\n", Ok(()));
    // A comment is placed by its indentation alone: the braces in it aren't code, and its text
    // can be any Unicode.
    examples.add("a {\n    // café } {\n}\n", Ok(()));
    examples.add("a {\n// } {\n}\n", Err((IndentationNotDepth(1), 2, 1)));
    // A line after a comment that isn't ASCII is placed by its own columns.
    examples.add("// café\nmain  {\n}\n", Err((SpacesExtra(between(Name, BraceOpen)), 2, 6)));
    check(examples);
}

#[test]
fn every_indentation() {
    // The same rule tried whole: every kind of line, at every depth up to three, indented every
    // way up to four levels.
    let mut examples = Examples::new();
    // A line of each kind, a comment, a line that opens a brace and one that closes one, at every
    // depth up to three, indented every way up to four levels. The lines around it are laid out
    // the one way.
    let mut cases_count: u32 = 0;
    for depth in 0..=3_u32 {
        for levels in 0..=4_u32 {
            for body in ["// x", "b {", "}"] {
                let mut lines: Vec<String> =
                    (0..depth).map(|level| format!("{}a {{", indent(level))).collect();
                lines.push(format!("{}{body}", indent(levels)));
                let still_open = match body {
                    "b {" => depth + 1,
                    "}" => depth.saturating_sub(1),
                    _ => depth,
                };
                lines.extend((0..still_open).rev().map(|level| format!("{}}}", indent(level))));
                let (source, line) = (source_of(&lines), depth + 1);
                // One level for each `{` still open on the line, and a `}` counts its own brace
                // as closed, so one with nothing open has no level at all.
                let wanted = if body == "}" { depth.checked_sub(1) } else { Some(depth) };
                match wanted {
                    None => {
                        examples.add(&source, Err((BraceClosesNothing, line, column_at(levels))));
                    }
                    Some(wanted) => match levels.cmp(&wanted) {
                        Ordering::Equal => examples.add(&source, Ok(())),
                        Ordering::Greater => {
                            examples.add(
                                &source,
                                Err((IndentationNotDepth(wanted), line, column_at(wanted))),
                            );
                        }
                        Ordering::Less => {
                            examples.add(
                                &source,
                                Err((IndentationNotDepth(wanted), line, column_at(levels))),
                            );
                        }
                    },
                }
                cases_count += 1;
            }
        }
    }
    assert_eq!(cases_count, 4 * 5 * 3);
    check(examples);
}

#[test]
fn spaces() {
    // The last step, between two tokens on a line: one space, or none beside a parenthesis.
    let mut examples = Examples::new();
    // A function's first line is written one way, with single spaces as shown.
    examples.add(SMALLEST, Ok(()));
    examples.add(
        "function main () returns Void {\n}\n",
        Err((SpaceUnwanted(between(Name, ParenthesisOpen)), 1, 14)),
    );
    examples.add(
        "function main() returns Void{\n}\n",
        Err((SpaceMissing(between(TypeName, BraceOpen)), 1, 29)),
    );
    examples.add(
        "function  main() returns Void {\n}\n",
        Err((SpacesExtra(between(Function, Name)), 1, 10)),
    );
    examples.add(
        "function main( ) returns Void {\n}\n",
        Err((SpaceUnwanted(between(ParenthesisOpen, ParenthesisClose)), 1, 15)),
    );
    examples.add(
        "function main()returns Void {\n}\n",
        Err((SpaceMissing(between(ParenthesisClose, Returns)), 1, 16)),
    );
    examples.add(
        "function main()  returns Void {\n}\n",
        Err((SpacesExtra(between(ParenthesisClose, Returns)), 1, 17)),
    );
    examples.add(
        "function main() returns   Void {\n}\n",
        Err((SpacesExtra(between(Returns, TypeName)), 1, 25)),
    );
    examples.add(
        "function main() returns Void  {\n}\n",
        Err((SpacesExtra(between(TypeName, BraceOpen)), 1, 30)),
    );
    // More than one space too many is still at the first one too many.
    examples.add(
        "function    main() returns Void {\n}\n",
        Err((SpacesExtra(between(Function, Name)), 1, 10)),
    );
    examples.add(
        "function main   () returns Void {\n}\n",
        Err((SpaceUnwanted(between(Name, ParenthesisOpen)), 1, 14)),
    );
    // Lines that are indented are spaced the same way.
    examples.add("a {\n    b  {\n    }\n}\n", Err((SpacesExtra(between(Name, BraceOpen)), 2, 7)));
    examples.add(
        "a {\n    b( {\n    }\n}\n",
        Err((SpaceUnwanted(between(ParenthesisOpen, BraceOpen)), 2, 7)),
    );
    // No space goes after `(`, or before `(` or `)`.
    examples.add("a(b)(c) {\n}\n", Ok(()));
    check(examples);
}

#[test]
fn every_gap() {
    // The same rule tried whole: every pair of tokens that can share a line, with every gap up
    // to two spaces.
    let mut examples = Examples::new();
    // Every pair of tokens that can share a line, with no space between them, one, or two. The
    // rest of the line is laid out the one way, so the gap is all that can be wrong.
    let mut cases_count: u32 = 0;
    for (row, &(first, first_text)) in FIRSTS.iter().enumerate() {
        for (column, &(second, second_text)) in SECONDS.iter().enumerate() {
            let wanted = SPACES[row][column];
            for gap in 0..=2 {
                // Two words with nothing between them are one word.
                if gap == 0 && is_word(first) && is_word(second) {
                    continue;
                }
                let rest = match second {
                    BraceOpen => "",
                    ParenthesisOpen => ") {",
                    Function | Returns | Name | TypeName | ParenthesisClose => " {",
                    BraceClose | TokenKind::Comment => {
                        unreachable!("no line here has a `}}` or a comment second")
                    }
                };
                let spaces = " ".repeat(gap);
                let source = format!("{first_text}{spaces}{second_text}{rest}\n}}\n");
                let after_first = u32::try_from(first_text.len()).expect("a short word") + 1;
                match gap.cmp(&wanted) {
                    Ordering::Equal => examples.add(&source, Ok(())),
                    // The second token is where the space should be.
                    Ordering::Less => {
                        examples.add(
                            &source,
                            Err((SpaceMissing(between(first, second)), 1, after_first)),
                        );
                    }
                    Ordering::Greater if wanted == 0 => {
                        examples.add(
                            &source,
                            Err((SpaceUnwanted(between(first, second)), 1, after_first)),
                        );
                    }
                    Ordering::Greater => {
                        examples.add(
                            &source,
                            Err((SpacesExtra(between(first, second)), 1, after_first + 1)),
                        );
                    }
                }
                cases_count += 1;
            }
        }
    }
    assert_eq!(cases_count, 6 * 7 * 3 - 4 * 4);
    check(examples);
}

#[test]
fn the_steps_run_in_order() {
    // Each source has a problem for two steps, with the later step's first in the file. The
    // earlier step's is the one reported.
    let mut examples = Examples::new();
    // The lines come first, so a problem with them wins over a problem with the braces, or
    // within a line, that comes earlier in the file.
    examples.add("}\na  {\nb\n", Err((LineEndsWithoutBrace, 3, 2)));
    examples.add("a  {\n}\n}\nb { c\n", Err((BraceOpenNotLast, 4, 3)));
    examples.add("}\n{\n", Err((BraceOpenStartsLine, 2, 1)));
    examples.add("a  {\n}\n{\n}\n", Err((BraceOpenStartsLine, 3, 1)));
    examples.add("    {\n}\n", Err((BraceOpenStartsLine, 1, 5)));
    // Then the braces, so a problem with them wins over one within a line, even earlier on the
    // same line.
    examples.add("a  {\n// x\n}\n}\n", Err((BraceClosesNothing, 4, 1)));
    examples.add("a  {\n", Err((BraceNeverClosed, 1, 4)));
    examples.add("    a {\n", Err((BraceNeverClosed, 1, 7)));
    check(examples);
}

#[test]
fn the_first_problem_wins() {
    // Each source has two problems for one step, and the earlier in the file is the one reported.
    let mut examples = Examples::new();
    // Within each step, the earliest problem in the file wins.
    examples.add("a b\n{ }\n", Err((LineEndsWithoutBrace, 1, 4)));
    examples.add("a {\n} b\nc\n", Err((BraceCloseNotAlone, 2, 1)));
    // On one line, the first token at fault. A `{` that starts its line and doesn't end it is
    // refused for not ending it.
    examples.add("{ }\n", Err((BraceOpenNotLast, 1, 1)));
    examples.add("{ a\n", Err((BraceOpenNotLast, 1, 1)));
    examples.add("{\n}\na\n", Err((BraceOpenStartsLine, 1, 1)));
    examples.add("a } {\n", Err((BraceCloseNotAlone, 1, 3)));
    examples.add("a } b\n", Err((BraceCloseNotAlone, 1, 3)));
    examples.add("} { a\n", Err((BraceCloseNotAlone, 1, 1)));
    // A `}` with nothing open always comes before any `{` left open, and of several, the first.
    examples.add("a {\n}\n}\nb {\n", Err((BraceClosesNothing, 3, 1)));
    examples.add("}\n}\n", Err((BraceClosesNothing, 1, 1)));
    // Indentation and spaces are checked in one pass, so whichever comes first wins.
    examples.add("a  {\n// x\n}\n", Err((SpacesExtra(between(Name, BraceOpen)), 1, 3)));
    examples.add("a {\n// x\n    b  {\n    }\n}\n", Err((IndentationNotDepth(1), 2, 1)));
    examples
        .add("a {\n    b  {\n// x\n    }\n}\n", Err((SpacesExtra(between(Name, BraceOpen)), 2, 7)));
    // On one line, its indentation comes before the spaces after it.
    examples.add("a {\n        b  {\n    }\n}\n", Err((IndentationNotDepth(1), 2, 5)));
    examples.add("a {\nb  {\n    }\n}\n", Err((IndentationNotDepth(1), 2, 1)));
    // And of two gaps on a line, the first.
    examples.add(
        "function main ()  returns Void{\n}\n",
        Err((SpaceUnwanted(between(Name, ParenthesisOpen)), 1, 14)),
    );
    check(examples);
}

#[test]
fn the_largest_sources() {
    // Depths and line numbers go as far as the largest source lets them, so none of them can be
    // counted in anything smaller.
    let mut examples = Examples::new();
    let size = usize_from_u32(SOURCE_BYTES_MAX);
    // The deepest nesting laid out the one way.
    assert!(source_of(&nested_blocks(63)).len() <= size);
    assert!(source_of(&nested_blocks(64)).len() > size);
    examples.add(&source_of(&nested_blocks(63)), Ok(()));
    // The deepest a line can be refused for its indentation, too shallow, and too deep.
    assert!(opened_then(89, "({").len() <= size);
    assert!(opened_then(90, "({").len() > size);
    examples.add(&opened_then(89, "({"), Err((IndentationNotDepth(89), 90, 1)));
    let too_deep = |depth: u32| opened_then(depth, &format!("{}({{", indent(depth + 1)));
    assert!(too_deep(88).len() <= size);
    assert!(too_deep(89).len() > size);
    examples.add(&too_deep(88), Err((IndentationNotDepth(88), 89, column_at(88))));
    // Braces go deeper still when they're refused before any indentation is looked at.
    examples.add(&"({\n".repeat(size / 3), Err((BraceNeverClosed, 1, 2)));
    let half = format!("{}{}", "({\n".repeat(size / 5), "}\n".repeat(size / 5));
    examples.add(&half, Err((IndentationNotDepth(1), 2, 1)));
    // The most lines of code, opening and closing one block after another.
    examples.add(&"({\n}\n".repeat(size / 5), Ok(()));
    let last_line = u32::try_from(size / 5 * 2 - 1).expect("a line number");
    let left_open = format!("{}({{\n", "({\n}\n".repeat(size / 5 - 1));
    examples.add(&left_open, Err((BraceNeverClosed, last_line, 2)));
    // The most lines of all: every one blank but the last, a `}` with nothing to close.
    let most_lines = format!("{}}}\n", "\n".repeat(size - 2));
    examples.add(&most_lines, Err((BraceClosesNothing, SOURCE_BYTES_MAX - 1, 1)));
    // The last lines the last steps can reach, after the most blank lines: the last line that
    // lays out, the last refused within its line, and the last refused for its braces.
    examples.add(&format!("{}//\n", "\n".repeat(size - 3)), Ok(()));
    let last_gap = format!("{}a{{\n}}\n", "\n".repeat(size - 5));
    examples.add(&last_gap, Err((SpaceMissing(between(Name, BraceOpen)), SOURCE_BYTES_MAX - 4, 2)));
    let last_comment = format!("{}    //\n", "\n".repeat(size - 7));
    examples.add(&last_comment, Err((IndentationNotDepth(0), SOURCE_BYTES_MAX - 6, 1)));
    let last_brace = format!("{}{{\n", "\n".repeat(size - 2));
    examples.add(&last_brace, Err((BraceOpenStartsLine, SOURCE_BYTES_MAX - 1, 1)));
    check(examples);
}

#[test]
fn the_widest_lines() {
    // Gaps, indentation and columns go as far as the largest source lets them, so none of them
    // can be counted in anything smaller. A byte counts up to 255, so widths just past it are
    // tried too.
    let mut examples = Examples::new();
    let size = usize_from_u32(SOURCE_BYTES_MAX);
    // Gaps of two spaces, just past what a byte counts, and the widest.
    for spaces in [2, 256, 257, size - 5] {
        let source = format!("a{}{{\n}}\n", " ".repeat(spaces));
        examples.add(&source, Err((SpacesExtra(between(Name, BraceOpen)), 1, 3)));
    }
    // Indentation of one level, just past what a byte counts, and the widest.
    for levels in [1, 64, 65, (SOURCE_BYTES_MAX - 4) / LEVEL_SPACES] {
        let source = format!("{}//\n", indent(levels));
        examples.add(&source, Err((IndentationNotDepth(0), 1, 1)));
    }
    // A line of code can fill the largest source, and its problems are as far right as a column
    // goes: just past the line's end.
    examples.add(&format!("a{} {{\n}}\n", ")".repeat(size - 6)), Ok(()));
    let longest = format!("a{}\n", ")".repeat(size - 2));
    examples.add(&longest, Err((LineEndsWithoutBrace, 1, SOURCE_BYTES_MAX)));
    let spaced = format!("a{}  {{\n}}\n", ")".repeat(size - 7));
    examples.add(
        &spaced,
        Err((SpacesExtra(between(ParenthesisClose, BraceOpen)), 1, SOURCE_BYTES_MAX - 4)),
    );
    // The most tokens that lay out: a `(` in every byte but the last four.
    examples.add(&format!("{}{{\n}}\n", "(".repeat(size - 4)), Ok(()));
    check(examples);
}

#[test]
fn fuzz_against_the_oracle() {
    // 40,000 random programs, laid out the one way and then changed up to twice, each checked
    // against what the oracle says of it. Every kind of problem is counted, so the test fails if
    // the programs stop reaching one.
    let mut rng = Rng::new(0x4C41_594F_5554_0001);
    // Each source, and whether it was changed after it was laid out the one way.
    let mut sources = Vec::new();
    for _ in 0_u32..40_000 {
        let mut lines = random_program(&mut rng);
        let mutations = rng.below(3);
        for _ in 0..mutations {
            mutate(&mut rng, &mut lines);
        }
        sources.push((source_of(&lines), mutations > 0));
    }
    let kinds = [
        BraceOpenNotLast,
        BraceOpenStartsLine,
        BraceCloseNotAlone,
        LineEndsWithoutBrace,
        BraceClosesNothing,
        BraceNeverClosed,
        IndentationNotDepth(0),
        SpaceMissing(between(Name, Name)),
        SpaceUnwanted(between(Name, Name)),
        SpacesExtra(between(Name, Name)),
    ];
    let mut kind_counts: Vec<(String, u32)> =
        kinds.iter().map(|&kind| (variant(kind), 0)).collect();
    let (mut skipped_count, mut changed_accepted_count): (u32, u32) = (0, 0);
    let mut checks = Vec::new();
    for (source, changed) in &sources {
        // A source the text lint or the lexer refuses has no tokens to lint.
        if !has_tokens(source) {
            skipped_count += 1;
            continue;
        }
        let expected = oracle(source);
        match &expected {
            Ok(()) => changed_accepted_count += u32::from(*changed),
            Err((kind, ..)) => {
                let name = variant(*kind);
                let (_, count) =
                    kind_counts.iter_mut().find(|(each, _)| *each == name).expect(&name);
                *count += 1;
            }
        }
        checks.push((source, expected));
    }
    // The fuzzing only means something if the sources reach every answer, often, and if sources
    // that were changed are still laid out the one way often too, so passing isn't only the
    // unchanged ones. This is checked before anything is linted, so it holds whatever the tokens
    // lint does.
    let accepted_count = checks.iter().filter(|(_, expected)| expected.is_ok()).count();
    assert!(accepted_count > 14_000, "only {accepted_count} were accepted");
    assert!(changed_accepted_count > 4_000, "only {changed_accepted_count} changed were accepted");
    assert!(skipped_count < 5_000, "{skipped_count} were refused before the tokens lint");
    for (name, count) in kind_counts {
        assert!(count > 400, "only {count} were refused with {name}");
    }
    // The oracle made these answers, so each goes straight to the tokens lint.
    for (source, expected) in checks {
        assert_eq!(checked(source), expected, "for {}", excerpt(source));
    }
}
