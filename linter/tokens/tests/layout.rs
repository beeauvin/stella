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
use std::panic::Location;

use stella_lexer::TokenKind::{self, *};
use stella_lexer::{TokenBuffer, lex};
use stella_linter_text::{LEVEL_SPACES, lint};
use stella_linter_tokens::Kind::{
    BraceCloseNotAlone, BraceClosesNothing, BraceNeverClosed, BraceOpenNotLast,
    BraceOpenStartsLine, IndentationNotDepth, LineEndsWithoutBrace, SpaceMissing, SpaceUnwanted,
    SpacesExtra,
};
use stella_linter_tokens::{Kind, Problem, lint_tokens};
use stella_source::SOURCE_BYTES_MAX;

/// An expected refusal: the problem's kind, and its line and column.
type Refusal = (Kind, u32, u32);

/// This is the smallest program, laid out the one way.
const SMALLEST: &str = "function main() returns Void {\n}\n";

thread_local! {
    /// Each test lexes every source it tries into this one buffer, as the command does.
    static BUFFER: RefCell<TokenBuffer> = RefCell::new(TokenBuffer::new());
}

/// Lints and lexes `source`, which must pass both, and then lints its tokens. Returns nothing when
/// the tokens lint passes them, or its problem. A layout that passes must hold the very tokens it
/// was given.
fn checked(source: &str) -> Result<(), Refusal> {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    BUFFER.with_borrow_mut(|buffer| {
        let tokens = lex(text, buffer);
        let tokens = tokens.unwrap_or_else(|problem| panic!("{source:?} should lex: {problem:?}"));
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
    BUFFER.with_borrow_mut(|buffer| lex(text, buffer).is_ok())
}

/// The examples a test tries. `check` tries the oracle on every one before the tokens lint sees
/// any, so every example checks the oracle, even before the tokens lint can run.
#[derive(Default)]
struct Examples {
    examples: Vec<Example>,
}

/// An example is a source, what the tokens lint should make of it, and the line of the test that
/// added it, so a failure points at the example rather than at `check`.
struct Example {
    source: String,
    expected: Result<(), Refusal>,
    at: &'static Location<'static>,
}

impl Examples {
    /// Adds `source`, which must be laid out the one way.
    #[track_caller]
    fn lays_out(&mut self, source: &str) {
        let at = Location::caller();
        self.examples.push(Example { source: String::from(source), expected: Ok(()), at });
    }

    /// Adds `source`, which must be refused with `kind` at `line` and `column`.
    #[track_caller]
    fn refuses(&mut self, source: &str, kind: Kind, line: u32, column: u32) {
        let (at, expected) = (Location::caller(), Err((kind, line, column)));
        self.examples.push(Example { source: String::from(source), expected, at });
    }

    /// Checks that every example passes the text lint and the lexer, as the oracle assumes, and
    /// agrees with the oracle. Then checks every one against the tokens lint.
    fn check(mut self) {
        let examples = std::mem::take(&mut self.examples);
        assert!(!examples.is_empty(), "a test tries at least one example");
        for Example { source, expected, at } in &examples {
            let shown = shown(source);
            assert!(has_tokens(source), "{at}: {shown} should pass the text lint and the lexer");
            assert_eq!(oracle(source), *expected, "{at}: the oracle, for {shown}");
        }
        for Example { source, expected, at } in &examples {
            assert_eq!(checked(source), *expected, "{at}: for {}", shown(source));
        }
    }
}

/// Returns `source` as a failure shows it: whole when it's short, or else its start and its
/// length, since the largest sources are 16,384 bytes.
fn shown(source: &str) -> String {
    if source.len() <= 120 {
        format!("{source:?}")
    } else {
        let start: String = source.chars().take(60).collect();
        format!("{start:?}, and on to {} bytes", source.len())
    }
}

impl Drop for Examples {
    /// Every example added is checked, so a test can't pass by forgetting to. A test that has
    /// already failed is left to report its own failure.
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.examples.is_empty(), "examples were added but never checked");
        }
    }
}

/// Returns the spaces that indent a line `levels` levels.
fn indent(levels: u32) -> String {
    " ".repeat((levels * LEVEL_SPACES) as usize)
}

/// Returns the column of a line's first token when it's indented `levels` levels.
fn column_at(levels: u32) -> u32 {
    levels * LEVEL_SPACES + 1
}

/// Returns `lines` as a source, each ending in `\n`.
fn source_of(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

#[test]
fn the_smallest_program() {
    let mut examples = Examples::default();
    examples.lays_out(SMALLEST);
    // Comments and blank lines can go anywhere, and a comment is indented like any other line.
    examples.lays_out(
        "// above\n\nfunction main() returns Void {\n    // inside\n\n    //\n}\n\n// below\n",
    );
    // A source of only comments has no code to lay out.
    examples.lays_out("// only a comment\n");
    examples.lays_out("//\n\n// and another\n");
    examples.check();
}

#[test]
fn lines() {
    let mut examples = Examples::default();
    // `{` ends its line, even for an empty body.
    examples.refuses("function main() returns Void {}\n", BraceOpenNotLast, 1, 30);
    examples.refuses("function main() returns Void { }\n", BraceOpenNotLast, 1, 30);
    examples.refuses("a { b\n}\n", BraceOpenNotLast, 1, 3);
    examples.refuses("a { {\n    }\n}\n", BraceOpenNotLast, 1, 3);
    // `}` is the only token on its line.
    examples.refuses("a {\nb }\n", BraceCloseNotAlone, 2, 3);
    examples.refuses("a {\n} b\n", BraceCloseNotAlone, 2, 1);
    examples.refuses("a {\n    b {\n    }}\n", BraceCloseNotAlone, 3, 5);
    // A line of code ends with `{` or `}`, so no other token can end one. So a function's first
    // line is never split, wherever it would break, and can't leave its `{` to the next line.
    for (_, text) in FIRSTS {
        let after = u32::try_from(text.len()).expect("a short word") + 1;
        examples.refuses(&format!("{text}\n"), LineEndsWithoutBrace, 1, after);
    }
    examples.refuses("function main() returns Void\n{\n}\n", LineEndsWithoutBrace, 1, 29);
    examples.refuses("function main() returns\nVoid {\n}\n", LineEndsWithoutBrace, 1, 24);
    examples.refuses("function main()\nreturns Void {\n}\n", LineEndsWithoutBrace, 1, 16);
    examples.refuses("function main(\n) returns Void {\n}\n", LineEndsWithoutBrace, 1, 15);
    examples.refuses("function main\n() returns Void {\n}\n", LineEndsWithoutBrace, 1, 14);
    examples.refuses("function\nmain() returns Void {\n}\n", LineEndsWithoutBrace, 1, 9);
    examples.refuses(
        "function main() returns Void {\n    main()\n}\n",
        LineEndsWithoutBrace,
        2,
        11,
    );
    examples.refuses("main\n", LineEndsWithoutBrace, 1, 5);
    examples.refuses("(\n", LineEndsWithoutBrace, 1, 2);
    // A comment is a whole line, so it ends a line of its own, and a blank line has no code.
    examples.lays_out("function main() returns Void {\n    // a comment ends here\n\n}\n");
    // A `{` can't start a line: some token comes before it on its line.
    examples.refuses("{\n}\n", BraceOpenStartsLine, 1, 1);
    examples.refuses("a {\n    {\n    }\n}\n", BraceOpenStartsLine, 2, 5);
    examples.refuses("a {\n}\n{\n}\n", BraceOpenStartsLine, 3, 1);
    examples.refuses("// a\n{\n}\n", BraceOpenStartsLine, 2, 1);
    // However far down it is, after blank lines too.
    examples.refuses("a {\n\n\n    {\n    }\n}\n", BraceOpenStartsLine, 4, 5);
    let far_down = format!("a {{\n{}    {{\n    }}\n}}\n", "\n".repeat(255));
    examples.refuses(&far_down, BraceOpenStartsLine, 257, 5);
    // Which tokens can come before it is for the grammar to say, so for the layout, any that
    // can share its line can.
    for (_, text) in FIRSTS {
        examples.lays_out(&format!("{text}{}{{\n}}\n", if text == "(" { "" } else { " " }));
    }
    examples.check();
}

#[test]
fn braces() {
    let mut examples = Examples::default();
    // Every `}` closes a `{` before it.
    examples.refuses("}\n", BraceClosesNothing, 1, 1);
    examples.refuses("a {\n}\n}\n", BraceClosesNothing, 3, 1);
    examples.refuses("// a\n}\na {\n", BraceClosesNothing, 2, 1);
    // Every `{` is closed. The one found is the outermost left open.
    examples.refuses("a {\n", BraceNeverClosed, 1, 3);
    examples.refuses("function main() returns Void {\n", BraceNeverClosed, 1, 30);
    examples.refuses("a {\n    b {\n    }\n", BraceNeverClosed, 1, 3);
    examples.refuses("a {\n    b {\n", BraceNeverClosed, 1, 3);
    examples.refuses("a {\n}\nb {\n", BraceNeverClosed, 3, 3);
    examples.refuses("a {\n}\nb {\n    c {\n    }\n", BraceNeverClosed, 3, 3);
    // Braces that pair up pass, one inside another or one after another.
    examples.lays_out("a {\n    b {\n        c {\n        }\n    }\n    d {\n    }\n}\n");
    examples.lays_out("a {\n}\nb {\n}\n");
    examples.check();
}

#[test]
fn indentation() {
    let mut examples = Examples::default();
    // A line is indented one level for each `{` still open on it.
    examples.refuses("function main() returns Void {\n// hi\n}\n", IndentationNotDepth(1), 2, 1);
    examples.refuses("    function main() returns Void {\n}\n", IndentationNotDepth(0), 1, 1);
    // Too deep, the first space past the right indentation is at fault. Too shallow, the line's
    // first token is.
    examples.refuses(
        "function main() returns Void {\n        // hi\n}\n",
        IndentationNotDepth(1),
        2,
        5,
    );
    examples.refuses("a {\n    b {\n    // hi\n    }\n}\n", IndentationNotDepth(2), 3, 5);
    // A line that starts with `}` counts its own brace as closed.
    examples.refuses("function main() returns Void {\n    }\n", IndentationNotDepth(0), 2, 1);
    examples.refuses("a {\n    b {\n}\n}\n", IndentationNotDepth(1), 3, 1);
    examples.refuses("a {\n    b {\n            }\n    }\n", IndentationNotDepth(1), 3, 5);
    // Code and comments alike, at every depth.
    examples.lays_out("a {\n    // one\n    b {\n        // two\n    }\n    // one\n}\n// none\n");
    // A comment is placed by its indentation alone: the braces in it aren't code, and its text
    // can be any Unicode.
    examples.lays_out("a {\n    // café } {\n}\n");
    examples.refuses("a {\n// } {\n}\n", IndentationNotDepth(1), 2, 1);
    // A line after a comment that isn't ASCII is placed by its own columns.
    examples.refuses("// café\nmain  {\n}\n", SpacesExtra(Name, BraceOpen), 2, 6);
    examples.check();
}

#[test]
fn every_indentation() {
    let mut examples = Examples::default();
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
                    None => examples.refuses(&source, BraceClosesNothing, line, column_at(levels)),
                    Some(wanted) => match levels.cmp(&wanted) {
                        Ordering::Equal => examples.lays_out(&source),
                        Ordering::Greater => {
                            examples.refuses(
                                &source,
                                IndentationNotDepth(wanted),
                                line,
                                column_at(wanted),
                            );
                        }
                        Ordering::Less => {
                            examples.refuses(
                                &source,
                                IndentationNotDepth(wanted),
                                line,
                                column_at(levels),
                            );
                        }
                    },
                }
                cases_count += 1;
            }
        }
    }
    assert_eq!(cases_count, 4 * 5 * 3);
    examples.check();
}

#[test]
fn spaces() {
    let mut examples = Examples::default();
    // A function's first line is written one way, with single spaces as shown.
    examples.lays_out(SMALLEST);
    examples.refuses("function main () returns Void {\n}\n", SpaceUnwanted(Name, ParenOpen), 1, 14);
    examples.refuses(
        "function main() returns Void{\n}\n",
        SpaceMissing(TypeName, BraceOpen),
        1,
        29,
    );
    examples.refuses("function  main() returns Void {\n}\n", SpacesExtra(Function, Name), 1, 10);
    examples.refuses(
        "function main( ) returns Void {\n}\n",
        SpaceUnwanted(ParenOpen, ParenClose),
        1,
        15,
    );
    examples.refuses(
        "function main()returns Void {\n}\n",
        SpaceMissing(ParenClose, Returns),
        1,
        16,
    );
    examples.refuses(
        "function main()  returns Void {\n}\n",
        SpacesExtra(ParenClose, Returns),
        1,
        17,
    );
    examples.refuses(
        "function main() returns   Void {\n}\n",
        SpacesExtra(Returns, TypeName),
        1,
        25,
    );
    examples.refuses(
        "function main() returns Void  {\n}\n",
        SpacesExtra(TypeName, BraceOpen),
        1,
        30,
    );
    // More than one space too many is still at the first one too many.
    examples.refuses("function    main() returns Void {\n}\n", SpacesExtra(Function, Name), 1, 10);
    examples.refuses(
        "function main   () returns Void {\n}\n",
        SpaceUnwanted(Name, ParenOpen),
        1,
        14,
    );
    // Lines that are indented are spaced the same way.
    examples.refuses("a {\n    b  {\n    }\n}\n", SpacesExtra(Name, BraceOpen), 2, 7);
    examples.refuses("a {\n    b( {\n    }\n}\n", SpaceUnwanted(ParenOpen, BraceOpen), 2, 7);
    // No space goes after `(`, or before `(` or `)`.
    examples.lays_out("a(b)(c) {\n}\n");
    examples.check();
}

/// The tokens that can come first of two on a line, once the braces are where they go, each
/// written as code writes it.
const FIRSTS: [(TokenKind, &str); 6] = [
    (Function, "function"),
    (Returns, "returns"),
    (Name, "main"),
    (TypeName, "Void"),
    (ParenOpen, "("),
    (ParenClose, ")"),
];

/// The tokens that can come second: the same ones, and `{`.
const SECONDS: [(TokenKind, &str); 7] = [
    (Function, "function"),
    (Returns, "returns"),
    (Name, "main"),
    (TypeName, "Void"),
    (ParenOpen, "("),
    (ParenClose, ")"),
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

/// Says whether a token of `kind` is a word, which runs into a word right after it.
fn is_word(kind: TokenKind) -> bool {
    matches!(kind, Function | Returns | Name | TypeName)
}

#[test]
fn every_gap() {
    let mut examples = Examples::default();
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
                    ParenOpen => ") {",
                    _ => " {",
                };
                let spaces = " ".repeat(gap);
                let source = format!("{first_text}{spaces}{second_text}{rest}\n}}\n");
                let after_first = u32::try_from(first_text.len()).expect("a short word") + 1;
                match gap.cmp(&wanted) {
                    Ordering::Equal => examples.lays_out(&source),
                    // The second token is where the space should be.
                    Ordering::Less => {
                        examples.refuses(&source, SpaceMissing(first, second), 1, after_first);
                    }
                    Ordering::Greater if wanted == 0 => {
                        examples.refuses(&source, SpaceUnwanted(first, second), 1, after_first);
                    }
                    Ordering::Greater => {
                        examples.refuses(&source, SpacesExtra(first, second), 1, after_first + 1);
                    }
                }
                cases_count += 1;
            }
        }
    }
    assert_eq!(cases_count, 6 * 7 * 3 - 4 * 4);
    examples.check();
}

#[test]
fn the_steps_run_in_order() {
    let mut examples = Examples::default();
    // The lines come first, so a problem with them wins over a problem with the braces, or
    // within a line, that comes earlier in the file.
    examples.refuses("}\na  {\nb\n", LineEndsWithoutBrace, 3, 2);
    examples.refuses("a  {\n}\n}\nb { c\n", BraceOpenNotLast, 4, 3);
    examples.refuses("}\n{\n", BraceOpenStartsLine, 2, 1);
    examples.refuses("a  {\n}\n{\n}\n", BraceOpenStartsLine, 3, 1);
    examples.refuses("    {\n}\n", BraceOpenStartsLine, 1, 5);
    // Then the braces, so a problem with them wins over one within a line, even earlier on the
    // same line.
    examples.refuses("a  {\n// x\n}\n}\n", BraceClosesNothing, 4, 1);
    examples.refuses("a  {\n", BraceNeverClosed, 1, 4);
    examples.refuses("    a {\n", BraceNeverClosed, 1, 7);
    examples.check();
}

#[test]
fn the_first_problem_wins() {
    let mut examples = Examples::default();
    // Within each step, the earliest problem in the file wins.
    examples.refuses("a b\n{ }\n", LineEndsWithoutBrace, 1, 4);
    examples.refuses("a {\n} b\nc\n", BraceCloseNotAlone, 2, 1);
    // On one line, the first token at fault. A `{` that starts its line and doesn't end it is
    // refused for not ending it.
    examples.refuses("{ }\n", BraceOpenNotLast, 1, 1);
    examples.refuses("{ a\n", BraceOpenNotLast, 1, 1);
    examples.refuses("{\n}\na\n", BraceOpenStartsLine, 1, 1);
    examples.refuses("a } {\n", BraceCloseNotAlone, 1, 3);
    examples.refuses("a } b\n", BraceCloseNotAlone, 1, 3);
    examples.refuses("} { a\n", BraceCloseNotAlone, 1, 1);
    // A `}` with nothing open always comes before any `{` left open, and of several, the first.
    examples.refuses("a {\n}\n}\nb {\n", BraceClosesNothing, 3, 1);
    examples.refuses("}\n}\n", BraceClosesNothing, 1, 1);
    // Indentation and spaces are checked in one pass, so whichever comes first wins.
    examples.refuses("a  {\n// x\n}\n", SpacesExtra(Name, BraceOpen), 1, 3);
    examples.refuses("a {\n// x\n    b  {\n    }\n}\n", IndentationNotDepth(1), 2, 1);
    examples.refuses("a {\n    b  {\n// x\n    }\n}\n", SpacesExtra(Name, BraceOpen), 2, 7);
    // On one line, its indentation comes before the spaces after it.
    examples.refuses("a {\n        b  {\n    }\n}\n", IndentationNotDepth(1), 2, 5);
    examples.refuses("a {\nb  {\n    }\n}\n", IndentationNotDepth(1), 2, 1);
    // And of two gaps on a line, the first.
    examples.refuses("function main ()  returns Void{\n}\n", SpaceUnwanted(Name, ParenOpen), 1, 14);
    examples.check();
}

/// Returns the lines of blocks nested `depth` deep, each opened by `({` and closed by its own `}`,
/// all laid out the one way. A `{` can't start a line, so `({` is the shortest line that opens a
/// block.
fn nested(depth: u32) -> Vec<String> {
    let mut lines: Vec<String> = (0..depth).map(|level| format!("{}({{", indent(level))).collect();
    lines.extend((0..depth).rev().map(|level| format!("{}}}", indent(level))));
    lines
}

/// Returns the lines of blocks opened `depth` deep, each line laid out the one way, then `line`,
/// then a `}` for every block still open, none of them indented. Nothing past the first problem
/// is looked at, so a source that's refused at `line` needn't indent them.
fn opened_then(depth: u32, line: &str) -> String {
    let mut lines: Vec<String> = (0..depth).map(|level| format!("{}({{", indent(level))).collect();
    lines.push(String::from(line));
    let still_open = depth + u32::from(line.ends_with('{'));
    lines.extend((0..still_open).map(|_| String::from("}")));
    source_of(&lines)
}

#[test]
fn the_largest_sources() {
    // Depths and line numbers go as far as the largest source lets them, so none of them can be
    // counted in anything smaller.
    let mut examples = Examples::default();
    let size = SOURCE_BYTES_MAX as usize;
    // The deepest nesting laid out the one way.
    assert!(source_of(&nested(63)).len() <= size);
    assert!(source_of(&nested(64)).len() > size);
    examples.lays_out(&source_of(&nested(63)));
    // The deepest a line can be refused for its indentation, too shallow, and too deep.
    assert!(opened_then(89, "({").len() <= size);
    assert!(opened_then(90, "({").len() > size);
    examples.refuses(&opened_then(89, "({"), IndentationNotDepth(89), 90, 1);
    let too_deep = |depth: u32| opened_then(depth, &format!("{}({{", indent(depth + 1)));
    assert!(too_deep(88).len() <= size);
    assert!(too_deep(89).len() > size);
    examples.refuses(&too_deep(88), IndentationNotDepth(88), 89, column_at(88));
    // Braces go deeper still when they're refused before any indentation is looked at.
    examples.refuses(&"({\n".repeat(size / 3), BraceNeverClosed, 1, 2);
    let half = format!("{}{}", "({\n".repeat(size / 5), "}\n".repeat(size / 5));
    examples.refuses(&half, IndentationNotDepth(1), 2, 1);
    // The most lines of code, opening and closing one block after another.
    examples.lays_out(&"({\n}\n".repeat(size / 5));
    let last_line = u32::try_from(size / 5 * 2 - 1).expect("a line number");
    let left_open = format!("{}({{\n", "({\n}\n".repeat(size / 5 - 1));
    examples.refuses(&left_open, BraceNeverClosed, last_line, 2);
    // The most lines of all: every one blank but the last, a `}` with nothing to close.
    let most_lines = format!("{}}}\n", "\n".repeat(size - 2));
    examples.refuses(&most_lines, BraceClosesNothing, SOURCE_BYTES_MAX - 1, 1);
    // The last lines the last steps can reach, after the most blank lines: the last line that
    // lays out, the last refused within its line, and the last refused for its braces.
    examples.lays_out(&format!("{}//\n", "\n".repeat(size - 3)));
    let last_gap = format!("{}a{{\n}}\n", "\n".repeat(size - 5));
    examples.refuses(&last_gap, SpaceMissing(Name, BraceOpen), SOURCE_BYTES_MAX - 4, 2);
    let last_comment = format!("{}    //\n", "\n".repeat(size - 7));
    examples.refuses(&last_comment, IndentationNotDepth(0), SOURCE_BYTES_MAX - 6, 1);
    let last_brace = format!("{}{{\n", "\n".repeat(size - 2));
    examples.refuses(&last_brace, BraceOpenStartsLine, SOURCE_BYTES_MAX - 1, 1);
    examples.check();
}

#[test]
fn the_widest_lines() {
    // Gaps, indentation and columns go as far as the largest source lets them, so none of them
    // can be counted in anything smaller. A byte counts up to 255, so widths just past it are
    // tried too.
    let mut examples = Examples::default();
    let size = SOURCE_BYTES_MAX as usize;
    // Gaps of two spaces, just past what a byte counts, and the widest.
    for spaces in [2, 256, 257, size - 5] {
        let source = format!("a{}{{\n}}\n", " ".repeat(spaces));
        examples.refuses(&source, SpacesExtra(Name, BraceOpen), 1, 3);
    }
    // Indentation of one level, just past what a byte counts, and the widest.
    for levels in [1, 64, 65, (SOURCE_BYTES_MAX - 4) / LEVEL_SPACES] {
        let source = format!("{}//\n", indent(levels));
        examples.refuses(&source, IndentationNotDepth(0), 1, 1);
    }
    // A line of code can fill the largest source, and its problems are as far right as a column
    // goes: just past the line's end.
    examples.lays_out(&format!("a{} {{\n}}\n", ")".repeat(size - 6)));
    let longest = format!("a{}\n", ")".repeat(size - 2));
    examples.refuses(&longest, LineEndsWithoutBrace, 1, SOURCE_BYTES_MAX);
    let spaced = format!("a{}  {{\n}}\n", ")".repeat(size - 7));
    examples.refuses(&spaced, SpacesExtra(ParenClose, BraceOpen), 1, SOURCE_BYTES_MAX - 4);
    // The most tokens that lay out: a `(` in every byte but the last four.
    examples.lays_out(&format!("{}{{\n}}\n", "(".repeat(size - 4)));
    examples.check();
}

// The oracle, which works from each line's text.

/// What a line holds, as the oracle sees it.
enum Holds<'source> {
    /// Nothing: the line is blank.
    Nothing,
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
    /// The holds says what's on the line.
    holds: Holds<'source>,
}

/// Returns the column of the byte at `index` on a line, where everything before it is ASCII.
fn column(index: usize) -> u32 {
    u32::try_from(index).expect("a line is short") + 1
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
    let holds = if code.is_empty() {
        Holds::Nothing
    } else if code.starts_with("//") {
        Holds::Comment(code)
    } else {
        Holds::Code(split(content, indentation))
    };
    Line { number, content, indentation, holds }
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
        tokens.push((index, &content[index..index + length]));
        index += length;
    }
    tokens
}

/// Returns the kind of the token written `text`.
fn kind_of(text: &str) -> TokenKind {
    match text {
        "function" => Function,
        "returns" => Returns,
        "(" => ParenOpen,
        ")" => ParenClose,
        "{" => BraceOpen,
        "}" => BraceClose,
        _ if text.starts_with(|character: char| character.is_ascii_uppercase()) => TypeName,
        _ => Name,
    }
}

/// Returns `texts`, the tokens of a line of code, written the one way: one space between two
/// tokens, except none after `(`, and none before `(` or `)`.
fn joined(texts: &[&str]) -> String {
    let mut line = String::new();
    for (index, text) in texts.iter().enumerate() {
        if index > 0 && texts[index - 1] != "(" && !matches!(*text, "(" | ")") {
            line.push(' ');
        }
        line.push_str(text);
    }
    line
}

/// Returns what the tokens lint should make of `source`, which passed the text lint and the
/// lexer, worked out the way the layout page words it: nothing, or its first problem.
fn oracle(source: &str) -> Result<(), Refusal> {
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
        let Holds::Code(_) = line.holds else {
            continue;
        };
        let code = &line.content[line.indentation..];
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
        let Holds::Code(tokens) = &line.holds else {
            continue;
        };
        for &(index, text) in tokens {
            if text == "{" {
                open.push((line.number, column(index)));
            } else if text == "}" && open.pop().is_none() {
                return Err((BraceClosesNothing, line.number, column(index)));
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
        let (level, written) = match &line.holds {
            Holds::Nothing => continue,
            Holds::Comment(comment) => (depth, String::from(*comment)),
            Holds::Code(tokens) => {
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
    let Holds::Code(tokens) = &line.holds else {
        panic!("a comment is the same text, however it's indented");
    };
    let second = tokens.iter().position(|&(start, _)| start >= index).expect("a token follows");
    assert!(second > 0, "the first token is placed by the indentation");
    let pair = (kind_of(tokens[second - 1].1), kind_of(tokens[second].1));
    let kind = match (bytes[index], wanted_bytes[index]) {
        (b' ', _) if bytes[index - 1] == b' ' => SpacesExtra(pair.0, pair.1),
        (b' ', _) => SpaceUnwanted(pair.0, pair.1),
        (_, b' ') => SpaceMissing(pair.0, pair.1),
        _ => panic!("the same tokens differ only in their spaces"),
    };
    Err((kind, line.number, column(index)))
}

// Fuzzing against the oracle, seeded so a failing case replays.

/// This is a small deterministic generator, xorshift64*, as the lexer's tests use.
struct Rng {
    state: u64,
}

impl Rng {
    /// Returns the next number in the sequence.
    fn next(&mut self) -> u64 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Returns a number from 0 up to, but not including, `count`.
    fn below(&mut self, count: usize) -> usize {
        assert!(count > 0);
        usize::try_from(self.next() % count as u64).expect("below a usize")
    }

    /// Returns one of `choices`.
    fn pick<'a>(&mut self, choices: &[&'a str]) -> &'a str {
        choices[self.below(choices.len())]
    }
}

/// These are what a random line of code is made of, before the `{` it ends with.
const PIECES: &[&str] = &["function", "returns", "main", "Void", "x", "MyType", "(", ")", "(", ")"];

/// These follow a comment's `//`.
const COMMENT_TEXTS: &[&str] = &["", " a", " café", " { } ( )", " function main() returns Void {"];

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
                let mut texts: Vec<&str> = (0..heads).map(|_| rng.pick(PIECES)).collect();
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
            lines[index].drain(..LEVEL_SPACES as usize);
        }
        10 if index + 1 < lines.len() => {
            let next = lines.remove(index + 1);
            lines[index] = format!("{line}{}{}", rng.pick(&["", " "]), next.trim_start());
        }
        11 if !spaces.is_empty() => {
            let at = spaces[rng.below(spaces.len())];
            let levels = u32::try_from(rng.below(6)).expect("a few levels");
            lines[index] = String::from(&line[..at]);
            lines.insert(index + 1, format!("{}{}", indent(levels), &line[at + 1..]));
        }
        12 => {
            if let Some(at) = line.rfind(['{', '}']) {
                lines[index] = format!("{}{}", line[..at].trim_end(), &line[at + 1..]);
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
            lines[index] = format!("{}{{", &line[..indentation]);
        }
        _ => lines[index].push_str(rng.pick(&[" x", "(", ")", " {", "{", " }", "}"])),
    }
}

/// Returns the name of `kind`'s variant, without what it holds.
fn variant(kind: &Kind) -> String {
    let name = format!("{kind:?}");
    String::from(name.split('(').next().expect("a name"))
}

#[test]
fn fuzz_against_the_oracle() {
    let mut rng = Rng { state: 0x4C41_594F_5554_0001 };
    // Each source, and whether it was changed after it was laid out the one way.
    let mut sources = Vec::new();
    for _ in 0..40_000 {
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
        SpaceMissing(Name, Name),
        SpaceUnwanted(Name, Name),
        SpacesExtra(Name, Name),
    ];
    let mut kind_counts: Vec<(String, u32)> = kinds.iter().map(|kind| (variant(kind), 0)).collect();
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
                let name = variant(kind);
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
        assert_eq!(checked(source), expected, "for {}", shown(source));
    }
}
