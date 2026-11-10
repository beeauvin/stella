// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the lexer. They're a crate of their own, so the compiler lets them reach only
//! what the crates make public: every source goes through the text lint's `lint`, and then `lex`.
//!
//! Code holds only printable ASCII, so every one of those characters is tried against `role`, a
//! table worded the way the tokens page is: where a token starts, in a name, in a type's name,
//! where a line starts, after indentation, right after a symbol, and after a comment that isn't
//! ASCII. A name's shape is tried on every short word from a small alphabet. The rest are tables
//! of sources and the tokens or problem each should get. A seeded fuzz test checks random sources
//! against an oracle, which states the tokens and names pages as plainly as it can, using `role`
//! too, and every example checks the oracle as well. Every token any test sees is checked to be
//! the source's own text, at its own position, with only spaces and line ends between and around
//! them.

use std::cell::RefCell;

use stella_lexer::Kind::{NameNotSnakeCase, TypeNameNotPascalCase, Unexpected};
use stella_lexer::TokenKind::*;
use stella_lexer::{Kind, Problem, TOKENS_MAX, TokenBuffer, TokenKind, Tokens, lex};
use stella_linter_text::{Position, SOURCE_BYTES_MAX, lint};

/// An expected token: its kind, its line and column, and its text.
type Expected<'source> = (TokenKind, u32, u32, &'source str);

/// An expected refusal: the problem's kind, and its line and column.
type Refusal = (Kind, u32, u32);

thread_local! {
    /// Each test lexes every source it tries into this one buffer, as the command does for every
    /// program in a tree. So a source's tokens are only ever its own if each source replaces what
    /// the last one left.
    static BUFFER: RefCell<TokenBuffer> = RefCell::new(TokenBuffer::new());
}

/// Lints `source`, which must pass, and lexes it into the test's buffer, which must pass too.
/// Returns its tokens.
fn tokens(source: &str) -> Vec<Expected<'_>> {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    BUFFER.with_borrow_mut(|buffer| {
        let tokens = lex(text, buffer);
        let tokens = tokens.unwrap_or_else(|problem| panic!("{source:?} should lex: {problem:?}"));
        expected_of(source, &tokens)
    })
}

/// Returns `tokens`, which `source` lexed to, as expected tokens. Each token's start and length
/// must be where its text is in the source, and the text must be in place.
fn expected_of<'source>(
    source: &'source str,
    tokens: &Tokens<'source, '_>,
) -> Vec<Expected<'source>> {
    let found: Vec<Expected<'source>> = tokens
        .as_slice()
        .iter()
        .map(|token| {
            let text = tokens.text(token);
            let start = token.start as usize;
            assert_eq!(text.as_ptr(), source[start..].as_ptr(), "{text:?} starts at its start");
            assert_eq!(text.len(), token.length as usize, "{text:?} is its length");
            (token.kind, token.position.line, token.position.column, text)
        })
        .collect();
    assert_in_place(source, &found);
    found
}

/// Checks that each of `found` is a slice of `source` itself, at the line and column it gives,
/// and that they come in order without overlapping. Everything between them, and after the last,
/// is spaces and line ends, so nothing else was passed over. It walks the source once.
fn assert_in_place(source: &str, found: &[Expected<'_>]) {
    let (mut line, mut column, mut index) = (1_u32, 1_u32, 0_usize);
    for &(_, token_line, token_column, text) in found {
        assert!(!text.is_empty(), "a token is at least one character");
        assert!(!text.contains('\n'), "a token is on one line");
        let start = text.as_ptr().addr().checked_sub(source.as_ptr().addr());
        let start = start.expect("a token's text is in the source");
        assert!(start + text.len() <= source.len(), "a token's text is in the source");
        assert!(start >= index, "tokens come in order, and don't overlap");
        assert_passed_over(&source[index..start]);
        advance(&source[index..start], &mut line, &mut column);
        assert_eq!((token_line, token_column), (line, column), "for {text:?}");
        advance(text, &mut line, &mut column);
        index = start + text.len();
    }
    assert_passed_over(&source[index..]);
}

/// Checks that `passed`, which the lexer passed over, is only spaces and line ends.
fn assert_passed_over(passed: &str) {
    let only_spaces = passed.chars().all(|character| matches!(character, ' ' | '\n'));
    assert!(only_spaces, "only spaces and line ends are passed over, not {passed:?}");
}

/// Moves `line` and `column` past `passed`.
fn advance(passed: &str, line: &mut u32, column: &mut u32) {
    for character in passed.chars() {
        if character == '\n' {
            *line += 1;
            *column = 1;
        } else {
            *column += 1;
        }
    }
}

/// Checks that `source` lexes to exactly `expected`. The oracle must agree first, so every
/// example checks it too.
fn lexes(source: &str, expected: &[Expected<'_>]) {
    assert!(lint(source.as_bytes()).is_ok(), "{source:?} should pass the text lint");
    assert_eq!(oracle(source).as_deref(), Ok(expected), "the oracle, for {source:?}");
    assert_eq!(tokens(source), expected, "for {source:?}");
}

/// Lints `source`, which must pass, and then lexes it, which must refuse it with `kind` at `line`
/// and `column`. The oracle must agree first, so every example checks it too.
fn refuses(source: &str, kind: Kind, line: u32, column: u32) {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    assert_eq!(oracle(source), Err((kind, line, column)), "the oracle, for {source:?}");
    let lexed =
        BUFFER.with_borrow_mut(|buffer| lex(text, buffer).map(|tokens| tokens.as_slice().len()));
    let found = lexed.expect_err("source should be refused");
    assert_eq!(found, Problem { kind, position: Position { line, column } }, "for {source:?}");
}

/// Returns the tokens of the smallest program's first line, `function main() returns Void {`,
/// when it's line `line`.
fn first_line(line: u32) -> [Expected<'static>; 7] {
    [
        (Function, line, 1, "function"),
        (Name, line, 10, "main"),
        (ParenOpen, line, 14, "("),
        (ParenClose, line, 15, ")"),
        (Returns, line, 17, "returns"),
        (TypeName, line, 25, "Void"),
        (BraceOpen, line, 30, "{"),
    ]
}

#[test]
fn the_smallest_program() {
    lexes(
        "function main() returns Void {\n}\n",
        &[
            (Function, 1, 1, "function"),
            (Name, 1, 10, "main"),
            (ParenOpen, 1, 14, "("),
            (ParenClose, 1, 15, ")"),
            (Returns, 1, 17, "returns"),
            (TypeName, 1, 25, "Void"),
            (BraceOpen, 1, 30, "{"),
            (BraceClose, 2, 1, "}"),
        ],
    );
}

#[test]
fn comments() {
    // A comment is one token, from its `//` to the end of its line, wherever the line is.
    let mut around = vec![(Comment, 1, 1, "// above")];
    around.extend(first_line(2));
    around.extend([
        (Comment, 3, 5, "// inside"),
        (BraceClose, 4, 1, "}"),
        (Comment, 5, 1, "// below"),
    ]);
    lexes("// above\nfunction main() returns Void {\n    // inside\n}\n// below\n", &around);
    for (source, expected) in [
        ("//\n", &[(Comment, 1, 1, "//")][..]),
        ("    //\n", &[(Comment, 1, 5, "//")]),
        ("        // eight\n", &[(Comment, 1, 9, "// eight")]),
        ("            // twelve\n", &[(Comment, 1, 13, "// twelve")]),
        ("// a\n// b\n", &[(Comment, 1, 1, "// a"), (Comment, 2, 1, "// b")]),
        // Nothing in a comment is code, so none of it is a token of its own, or refused.
        ("// function main() {\n", &[(Comment, 1, 1, "// function main() {")]),
        ("// ; \" ` 9 _ / /// //! ~\n", &[(Comment, 1, 1, "// ; \" ` 9 _ / /// //! ~")]),
        ("// myName HTTP a__b\n", &[(Comment, 1, 1, "// myName HTTP a__b")]),
        // A comment can hold any text the text lint allows.
        ("// café, 日本語\n", &[(Comment, 1, 1, "// café, 日本語")]),
    ] {
        lexes(source, expected);
    }
    // A comment's characters aren't bytes, and the lines after it are still placed right.
    let mut after = first_line(1).to_vec();
    after.extend([(Comment, 2, 5, "// café 日本語"), (BraceClose, 3, 1, "}")]);
    lexes("function main() returns Void {\n    // café 日本語\n}\n", &after);
}

#[test]
fn words() {
    // A word is a letter, then any letters, digits and `_`. The longest run is one word, and it
    // ends where something that can't go on with it begins.
    lexes(
        "a(b)c\n",
        &[
            (Name, 1, 1, "a"),
            (ParenOpen, 1, 2, "("),
            (Name, 1, 3, "b"),
            (ParenClose, 1, 4, ")"),
            (Name, 1, 5, "c"),
        ],
    );
    lexes(
        "Ab{cd}\n",
        &[
            (TypeName, 1, 1, "Ab"),
            (BraceOpen, 1, 3, "{"),
            (Name, 1, 4, "cd"),
            (BraceClose, 1, 6, "}"),
        ],
    );
    lexes("ab Cd\n", &[(Name, 1, 1, "ab"), (TypeName, 1, 4, "Cd")]);
    // A word is on one line, so a line's end ends it.
    lexes("ab\ncd\n", &[(Name, 1, 1, "ab"), (Name, 2, 1, "cd")]);
    // A word that runs on is still one word, so its shape is checked whole: `myName` is one
    // word, refused, not `my` and then `Name`.
    refuses("myName\n", NameNotSnakeCase, 1, 3);
    // The longest word fills the largest source, and either shape can.
    let longest = "a".repeat(SOURCE_BYTES_MAX as usize - 1);
    lexes(&format!("{longest}\n"), &[(Name, 1, 1, &longest)]);
    let longest = format!("A{}", "a".repeat(SOURCE_BYTES_MAX as usize - 2));
    lexes(&format!("{longest}\n"), &[(TypeName, 1, 1, &longest)]);
}

#[test]
fn names() {
    // A name that starts with a lowercase letter is snake case: lowercase letters and digits,
    // with single underscores between them. A digit can go anywhere but first.
    for name in ["x", "main", "main2", "my_function", "a1", "x9y8", "a_1_b", "x_2", "rung_0"] {
        lexes(&format!("{name}\n"), &[(Name, 1, 1, name)]);
    }
    // A name that starts with a capital is a type's, and Pascal case: words that each start with
    // one capital, then lowercase letters and digits.
    for name in ["Z", "Void", "Ab", "Http", "MessageQueue", "Utf8", "Void2", "A1B"] {
        lexes(&format!("{name}\n"), &[(TypeName, 1, 1, name)]);
    }
    // Any other shape is refused, at the first character that breaks it.
    for (name, kind, column) in [
        // A capital in a name, as camel case has.
        ("myFunction", NameNotSnakeCase, 3),
        ("mainV", NameNotSnakeCase, 5),
        ("a_B", NameNotSnakeCase, 3),
        // Two underscores in a row, or one at the end.
        ("my__name", NameNotSnakeCase, 4),
        ("a__", NameNotSnakeCase, 3),
        ("name_", NameNotSnakeCase, 5),
        ("a_", NameNotSnakeCase, 2),
        // An underscore in a type's name.
        ("Http_Request", TypeNameNotPascalCase, 5),
        ("Void_", TypeNameNotPascalCase, 5),
        ("A_b", TypeNameNotPascalCase, 2),
        // Two capitals in a row in a type's name.
        ("VOID", TypeNameNotPascalCase, 2),
        ("AB", TypeNameNotPascalCase, 2),
        ("HTTPRequest", TypeNameNotPascalCase, 2),
        ("MyTYpe", TypeNameNotPascalCase, 4),
    ] {
        refuses(&format!("{name}\n"), kind, 1, column);
    }
    // A name is placed where it is, after the tokens before it.
    refuses("function myFunction() returns Void {\n}\n", NameNotSnakeCase, 1, 12);
    refuses("function main() returns VOID {\n}\n", TypeNameNotPascalCase, 1, 26);
}

#[test]
fn every_short_word() {
    // Every word of up to five characters from `a`, `A`, `0` and `_` that starts with a letter.
    // That's enough to make every way a name can keep or break its shape. Each is checked against
    // the oracle, which the tables above pin.
    let alphabet = ['a', 'A', '0', '_'];
    let outcomes = [Ok(Name), Ok(TypeName), Err(NameNotSnakeCase), Err(TypeNameNotPascalCase)];
    let mut outcome_counts = outcomes.map(|outcome| (outcome, 0_u32));
    let mut sources = Vec::new();
    for length in 1..=5_u32 {
        for number in 0..4_u32.pow(length) {
            let word: String = (0..length)
                .map(|place| alphabet[(number / 4_u32.pow(place) % 4) as usize])
                .collect();
            if word.starts_with(['a', 'A']) {
                sources.push(format!("{word}\n"));
            }
        }
    }
    assert_eq!(sources.len(), 2 * (1 + 4 + 16 + 64 + 256));
    let checks: Vec<(&str, Result<Vec<Expected<'_>>, Refusal>)> =
        sources.iter().map(|source| (source.as_str(), oracle(source))).collect();
    for (_, expected) in &checks {
        let outcome = expected.as_ref().map(|found| found[0].0).map_err(|&(kind, ..)| kind);
        count(&mut outcome_counts, &outcome);
    }
    // Every outcome comes up, often. This is checked before anything is lexed.
    for (outcome, count) in outcome_counts {
        assert!(count > 50, "only {count} were {outcome:?}");
    }
    for (source, expected) in checks {
        match expected {
            Ok(found) => lexes(source, &found),
            Err((kind, line, column)) => refuses(source, kind, line, column),
        }
    }
}

#[test]
fn keywords() {
    lexes("function\n", &[(Function, 1, 1, "function")]);
    lexes("returns\n", &[(Returns, 1, 1, "returns")]);
    // A keyword is a whole word, matched by case, so none of these is one. Each is a name of its
    // shape instead, or refused for its shape.
    for word in [
        "functions",
        "function2",
        "func",
        "fn",
        "fun",
        "return",
        "returning",
        "ret",
        "functionreturns",
        "main",
    ] {
        lexes(&format!("{word}\n"), &[(Name, 1, 1, word)]);
    }
    for word in ["Function", "Returns", "Return", "Void"] {
        lexes(&format!("{word}\n"), &[(TypeName, 1, 1, word)]);
    }
    for (word, kind, column) in [
        ("FUNCTION", TypeNameNotPascalCase, 2),
        ("RETURNS", TypeNameNotPascalCase, 2),
        ("function_", NameNotSnakeCase, 9),
        ("returns_", NameNotSnakeCase, 8),
        ("functionReturns", NameNotSnakeCase, 9),
    ] {
        refuses(&format!("{word}\n"), kind, 1, column);
    }
    // Keywords are reserved, so a keyword is never a name, even where a name would go.
    lexes(
        "function function() returns returns {\n}\n",
        &[
            (Function, 1, 1, "function"),
            (Function, 1, 10, "function"),
            (ParenOpen, 1, 18, "("),
            (ParenClose, 1, 19, ")"),
            (Returns, 1, 21, "returns"),
            (Returns, 1, 29, "returns"),
            (BraceOpen, 1, 37, "{"),
            (BraceClose, 2, 1, "}"),
        ],
    );
}

#[test]
fn spaces_and_lines() {
    // Spaces separate tokens and nothing more. Whether they're the right ones is the tokens
    // lint's to check.
    lexes(
        "function  main ( )returns Void{}\n",
        &[
            (Function, 1, 1, "function"),
            (Name, 1, 11, "main"),
            (ParenOpen, 1, 16, "("),
            (ParenClose, 1, 18, ")"),
            (Returns, 1, 19, "returns"),
            (TypeName, 1, 27, "Void"),
            (BraceOpen, 1, 31, "{"),
            (BraceClose, 1, 32, "}"),
        ],
    );
    lexes(
        "(){}\n",
        &[
            (ParenOpen, 1, 1, "("),
            (ParenClose, 1, 2, ")"),
            (BraceOpen, 1, 3, "{"),
            (BraceClose, 1, 4, "}"),
        ],
    );
    // Indentation isn't a token either.
    lexes("    main    (\n", &[(Name, 1, 5, "main"), (ParenOpen, 1, 13, "(")]);
    // A blank line has no tokens, but it's still counted.
    let mut spaced = first_line(3).to_vec();
    spaced.push((BraceClose, 5, 1, "}"));
    lexes("\n\nfunction main() returns Void {\n\n}\n", &spaced);
}

/// A role is what a character does in code, as the tokens and names pages say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// It starts a name, or goes on with any word.
    Lowercase,
    /// It starts a type's name, or goes on with one. In a name, it breaks the name's shape.
    Capital,
    /// It goes on with any word, but can't start a token.
    Digit,
    /// It goes on with a name, but can't start a token. In a type's name, it breaks the shape.
    Underscore,
    /// It's a token of its own.
    Symbol(TokenKind),
    /// It separates tokens.
    Space,
    /// It can't start a token, or be in one.
    Refused,
}

/// Returns the role `character` has in code, worked out the way the tokens page words it.
fn role(character: char) -> Role {
    assert!((' '..='~').contains(&character), "code holds only printable ASCII");
    match character {
        'a'..='z' => Role::Lowercase,
        'A'..='Z' => Role::Capital,
        '0'..='9' => Role::Digit,
        '_' => Role::Underscore,
        '(' => Role::Symbol(ParenOpen),
        ')' => Role::Symbol(ParenClose),
        '{' => Role::Symbol(BraceOpen),
        '}' => Role::Symbol(BraceClose),
        ' ' => Role::Space,
        _ => Role::Refused,
    }
}

#[test]
fn every_printable_character() {
    // Code holds only printable ASCII, so these are all the characters the lexer meets in code.
    // Each is tried where a token starts, after `a ` (`at_start`), in a name, after `a`
    // (`in_name`), and in a type's name, after `A` (`in_type`).
    for character in ' '..='~' {
        let at_start = format!("a {character}b\n");
        let in_name = format!("a{character}b\n");
        let in_type = format!("A{character}b\n");
        let (here, started) = (&in_name[1..2], &at_start[2..4]);
        let (named, typed) = (&in_name[..3], &in_type[..3]);
        match role(character) {
            Role::Lowercase => {
                lexes(&at_start, &[(Name, 1, 1, "a"), (Name, 1, 3, started)]);
                lexes(&in_name, &[(Name, 1, 1, named)]);
                lexes(&in_type, &[(TypeName, 1, 1, typed)]);
            }
            Role::Capital => {
                lexes(&at_start, &[(Name, 1, 1, "a"), (TypeName, 1, 3, started)]);
                refuses(&in_name, NameNotSnakeCase, 1, 2);
                refuses(&in_type, TypeNameNotPascalCase, 1, 2);
            }
            Role::Digit => {
                refuses(&at_start, Unexpected(character), 1, 3);
                lexes(&in_name, &[(Name, 1, 1, named)]);
                lexes(&in_type, &[(TypeName, 1, 1, typed)]);
            }
            Role::Underscore => {
                refuses(&at_start, Unexpected(character), 1, 3);
                lexes(&in_name, &[(Name, 1, 1, named)]);
                refuses(&in_type, TypeNameNotPascalCase, 1, 2);
            }
            Role::Symbol(kind) => {
                lexes(&at_start, &[(Name, 1, 1, "a"), (kind, 1, 3, here), (Name, 1, 4, "b")]);
                lexes(&in_name, &[(Name, 1, 1, "a"), (kind, 1, 2, here), (Name, 1, 3, "b")]);
                lexes(&in_type, &[(TypeName, 1, 1, "A"), (kind, 1, 2, here), (Name, 1, 3, "b")]);
            }
            Role::Space => {
                lexes(&at_start, &[(Name, 1, 1, "a"), (Name, 1, 4, "b")]);
                lexes(&in_name, &[(Name, 1, 1, "a"), (Name, 1, 3, "b")]);
                lexes(&in_type, &[(TypeName, 1, 1, "A"), (Name, 1, 3, "b")]);
            }
            Role::Refused => {
                refuses(&at_start, Unexpected(character), 1, 3);
                refuses(&in_name, Unexpected(character), 1, 2);
                refuses(&in_type, Unexpected(character), 1, 2);
            }
        }
    }
}

#[test]
fn every_printable_character_in_context() {
    // Each character again, where a line starts, after indentation, right after a symbol, and on
    // a line after a comment whose characters aren't all one byte.
    let comment = (Comment, 1, 1, "// café");
    for character in ' '..='~' {
        let start = format!("{character}b\n");
        let indented = format!("    {character}b\n");
        let after_symbol = format!("({character}b\n");
        let after_comment = format!("// café\na {character}b\n");
        let (here, word) = (&start[..1], &start[..2]);
        match role(character) {
            Role::Lowercase | Role::Capital => {
                let kind = if role(character) == Role::Capital { TypeName } else { Name };
                lexes(&start, &[(kind, 1, 1, word)]);
                lexes(&indented, &[(kind, 1, 5, word)]);
                lexes(&after_symbol, &[(ParenOpen, 1, 1, "("), (kind, 1, 2, word)]);
                lexes(&after_comment, &[comment, (Name, 2, 1, "a"), (kind, 2, 3, word)]);
            }
            Role::Digit | Role::Underscore | Role::Refused => {
                refuses(&start, Unexpected(character), 1, 1);
                refuses(&indented, Unexpected(character), 1, 5);
                refuses(&after_symbol, Unexpected(character), 1, 2);
                refuses(&after_comment, Unexpected(character), 2, 3);
            }
            Role::Symbol(kind) => {
                lexes(&start, &[(kind, 1, 1, here), (Name, 1, 2, "b")]);
                lexes(&indented, &[(kind, 1, 5, here), (Name, 1, 6, "b")]);
                let expected = [(ParenOpen, 1, 1, "("), (kind, 1, 2, here), (Name, 1, 3, "b")];
                lexes(&after_symbol, &expected);
                let expected = [comment, (Name, 2, 1, "a"), (kind, 2, 3, here), (Name, 2, 4, "b")];
                lexes(&after_comment, &expected);
            }
            Role::Space => {
                // A line can't start with one space, or five, so only these two pass the text
                // lint.
                lexes(&after_symbol, &[(ParenOpen, 1, 1, "("), (Name, 1, 3, "b")]);
                lexes(&after_comment, &[comment, (Name, 2, 1, "a"), (Name, 2, 4, "b")]);
            }
        }
    }
}

#[test]
fn the_first_problem_wins() {
    // Lexing stops at the first problem in the file.
    refuses("function main() returns Void { ; @\n}\n", Unexpected(';'), 1, 32);
    refuses("a;b\nc@d\n", Unexpected(';'), 1, 2);
    refuses("a\nc@d;\n", Unexpected('@'), 2, 2);
    // Comments and blank lines before it are passed over, and still counted.
    let source = "// ;\n\nfunction main() returns Void {\n    // @\n    #\n}\n";
    refuses(source, Unexpected('#'), 5, 5);
    // A word can hold a digit or `_`, but can't start with one.
    refuses("9lives\n", Unexpected('9'), 1, 1);
    refuses("_main\n", Unexpected('_'), 1, 1);
    refuses("main 2\n", Unexpected('2'), 1, 6);
    // Names aren't kebab, as file names are, so `-` can't be in a word.
    refuses("my-name\n", Unexpected('-'), 1, 3);
    refuses("function main() returns Void {\n    \"hello\"\n}\n", Unexpected('"'), 2, 5);
    // A lone `/` is code, since only `//` starts a comment.
    refuses("function main() returns Void { /\n}\n", Unexpected('/'), 1, 32);
    refuses("`\n", Unexpected('`'), 1, 1);
    // A name's shape is a problem like any other, and whichever comes first in the file wins.
    refuses("myName ;\n", NameNotSnakeCase, 1, 3);
    refuses("; myName\n", Unexpected(';'), 1, 1);
    refuses("my;Name\n", Unexpected(';'), 1, 3);
    refuses("Void_ myName\n", TypeNameNotPascalCase, 1, 5);
}

#[test]
fn the_most_tokens() {
    let size = SOURCE_BYTES_MAX as usize;
    // Every token is at least one character, so the largest source holds a token in every byte
    // but its last `\n`. That's the most a buffer has room for.
    assert_eq!(TOKENS_MAX, SOURCE_BYTES_MAX - 1);
    let one_line = format!("{}\n", "(".repeat(size - 1));
    let found = tokens(&one_line);
    assert_eq!(found.len(), TOKENS_MAX as usize);
    for (token, column) in found.iter().zip(1_u32..) {
        assert_eq!(*token, (ParenOpen, 1, column, "("));
    }
    // And it holds the most lines with a token on each.
    let many_lines = "}\n".repeat(size / 2);
    let found = tokens(&many_lines);
    assert_eq!(found.len(), size / 2);
    for (token, line) in found.iter().zip(1_u32..) {
        assert_eq!(*token, (BraceClose, line, 1, "}"));
    }
    // The longest comment is still one token.
    let comment = format!("// {}\n", "x".repeat(size - "// \n".len()));
    lexes(&comment, &[(Comment, 1, 1, &comment[..size - 1])]);
}

#[test]
fn one_buffer_for_every_source() {
    // One buffer serves source after source, as it does when every program in a tree is linted.
    // Each source's tokens are its own, whatever the buffer held before: the most it can hold, a
    // refused source's, or a few. And they're always in the same room, since the buffer is made
    // once and never grows.
    let mut buffer = TokenBuffer::new();
    let largest = format!("{}\n", "(".repeat(TOKENS_MAX as usize));
    let smallest = "function main() returns Void {\n}\n";
    let refused = "function main() returns Void { ;\n}\n";
    let mut smallest_tokens = first_line(1).to_vec();
    smallest_tokens.push((BraceClose, 2, 1, "}"));
    let refusal = Problem { kind: Unexpected(';'), position: Position { line: 1, column: 32 } };
    let mut room = None;
    for source in [smallest, &largest, refused, smallest, &largest, refused, &largest, smallest] {
        let text = lint(source.as_bytes()).expect("source should pass the text lint");
        let lexed = lex(text, &mut buffer);
        if let Ok(tokens) = &lexed {
            let here = tokens.as_slice().as_ptr();
            assert_eq!(*room.get_or_insert(here), here, "the tokens are in the same room");
        }
        match lexed {
            Ok(tokens) if source == smallest => {
                assert_eq!(expected_of(source, &tokens), smallest_tokens);
            }
            Ok(tokens) => {
                let found = expected_of(source, &tokens);
                assert_eq!(found.len(), TOKENS_MAX as usize);
                assert!(found.iter().all(|&(kind, line, ..)| (kind, line) == (ParenOpen, 1)));
            }
            Err(problem) => assert_eq!((source, problem), (refused, refusal)),
        }
    }
}

// Fuzzing against an oracle, seeded so a failing case replays.

/// Returns what the lexer should make of `source`, which passed the text lint, worked out the way
/// the tokens and names pages word it: its tokens, or its first problem.
fn oracle(source: &str) -> Result<Vec<Expected<'_>>, Refusal> {
    let mut found = Vec::new();
    for (content, line) in source.split_terminator('\n').zip(1_u32..) {
        let indentation = content.len() - content.trim_start_matches(' ').len();
        let column = |index: usize| u32::try_from(index).expect("a line is short") + 1;
        if content[indentation..].starts_with("//") {
            found.push((Comment, line, column(indentation), &content[indentation..]));
            continue;
        }
        // The text lint lets only ASCII into code, so a character is a byte.
        assert!(content.is_ascii());
        let characters: Vec<char> = content.chars().collect();
        let mut index = 0;
        while index < characters.len() {
            let first = role(characters[index]);
            match first {
                Role::Space => index += 1,
                Role::Symbol(kind) => {
                    found.push((kind, line, column(index), &content[index..=index]));
                    index += 1;
                }
                Role::Lowercase | Role::Capital => {
                    let mut end = index + 1;
                    while end < characters.len() && in_word(role(characters[end])) {
                        end += 1;
                    }
                    let word = &content[index..end];
                    if let Some((kind, at)) = shape_problem(word) {
                        return Err((kind, line, column(index + at)));
                    }
                    let kind = match word {
                        "function" => Function,
                        "returns" => Returns,
                        _ if first == Role::Capital => TypeName,
                        _ => Name,
                    };
                    found.push((kind, line, column(index), word));
                    index = end;
                }
                Role::Digit | Role::Underscore | Role::Refused => {
                    return Err((Unexpected(characters[index]), line, column(index)));
                }
            }
        }
    }
    Ok(found)
}

/// Says whether a character with this role can go on with a word.
fn in_word(role: Role) -> bool {
    matches!(role, Role::Lowercase | Role::Capital | Role::Digit | Role::Underscore)
}

/// Returns where `word`, which starts with a letter, first breaks its shape, worked out the way
/// the names page words it: the problem's kind and the character's index. It's `None` when the
/// word keeps its shape.
fn shape_problem(word: &str) -> Option<(Kind, usize)> {
    let bytes = word.as_bytes();
    let last = bytes.len() - 1;
    if bytes[0].is_ascii_lowercase() {
        // Snake case: lowercase letters and digits, with single underscores between them.
        let broken = (0..=last).find(|&index| match bytes[index] {
            b'A'..=b'Z' => true,
            b'_' => index == last || bytes[index - 1] == b'_',
            _ => false,
        });
        broken.map(|index| (NameNotSnakeCase, index))
    } else {
        // Pascal case: words that each start with one capital, then lowercase letters and digits.
        let broken = (0..=last).find(|&index| match bytes[index] {
            b'_' => true,
            b'A'..=b'Z' => index > 0 && bytes[index - 1].is_ascii_uppercase(),
            _ => false,
        });
        broken.map(|index| (TypeNameNotPascalCase, index))
    }
}

/// This is a small deterministic generator, xorshift64*, as the path lint's tests use.
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
    fn below(&mut self, count: u64) -> u64 {
        assert!(count > 0);
        self.next() % count
    }

    /// Returns one of `choices`.
    fn pick<'a>(&mut self, choices: &[&'a str]) -> &'a str {
        choices[self.below(choices.len() as u64) as usize]
    }
}

/// These are most of the pieces random code is made of: keywords, names and type names, a
/// capital that runs into the next piece, and the symbols.
const PIECES: &[&str] = &[
    "function",
    "returns",
    "main",
    "Void",
    "Function",
    "function2",
    "my_name",
    "x",
    "MyType",
    "Http",
    "A",
    "(",
    ")",
    "{",
    "}",
];

/// These go on with a word, but can't start a token, so they're pieces now and then.
const IN_WORD: &[&str] = &["9", "_"];

/// These can't start a token, or be in one, so they're pieces now and then too.
const REFUSED: &[&str] = &[";", "\"", "/", "#", "`", "-", "="];

/// These go between pieces: nothing, so the pieces run together, or spaces.
const GAPS: &[&str] = &["", " ", " ", "  "];

/// These start a line. Two spaces isn't a level, so the text lint refuses it.
const INDENTS: &[&str] =
    &["", "", "", "", "    ", "    ", "    ", "        ", "        ", "            ", "  "];

/// These follow a comment's `//`. The text lint refuses `//x`.
const COMMENT_TEXTS: &[&str] = &["", " a", " ; ( }", " café", " function main", "x"];

/// Returns a random source of one to six lines: blank lines, comments and lines of code. Some are
/// ones the text lint refuses, and they're skipped.
fn random_source(rng: &mut Rng) -> String {
    let mut source = String::new();
    for _ in 0..=rng.below(6) {
        match rng.below(6) {
            0 => {}
            1 => {
                source.push_str(rng.pick(INDENTS));
                source.push_str("//");
                source.push_str(rng.pick(COMMENT_TEXTS));
            }
            _ => {
                source.push_str(rng.pick(INDENTS));
                for index in 0..=rng.below(6) {
                    if index > 0 {
                        source.push_str(rng.pick(GAPS));
                    }
                    let piece = match rng.below(16) {
                        0 => String::from(rng.pick(REFUSED)),
                        1 => String::from(rng.pick(IN_WORD)),
                        2 | 3 => near_keyword(rng),
                        _ => String::from(rng.pick(PIECES)),
                    };
                    source.push_str(&piece);
                }
            }
        }
        source.push('\n');
    }
    source
}

/// Returns a keyword, or a word that's nearly one: cut short, run on, or with a letter in
/// another case.
fn near_keyword(rng: &mut Rng) -> String {
    let keyword = rng.pick(&["function", "returns"]);
    let length = keyword.len() as u64;
    match rng.below(4) {
        0 => String::from(keyword),
        1 => String::from(&keyword[..=rng.below(length - 1) as usize]),
        2 => format!("{keyword}{}", rng.pick(&["s", "2", "_", "x"])),
        _ => {
            let index = rng.below(length) as usize;
            let upper = keyword[index..=index].to_ascii_uppercase();
            format!("{}{upper}{}", &keyword[..index], &keyword[index + 1..])
        }
    }
}

/// Adds one to the count for `counted`, which must have one.
fn count<T: PartialEq + std::fmt::Debug>(counts: &mut [(T, u32)], counted: &T) {
    let (_, count) = counts
        .iter_mut()
        .find(|(each, _)| each == counted)
        .unwrap_or_else(|| panic!("{counted:?} is counted"));
    *count += 1;
}

#[test]
fn fuzz_against_the_oracle() {
    let mut rng = Rng { state: 0x4C45_5845_5200_0001 };
    let sources: Vec<String> = (0..30_000).map(|_| random_source(&mut rng)).collect();
    let mut skipped_count: u32 = 0;
    let kinds =
        [Function, Returns, Name, TypeName, ParenOpen, ParenClose, BraceOpen, BraceClose, Comment];
    let mut kind_counts = kinds.map(|kind| (kind, 0_u32));
    let refused = [';', '"', '/', '#', '`', '-', '=', '9', '_'].map(Unexpected);
    let mut refused_counts: Vec<(Kind, u32)> = refused
        .into_iter()
        .chain([NameNotSnakeCase, TypeNameNotPascalCase])
        .map(|kind| (kind, 0))
        .collect();
    let mut checks = Vec::new();
    for source in &sources {
        if lint(source.as_bytes()).is_err() {
            skipped_count += 1;
            continue;
        }
        let expected = oracle(source);
        match &expected {
            Ok(found) => found.iter().for_each(|(kind, ..)| count(&mut kind_counts, kind)),
            Err((kind, ..)) => count(&mut refused_counts, kind),
        }
        checks.push((source, expected));
    }
    // The fuzzing only means something if the sources reach every answer, often. This is checked
    // before anything is lexed, so it holds whatever the lexer does.
    let accepted_count = checks.iter().filter(|(_, expected)| expected.is_ok()).count();
    assert!(accepted_count > 4_500, "only {accepted_count} were accepted");
    assert!(skipped_count < 15_000, "{skipped_count} were refused by the text lint");
    for (kind, count) in kind_counts {
        assert!(count > 1_000, "only {count} tokens were {kind:?}");
    }
    for (kind, count) in refused_counts {
        assert!(count > 350, "only {count} were refused with {kind:?}");
    }
    for (source, expected) in checks {
        match expected {
            Ok(found) => lexes(source, &found),
            Err((kind, line, column)) => refuses(source, kind, line, column),
        }
    }
}
