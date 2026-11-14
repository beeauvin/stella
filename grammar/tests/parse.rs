// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the grammar. They're a crate of their own, so the compiler lets them reach only
//! what the crates make public: every source goes through the text lint's `lint`, the lexer's
//! `lex`, the tokens lint's `lint_tokens` and `nest`, and then `parse`.
//!
//! Every source here is laid out the one way, so it reaches the grammar, and each row says what
//! the grammar should make of it: the form of each node, or the problem and where it is. An oracle
//! works the same out from each line's text alone, by comparing a line of code outside every block
//! with a function's first line, token by token, and every row checks the oracle before `parse`
//! sees any. The tokens a line can hold are few, so every line of them, with a `{` and without,
//! up to a length that settles every case is tried, and a seeded fuzz test checks random programs
//! against the oracle. The largest sources sit at each limit a source of 16,384 bytes has.

#![forbid(unsafe_code)]

use std::fmt::Debug;
use std::ptr;

use stella_blocks::{Blocks, nest};
use stella_grammar::Form::{Comment, Function};
use stella_grammar::Kind::{CodeInBody, Expected, NotAFunction};
use stella_grammar::Part::{
    BraceOpen, Name, ParenthesisClose, ParenthesisOpen, ReturnType, Returns,
};
use stella_grammar::{
    FUNCTION_NAME_TOKEN_INDEX, FUNCTION_RETURN_TYPE_TOKEN_INDEX_FROM_END, Form, Forms, Kind, Part,
    Problem, parse,
};
use stella_lexer::{TokenKind, lex};
use stella_linter_text::lint;
use stella_linter_tokens::lint_tokens;
use stella_source::{Position, SOURCE_BYTES_MAX, u32_from_usize, usize_from_u32};
use stella_test_support::{
    Example, Examples, Rng, excerpt, indent, joined, sequences, slice, source_of,
};

/// What the grammar makes of a source: the form of each node, or the first problem.
type Outcome = Result<Vec<Form>, Problem>;

/// An expected refusal: the problem's kind, and its line and column.
type Refusal = (Kind, u32, u32);

/// This is the smallest program, laid out the one way.
const SMALLEST: &str = "function main() returns Void {\n}\n";

/// This is the shortest function, laid out the one way.
const SHORTEST_FUNCTION: &str = "function a() returns A {\n}\n";

/// One spelling for each kind of token that can come before a `{` on its line.
const SPELLINGS: [&str; 6] = ["function", "returns", "main", "Void", "(", ")"];

/// A function's first line, as the kinds of its tokens.
const RULE: [TokenKind; 7] = [
    TokenKind::Function,
    TokenKind::Name,
    TokenKind::ParenthesisOpen,
    TokenKind::ParenthesisClose,
    TokenKind::Returns,
    TokenKind::TypeName,
    TokenKind::BraceOpen,
];

/// The part of a function's first line each of its tokens after `function` is.
const PARTS: [Part; 6] = [Name, ParenthesisOpen, ParenthesisClose, Returns, ReturnType, BraceOpen];

/// These are a function's names.
const NAMES: &[&str] = &["main", "a", "x_2", "my_function"];

/// These are a function's return types.
const TYPES: &[&str] = &["Void", "A", "MessageQueue", "Http"];

/// These are what a token put into a line at random is.
const PIECES: &[&str] = &["function", "returns", "main", "x", "Void", "MyType", "(", ")"];

/// These follow a comment's `//`. Some read like code, which a comment never is.
const COMMENT_TEXTS: &[&str] = &["", " a", " café", " { } ( )", " }", " function main() {"];

/// Says whether `source` passes the text lint, the lexer and the tokens lint, so it reaches the
/// grammar. Nesting can't refuse, so a source laid out the one way always does.
fn lays_out(source: &str) -> bool {
    let Ok(text) = lint(source.as_bytes()) else {
        return false;
    };
    lex(text).is_ok_and(|tokens| lint_tokens(tokens).is_ok())
}

/// Returns the blocks of `source`, which must be laid out the one way.
fn blocks_of(source: &str) -> Blocks<'_> {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    let tokens = lex(text).expect("source should lex");
    nest(lint_tokens(tokens).expect("source should be laid out the one way"))
}

/// Parses `source`, which must be laid out the one way, and returns what the grammar made of it.
/// On the way, it checks that what `parse` returns holds the very nodes `nest` made, with one
/// form for each.
fn parsed(source: &str) -> Outcome {
    let blocks = blocks_of(source);
    // The grammar takes the blocks whole, so the nodes it gives back are in the room they were
    // made in, as many as there were.
    let (given, given_count) = (blocks.nodes().as_ptr(), blocks.nodes().len());
    let proved = parse(blocks)?;
    let back = proved.blocks().nodes();
    assert!(ptr::eq(given, back.as_ptr()), "what's parsed holds the nodes `nest` made");
    assert_eq!(back.len(), given_count, "what's parsed holds every node `nest` made");
    let forms = proved.as_slice();
    assert_eq!(forms.len(), back.len(), "each node has one form");
    Ok(forms.to_vec())
}

/// Returns the problem a refusal expects.
const fn problem((kind, line, column): Refusal) -> Problem {
    Problem { kind, position: Position::on_line(line).at_column(column) }
}

/// Adds `source` to `examples`. It must be laid out the one way, and its nodes take `forms`.
#[track_caller]
fn accepts(examples: &mut Examples<Outcome>, source: &str, forms: &[Form]) {
    examples.add(source, Ok(forms.to_vec()));
}

/// Adds `source` to `examples`. It must be laid out the one way, and its first problem is
/// `refusal`.
#[track_caller]
fn refuses(examples: &mut Examples<Outcome>, source: &str, refusal: Refusal) {
    examples.add(source, Err(problem(refusal)));
}

/// Adds to `examples` the source of `line`, a line of code that opens a block, then the `}` that
/// closes it. Its first problem is the kind given, at the column given, on line 1.
#[track_caller]
fn refuses_line(examples: &mut Examples<Outcome>, line: &str, (kind, column): (Kind, u32)) {
    examples.add(&format!("{line}\n}}\n"), Err(problem((kind, 1, column))));
}

/// Checks that every example is laid out the one way, as the grammar needs, and that the oracle
/// agrees with it. Then checks every one against `parse`. So every example checks the oracle
/// before `parse` sees any, whatever `parse` does.
fn check(examples: Examples<Outcome>) {
    let examples = examples.take();
    for Example { source, expected, at } in &examples {
        let excerpt = excerpt(source);
        assert!(lays_out(source), "{at}: {excerpt} should be laid out the one way");
        assert_eq!(oracle(source), *expected, "{at}: the oracle, for {excerpt}");
    }
    for Example { source, expected, at } in &examples {
        assert_eq!(parsed(source), *expected, "{at}: for {}", excerpt(source));
    }
}

/// Returns the tokens of `code`, a line of code without its `indentation`, as each one's kind and
/// column: each of `(`, `)`, `{` and `}` is one, and a run of letters, digits and `_` is a word.
/// A word spelled as a keyword is that keyword, and any other is a name, or a type's name when it
/// starts with a capital.
fn tokens_in(code: &str, indentation: usize) -> Vec<(TokenKind, u32)> {
    let mut tokens = Vec::new();
    let bytes = code.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let column = u32_from_usize(indentation + index) + 1;
        let in_word = |byte: &&u8| byte.is_ascii_alphanumeric() || **byte == b'_';
        // A character that isn't in a word is one byte: a symbol, or a space between tokens.
        let length = bytes[index..].iter().take_while(in_word).count().max(1);
        let kind = match slice(code, index..index + length) {
            " " => None,
            "(" => Some(TokenKind::ParenthesisOpen),
            ")" => Some(TokenKind::ParenthesisClose),
            "{" => Some(TokenKind::BraceOpen),
            "}" => Some(TokenKind::BraceClose),
            "function" => Some(TokenKind::Function),
            "returns" => Some(TokenKind::Returns),
            word if word.starts_with(|first: char| first.is_ascii_uppercase()) => {
                Some(TokenKind::TypeName)
            }
            _ => Some(TokenKind::Name),
        };
        tokens.extend(kind.map(|kind| (kind, column)));
        index += length;
    }
    tokens
}

/// Returns what the grammar should make of `source`, which is laid out the one way, worked out
/// from each line's text. A blank line and a `}` aren't nodes, and a comment is a node in any
/// block. A line of code outside every block must be a function's first line, token for token,
/// and is at fault at the first token that isn't, or just past its end when it stops short of
/// the rule. Every line of code outside every block that
/// passes opens a function's body, so until the first problem, a line of code that's indented is
/// in one, and it's at fault at its first token.
fn oracle(source: &str) -> Outcome {
    let mut forms = Vec::new();
    for (content, number) in source.split_terminator('\n').zip(1_u32..) {
        let code = content.trim_start_matches(' ');
        if code.is_empty() || code == "}" {
            continue;
        }
        if code.starts_with("//") {
            forms.push(Comment);
            continue;
        }
        let indentation = content.len() - code.len();
        let tokens = tokens_in(code, indentation);
        let at = |kind: Kind, (_, column): (TokenKind, u32)| Err(problem((kind, number, column)));
        let first = tokens[0];
        if indentation > 0 {
            return at(CodeInBody, first);
        }
        if first.0 != TokenKind::Function {
            return at(NotAFunction, first);
        }
        for (index, &token) in tokens.iter().enumerate().skip(1) {
            if RULE.get(index) != Some(&token.0) {
                assert!(index <= PARTS.len(), "a `{{` ends its line, so no token is past the rule");
                return at(Expected(PARTS[index - 1]), token);
            }
        }
        // A line has no spaces at its end, so the column just past its last character is just
        // past its last token.
        if tokens.len() < RULE.len() {
            let end = (TokenKind::BraceOpen, u32_from_usize(content.len()) + 1);
            return at(Expected(PARTS[tokens.len() - 1]), end);
        }
        assert_eq!(tokens.len(), RULE.len(), "a line that matches the rule ends where it does");
        forms.push(Function);
    }
    Ok(forms)
}

/// Returns the line of `sequence`'s tokens, then `{`, written the one way.
fn line_of(sequence: &[usize]) -> String {
    let mut texts: Vec<&str> = sequence.iter().map(|&spelling| SPELLINGS[spelling]).collect();
    texts.push("{");
    joined(&texts)
}

/// Says whether `position` is just past the end of its line in `source`, where a part that's
/// missing is reported.
fn is_past_the_end(source: &str, position: Position) -> bool {
    let line = source.split_terminator('\n').nth(usize_from_u32(position.line()) - 1);
    let line = line.expect("a problem is on one of the source's lines");
    usize_from_u32(position.column()) == line.len() + 1
}

/// Returns the line of `sequence`'s tokens alone, with no `{`, written the one way.
fn open_line_of(sequence: &[usize]) -> String {
    let texts: Vec<&str> = sequence.iter().map(|&spelling| SPELLINGS[spelling]).collect();
    joined(&texts)
}

/// Adds one to the count for `counted`, which must have one.
fn count<T: PartialEq + Debug>(counts: &mut [(T, u32)], counted: &T) {
    let found = counts.iter_mut().find(|(each, _)| each == counted);
    assert!(found.is_some(), "{counted:?} has a count");
    let (_, count) = found.expect("it has a count, as just asserted");
    *count += 1;
}

/// Returns one to three random tokens, for a line that isn't a function's first.
fn random_pieces(rng: &mut Rng) -> Vec<&'static str> {
    let count = 1 + rng.below(3);
    std::iter::repeat_with(|| rng.pick(PIECES)).take(count).collect()
}

/// Now and then, puts a long tail of `(` on the end of `texts`, a line before its `{`, so lines
/// are sometimes wider than a byte can count.
fn long_tail(rng: &mut Rng, texts: &mut Vec<&'static str>) {
    if rng.below(50) == 0 {
        texts.extend(std::iter::repeat_n("(", 250 + rng.below(151)));
    }
}

/// Returns a function's first line before its `{`, with random names, and a quarter of the time,
/// one or two random changes: a token taken out, put in, replaced, or swapped with the next.
fn random_first_line(rng: &mut Rng) -> Vec<&'static str> {
    let mut texts = vec!["function", rng.pick(NAMES), "(", ")", "returns", rng.pick(TYPES)];
    let changes = if rng.below(4) == 0 { 1 + rng.below(2) } else { 0 };
    for _ in 0..changes {
        let index = rng.below(texts.len());
        match rng.below(4) {
            0 => {
                texts.remove(index);
            }
            1 => texts.insert(rng.below(texts.len() + 1), rng.pick(PIECES)),
            2 => texts[index] = rng.pick(PIECES),
            _ if index + 1 < texts.len() => texts.swap(index, index + 1),
            _ => texts.push(rng.pick(PIECES)),
        }
        if texts.is_empty() {
            texts.push(rng.pick(PIECES));
        }
    }
    long_tail(rng, &mut texts);
    texts
}

/// Adds a random function to `lines`: its first line, as `random_first_line` makes it, then up to
/// three lines in its body, and its `}`. A line in its body is a comment, or now and then a blank
/// line, or a line of code that opens a block of its own.
fn random_function(rng: &mut Rng, lines: &mut Vec<String>) {
    let mut texts = random_first_line(rng);
    // Now and then the line ends before its `{`, and so opens no body.
    if rng.below(8) == 0 {
        lines.push(joined(&texts));
        return;
    }
    texts.push("{");
    lines.push(joined(&texts));
    for _ in 0..rng.below(4) {
        match rng.below(8) {
            0 => lines.push(String::new()),
            1 if rng.below(3) == 0 => {
                let texts = random_pieces(rng);
                lines.push(format!("{}{}", indent(1), joined(&texts)));
            }
            1 => {
                let mut texts = random_pieces(rng);
                long_tail(rng, &mut texts);
                texts.push("{");
                lines.push(format!("{}{}", indent(1), joined(&texts)));
                if rng.below(2) == 0 {
                    lines.push(format!("{}// deeper", indent(2)));
                }
                lines.push(format!("{}}}", indent(1)));
            }
            _ => lines.push(format!("{}//{}", indent(1), rng.pick(COMMENT_TEXTS))),
        }
    }
    lines.push(String::from("}"));
}

/// Returns a random program laid out the one way, as its lines: functions with comments in their
/// bodies and sometimes code, comments and blank lines outside them, and sometimes a line outside
/// every block that isn't a function's.
fn random_program(rng: &mut Rng) -> Vec<String> {
    let mut lines = Vec::new();
    for _ in 0..=rng.below(5) {
        match rng.below(10) {
            0 => lines.push(String::new()),
            1 | 2 => lines.push(format!("//{}", rng.pick(COMMENT_TEXTS))),
            3 if rng.below(3) == 0 => lines.push(joined(&random_pieces(rng))),
            3 => {
                let mut texts = random_pieces(rng);
                texts.push("{");
                lines.push(joined(&texts));
                lines.push(String::from("}"));
            }
            _ => random_function(rng, &mut lines),
        }
    }
    // A source can't end with a blank line, or be empty.
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push(String::from("// empty"));
    }
    lines
}

#[test]
fn the_smallest_program() {
    // The smallest program is one node, a function's first line.
    let mut examples = Examples::new();
    accepts(&mut examples, SMALLEST, &[Function]);
    check(examples);
}

#[test]
fn comments() {
    // A comment is allowed in every block and outside them, whatever its text reads like, and
    // each one is a node whose form is a comment.
    let mut examples = Examples::new();
    // A comment is allowed outside every block, so a source of only comments passes the grammar.
    // Whether it's a program, with a `main`, is for a later step.
    accepts(&mut examples, "// only\n", &[Comment]);
    accepts(&mut examples, "// a\n\n// b\n", &[Comment, Comment]);
    // A comment that reads like code is still a comment.
    accepts(&mut examples, "// function main {\n", &[Comment]);
    accepts(&mut examples, "// Void {\n", &[Comment]);
    // Comments in a function's body, before and after it, and with blank lines between.
    accepts(
        &mut examples,
        "// a\n\nfunction main() returns Void {\n    // b\n\n    // c\n}\n// d\n",
        &[Comment, Function, Comment, Comment, Comment],
    );
    accepts(
        &mut examples,
        "function main() returns Void {\n    // function inner() returns Void {\n}\n",
        &[Function, Comment],
    );
    // Comments between functions, and in each.
    accepts(
        &mut examples,
        "function a() returns A {\n    // in a\n}\n// between\nfunction b() returns B {\n    // \
         in b\n}\n",
        &[Function, Comment, Comment, Function, Comment],
    );
    check(examples);
}

#[test]
fn many_functions() {
    // The grammar checks only the order of tokens, so any function's first line in that order
    // passes, whatever its names, and however many there are. Which names and how many functions
    // a program may have is for a later step.
    let mut examples = Examples::new();
    accepts(&mut examples, SHORTEST_FUNCTION, &[Function]);
    accepts(&mut examples, "function x_2() returns MessageQueue {\n}\n", &[Function]);
    accepts(&mut examples, "function main() returns Http {\n}\n", &[Function]);
    accepts(
        &mut examples,
        "function a() returns A {\n}\nfunction b2() returns MessageQueue {\n}\nfunction main() \
         returns Void {\n}\n",
        &[Function, Function, Function],
    );
    accepts(&mut examples, &SMALLEST.repeat(2), &[Function, Function]);
    check(examples);
}

#[test]
fn function_lines() {
    // A line of code outside every block is a function's first line, part by part. Each row has
    // some other token where one part goes, and is refused for that part, at that token.
    let mut examples = Examples::new();
    // Outside every block, a line of code must start with `function`.
    refuses_line(&mut examples, "main() returns Void {", (NotAFunction, 1));
    refuses_line(&mut examples, "Void {", (NotAFunction, 1));
    refuses_line(&mut examples, "returns Void {", (NotAFunction, 1));
    refuses_line(&mut examples, "({", (NotAFunction, 1));
    refuses_line(&mut examples, ")() {", (NotAFunction, 1));
    refuses_line(&mut examples, "main main {", (NotAFunction, 1));
    // A function's name, a name that starts with a lowercase letter, comes after `function`. A
    // keyword is never a name.
    refuses_line(&mut examples, "function function() returns Void {", (Expected(Name), 10));
    refuses_line(&mut examples, "function Main() returns Void {", (Expected(Name), 10));
    refuses_line(&mut examples, "function returns Void {", (Expected(Name), 10));
    refuses_line(&mut examples, "function() returns Void {", (Expected(Name), 9));
    refuses_line(&mut examples, "function {", (Expected(Name), 10));
    // Then `(`.
    refuses_line(&mut examples, "function main {", (Expected(ParenthesisOpen), 15));
    refuses_line(
        &mut examples,
        "function main main() returns Void {",
        (Expected(ParenthesisOpen), 15),
    );
    refuses_line(&mut examples, "function main) returns Void {", (Expected(ParenthesisOpen), 14));
    refuses_line(&mut examples, "function main returns Void {", (Expected(ParenthesisOpen), 15));
    // Then `)`, since a function takes no parameters, for now.
    refuses_line(
        &mut examples,
        "function main(x) returns Void {",
        (Expected(ParenthesisClose), 15),
    );
    refuses_line(
        &mut examples,
        "function main(Void) returns Void {",
        (Expected(ParenthesisClose), 15),
    );
    refuses_line(
        &mut examples,
        "function main(() returns Void {",
        (Expected(ParenthesisClose), 15),
    );
    refuses_line(&mut examples, "function main({", (Expected(ParenthesisClose), 15));
    // Then `returns`: a function must have a return type.
    refuses_line(&mut examples, "function main() {", (Expected(Returns), 17));
    refuses_line(&mut examples, "function main() Void {", (Expected(Returns), 17));
    refuses_line(&mut examples, "function main()() returns Void {", (Expected(Returns), 16));
    refuses_line(&mut examples, "function main() main {", (Expected(Returns), 17));
    // Then the return type, a type's name.
    refuses_line(&mut examples, "function main() returns {", (Expected(ReturnType), 25));
    refuses_line(&mut examples, "function main() returns void {", (Expected(ReturnType), 25));
    refuses_line(&mut examples, "function main() returns returns {", (Expected(ReturnType), 25));
    refuses_line(&mut examples, "function main() returns() {", (Expected(ReturnType), 24));
    // Then the `{` that opens its body, and nothing more.
    refuses_line(&mut examples, "function main() returns Void Void {", (Expected(BraceOpen), 30));
    refuses_line(&mut examples, "function main() returns Void() {", (Expected(BraceOpen), 29));
    refuses_line(&mut examples, "function main() returns Void main {", (Expected(BraceOpen), 30));
    refuses_line(
        &mut examples,
        "function main() returns Void function {",
        (Expected(BraceOpen), 30),
    );
    // A line far down the file, after blank lines, which a byte can't count to.
    let far = format!("{}function main {{\n}}\n", "\n".repeat(300));
    refuses(&mut examples, &far, (Expected(ParenthesisOpen), 301, 15));
    let far = format!("{}main {{\n}}\n", "\n".repeat(256));
    refuses(&mut examples, &far, (NotAFunction, 257, 1));
    // After comments, and after a function that passes.
    refuses(&mut examples, "// a\nfunction main() {\n}\n", (Expected(Returns), 2, 17));
    refuses(&mut examples, "// a\n\n//\nVoid {\n}\n", (NotAFunction, 4, 1));
    let after = "function main() returns Void {\n}\nfunction x() {\n}\n";
    refuses(&mut examples, after, (Expected(Returns), 3, 14));
    check(examples);
}

#[test]
fn lines_that_end_early() {
    // How a line ends is the grammar's to check, so a line of code outside every block can stop
    // anywhere before its rule does, and is refused where it stops.
    let mut examples = Examples::new();
    // A line that ends before its `{` is refused for the first part it lacks, just past its last
    // token, however early it stops.
    refuses(&mut examples, "function\n", (Expected(Name), 1, 9));
    refuses(&mut examples, "function main\n", (Expected(ParenthesisOpen), 1, 14));
    refuses(&mut examples, "function main(\n", (Expected(ParenthesisClose), 1, 15));
    refuses(&mut examples, "function main()\n", (Expected(Returns), 1, 16));
    refuses(&mut examples, "function main() returns\n", (Expected(ReturnType), 1, 24));
    refuses(&mut examples, "function main() returns Void\n", (Expected(BraceOpen), 1, 29));
    // So a first line split anywhere is refused where it ends, and what's on the next line is
    // never reached.
    refuses(&mut examples, "function\nmain() returns Void {\n}\n", (Expected(Name), 1, 9));
    let split = "function main()\nreturns Void {\n}\n";
    refuses(&mut examples, split, (Expected(Returns), 1, 16));
    let split = "function main() returns\nVoid {\n}\n";
    refuses(&mut examples, split, (Expected(ReturnType), 1, 24));
    // A line that isn't a function's is refused for that, however it ends.
    refuses(&mut examples, "main\n", (NotAFunction, 1, 1));
    refuses(&mut examples, "(\n", (NotAFunction, 1, 1));
    refuses(&mut examples, "main()\n", (NotAFunction, 1, 1));
    // A token out of place is found before the line's end.
    refuses(&mut examples, "function main) returns Void\n", (Expected(ParenthesisOpen), 1, 14));
    check(examples);
}

#[test]
fn bodies() {
    // A function's body holds only comments, for now, so any line of code in it is refused at
    // its first token, whatever follows: a function's first line, or anything else.
    let mut examples = Examples::new();
    let body = |lines: &str| format!("function main() returns Void {{\n{lines}}}\n");
    let inner = body("    function inner() returns Void {\n    }\n");
    refuses(&mut examples, &inner, (CodeInBody, 2, 5));
    refuses(&mut examples, &body("    main {\n    }\n"), (CodeInBody, 2, 5));
    refuses(&mut examples, &body("    function main {\n    }\n"), (CodeInBody, 2, 5));
    refuses(&mut examples, &body("    ({\n    }\n"), (CodeInBody, 2, 5));
    // A line of code that opens no block, too.
    refuses(&mut examples, &body("    main()\n"), (CodeInBody, 2, 5));
    refuses(&mut examples, &body("    function main() returns Void\n"), (CodeInBody, 2, 5));
    // After comments and blank lines in the body.
    let after = body("    // a\n\n    // b\n    Void {\n    }\n");
    refuses(&mut examples, &after, (CodeInBody, 5, 5));
    // A line of code in a function's body that holds nodes of its own. The nodes two blocks deep
    // are never reached, since the node whose block they're in is refused first.
    let nested = body("    x {\n        // y\n        z {\n        }\n    }\n");
    refuses(&mut examples, &nested, (CodeInBody, 2, 5));
    // In the second of two functions.
    let second = "function a() returns A {\n}\nfunction b() returns B {\n    // c\n    x {\n    \
                  }\n}\n";
    refuses(&mut examples, second, (CodeInBody, 5, 5));
    // As deep as the largest source lets blocks go under a function, 63 in all, and one more
    // doesn't fit: refused at the first block in its body.
    let deep = |levels: u32| {
        let mut deep: Vec<String> = vec![String::from("function main() returns Void {")];
        deep.extend((1..levels).map(|level| format!("{}({{", indent(level))));
        deep.extend((0..levels).rev().map(|level| format!("{}}}", indent(level))));
        source_of(&deep)
    };
    assert!(deep(63).len() <= usize_from_u32(SOURCE_BYTES_MAX));
    assert!(deep(64).len() > usize_from_u32(SOURCE_BYTES_MAX));
    refuses(&mut examples, &deep(63), (CodeInBody, 2, 5));
    check(examples);
}

#[test]
fn the_first_problem_wins() {
    // A source with more than one thing wrong is refused for the first: the earliest node at
    // fault, and on that node's line, the first token at fault.
    let mut examples = Examples::new();
    // Of two lines at fault, the earlier one is reported, whichever block each is in.
    refuses(&mut examples, "function main {\n}\nVoid {\n}\n", (Expected(ParenthesisOpen), 1, 15));
    refuses(&mut examples, "Void {\n}\nfunction main {\n}\n", (NotAFunction, 1, 1));
    let body_first = "function a() returns A {\n    b {\n    }\n}\nVoid {\n}\n";
    refuses(&mut examples, body_first, (CodeInBody, 2, 5));
    let body_second = "Void {\n}\nfunction a() returns A {\n    b {\n    }\n}\n";
    refuses(&mut examples, body_second, (NotAFunction, 1, 1));
    let line_first = "function a() {\n    b {\n    }\n}\n";
    refuses(&mut examples, line_first, (Expected(Returns), 1, 14));
    // Within a line, the first token at fault is reported.
    refuses_line(&mut examples, "function Main(x) returns void {", (Expected(Name), 10));
    refuses_line(
        &mut examples,
        "function main(x) returns void {",
        (Expected(ParenthesisClose), 15),
    );
    refuses_line(&mut examples, "function main() Void Void {", (Expected(Returns), 17));
    check(examples);
}

#[test]
fn the_largest_sources() {
    // Each limit goes as far as the largest source lets it, and one more doesn't fit. So nothing
    // the grammar counts can be counted in anything smaller.
    let mut examples = Examples::new();
    let size = usize_from_u32(SOURCE_BYTES_MAX);
    // The most nodes the grammar passes: a comment on every line, and every form is a comment.
    assert!("//\n".repeat(5_461).len() <= size);
    assert!("//\n".repeat(5_462).len() > size);
    accepts(&mut examples, &"//\n".repeat(5_461), &[Comment; 5_461]);
    // The most nodes there can be, a line of code of one character on every line, reach the
    // grammar, which refuses the first.
    let most = "a\n".repeat(size / 2);
    assert_eq!(most.len(), size, "the most nodes fill the largest source");
    refuses(&mut examples, &most, (NotAFunction, 1, 1));
    // A line of code that fills the largest source and ends before its rule does is refused
    // just past its end, at the furthest column a line has.
    let ended = format!("function {}\n", "a".repeat(size - 10));
    assert_eq!(ended.len(), size, "the line fills the largest source");
    refuses(&mut examples, &ended, (Expected(ParenthesisOpen), 1, SOURCE_BYTES_MAX));
    // The most functions.
    assert!(SHORTEST_FUNCTION.repeat(606).len() <= size);
    assert!(SHORTEST_FUNCTION.repeat(607).len() > size);
    accepts(&mut examples, &SHORTEST_FUNCTION.repeat(606), &[Function; 606]);
    // The most nodes with a function among them: the shortest function, last, after the most
    // comments.
    let function_after = |count: usize| format!("{}{SHORTEST_FUNCTION}", "//\n".repeat(count));
    assert!(function_after(5_452).len() <= size);
    assert!(function_after(5_453).len() > size);
    let mut forms = vec![Comment; 5_452];
    forms.push(Function);
    accepts(&mut examples, &function_after(5_452), &forms);
    // A line at fault after the most comments: `({`, the shortest line that opens a block.
    let refused_after = |count: usize| format!("{}({{\n}}\n", "//\n".repeat(count));
    assert!(refused_after(5_459).len() <= size);
    assert!(refused_after(5_460).len() > size);
    refuses(&mut examples, &refused_after(5_459), (NotAFunction, 5_460, 1));
    // A line of code in a body whose function's line is far down: past where a byte can count,
    // and after the most comments.
    let body_after = |count: usize| {
        format!("{}function a() returns A {{\n    ({{\n    }}\n}}\n", "//\n".repeat(count))
    };
    refuses(&mut examples, &body_after(300), (CodeInBody, 302, 5));
    assert!(body_after(5_448).len() <= size);
    assert!(body_after(5_449).len() > size);
    refuses(&mut examples, &body_after(5_448), (CodeInBody, 5_450, 5));
    // The last line any line at fault can be on, after the most blank lines, and a function's
    // first line far down.
    let last_line = |blanks: usize| format!("{}({{\n}}\n", "\n".repeat(blanks));
    assert!(last_line(size - 5).len() <= size);
    assert!(last_line(size - 4).len() > size);
    refuses(&mut examples, &last_line(size - 5), (NotAFunction, 16_380, 1));
    let far_function = format!("{}function main {{\n}}\n", "\n".repeat(size - 18));
    assert_eq!(far_function.len(), size);
    refuses(&mut examples, &far_function, (Expected(ParenthesisOpen), 16_367, 15));
    // The last line a body's code can be on.
    let last_in_body = |blanks: usize| {
        format!("function a() returns A {{\n{}    ({{\n    }}\n}}\n", "\n".repeat(blanks))
    };
    assert!(last_in_body(size - 40).len() <= size);
    assert!(last_in_body(size - 39).len() > size);
    refuses(&mut examples, &last_in_body(size - 40), (CodeInBody, 16_346, 5));
    check(examples);
}

#[test]
fn wide_lines() {
    // Lines as wide as a source lets them be, and at the widths a small count wraps at, so a
    // line's tokens, and the column of a fault, can't be counted in anything smaller.
    let mut examples = Examples::new();
    let size = usize_from_u32(SOURCE_BYTES_MAX);
    let fits = |source: String| {
        assert!(source.len() <= size, "{} fits", excerpt(&source));
        source
    };
    // Outside every block: a line of `(`, then its `{`. The widest leaves room for `{` and `}`.
    let widest = size - 4;
    assert!(format!("{}{{\n}}\n", "(".repeat(widest + 1)).len() > size);
    for opens in [255, 256, 257, 511, 512, 513, widest] {
        let line = format!("{}{{", "(".repeat(opens));
        refuses(&mut examples, &fits(format!("{line}\n}}\n")), (NotAFunction, 1, 1));
        let line = format!("function{}{{", "(".repeat(opens.min(widest - 8)));
        refuses(&mut examples, &fits(format!("{line}\n}}\n")), (Expected(Name), 1, 9));
    }
    // A function's first line that's right up to its type, then a tail of `(`, as long as fits.
    let tail_most = size - "function main() returns Void{\n}\n".len();
    for tail in [1, 2, 255, 256, 257, tail_most] {
        let line = format!("function main() returns Void{}{{", "(".repeat(tail));
        refuses(&mut examples, &fits(format!("{line}\n}}\n")), (Expected(BraceOpen), 1, 29));
    }
    assert!(
        format!("function main() returns Void{}{{\n}}\n", "(".repeat(tail_most + 1)).len() > size
    );
    // A line of code in a body, as wide as fits.
    let body = |opens: usize| {
        format!("function a() returns A {{\n    {}{{\n    }}\n}}\n", "(".repeat(opens))
    };
    let body_most = size - body(0).len();
    assert!(body(body_most + 1).len() > size);
    for opens in [1, 8, 255, 256, 257, body_most] {
        refuses(&mut examples, &fits(body(opens)), (CodeInBody, 2, 5));
    }
    // A fault at the widest column: after a function's name as long as fits.
    let named = |length: usize| format!("function {} {{\n}}\n", "a".repeat(length));
    let name_most = size - named(0).len();
    assert!(named(name_most + 1).len() > size);
    refuses(&mut examples, &named(name_most), (Expected(ParenthesisOpen), 1, 16_381));
    check(examples);
}

#[test]
fn where_a_functions_parts_are() {
    // The grammar says where a function's name and return type are on its first line, for
    // whoever reads them next. Stated by hand: the name is the second token, after `function`,
    // and the return type is the last but one, before `{`. In the smallest program those are
    // `main` and `Void`.
    assert_eq!(FUNCTION_NAME_TOKEN_INDEX, 1);
    assert_eq!(FUNCTION_RETURN_TYPE_TOKEN_INDEX_FROM_END, 1);
    let forms = parse(blocks_of(SMALLEST)).expect("the smallest program should parse");
    let line = forms.blocks().tokens_of(&forms.blocks().nodes()[0]);
    let tokens = forms.blocks().tokens();
    let name = &line[usize_from_u32(FUNCTION_NAME_TOKEN_INDEX)];
    let from_end = usize_from_u32(FUNCTION_RETURN_TYPE_TOKEN_INDEX_FROM_END);
    let return_type = &line[line.len() - 1 - from_end];
    assert_eq!((tokens.text(name), tokens.text(return_type)), ("main", "Void"));
}

#[test]
fn every_line() {
    // Every line of 1 to 7 tokens before its `{`, outside every block. A function's first line
    // has six tokens before its `{`, so a line any longer is at fault within its first seven
    // tokens, just where its first seven tokens are.
    let sources: Vec<String> = (1..=7)
        .flat_map(|length| sequences(SPELLINGS.len(), length))
        .map(|sequence| format!("{}\n}}\n", line_of(&sequence)))
        .collect();
    assert_eq!(sources.len(), 335_922);
    // How many lines the oracle finds at fault for each kind, or none, worked out by counting: a
    // line is at fault at its token `i` when the `i` before it are right and that one isn't.
    // Where a token other than `{` goes, any of the five other kinds, or the `{` itself, can be
    // there; where the `{` goes, any of the six. Any tail can follow, up to seven.
    let counted = [
        (None, 1_u32),
        (Some(NotAFunction), 279_935),
        (Some(Expected(Name)), 46_656),
        (Some(Expected(ParenthesisOpen)), 7_776),
        (Some(Expected(ParenthesisClose)), 1_296),
        (Some(Expected(Returns)), 216),
        (Some(Expected(ReturnType)), 36),
        (Some(Expected(BraceOpen)), 6),
    ];
    let mut found = counted.map(|(kind, _)| (kind, 0_u32));
    let mut expected = Vec::with_capacity(sources.len());
    for source in &sources {
        assert!(lays_out(source), "{} should be laid out the one way", excerpt(source));
        let outcome = oracle(source);
        count(&mut found, &outcome.as_ref().err().map(|problem| problem.kind));
        expected.push(outcome);
    }
    assert_eq!(found, counted, "what the oracle finds, by kind");
    for (source, expected) in sources.iter().zip(expected) {
        assert_eq!(parsed(source), expected, "for {}", excerpt(source));
    }
    // Every line of 1 to 6 tokens with no `{`, outside every block. Each token where it goes is
    // one of six kinds, and exactly one is right, so a line is at fault at its token `i` when
    // the `i` before it are right and that one isn't, in five ways for each tail. A line whose
    // every token is right stops short of the rule, and is refused for the next part, just past
    // its end.
    let open_sources: Vec<String> = (1..=6)
        .flat_map(|length| sequences(SPELLINGS.len(), length))
        .map(|sequence| format!("{}\n", open_line_of(&sequence)))
        .collect();
    assert_eq!(open_sources.len(), 55_986);
    let open_counted = [
        (None, 0_u32),
        (Some(NotAFunction), 46_655),
        (Some(Expected(Name)), 7_776),
        (Some(Expected(ParenthesisOpen)), 1_296),
        (Some(Expected(ParenthesisClose)), 216),
        (Some(Expected(Returns)), 36),
        (Some(Expected(ReturnType)), 6),
        (Some(Expected(BraceOpen)), 1),
    ];
    let mut open_found = open_counted.map(|(kind, _)| (kind, 0_u32));
    for source in &open_sources {
        assert!(lays_out(source), "{} should be laid out the one way", excerpt(source));
        let expected = oracle(source);
        count(&mut open_found, &expected.as_ref().err().map(|problem| problem.kind));
        assert_eq!(parsed(source), expected, "for {}", excerpt(source));
    }
    assert_eq!(open_found, open_counted, "what the oracle finds, by kind, with no `{{`");
    // Every line of 1 to 4 tokens before its `{`, in a function's body, is refused at its first
    // token. What follows it on the line can't matter.
    let mut body_count: u32 = 0;
    for sequence in (1..=4).flat_map(|length| sequences(SPELLINGS.len(), length)) {
        let source =
            format!("function main() returns Void {{\n    {}\n    }}\n}}\n", line_of(&sequence));
        assert!(lays_out(&source), "{} should be laid out the one way", excerpt(&source));
        let expected = Err(problem((CodeInBody, 2, 5)));
        assert_eq!(oracle(&source), expected, "the oracle, for {}", excerpt(&source));
        assert_eq!(parsed(&source), expected, "for {}", excerpt(&source));
        body_count += 1;
    }
    assert_eq!(body_count, 1_554);
}

#[test]
fn every_source_owns_its_forms() {
    // Sources parsed one after another, the most nodes and the most functions a source can hold
    // among them and a refused one between, with every one's forms kept while the next is
    // parsed. Each source's forms are still what the oracle says of it at the end, whatever was
    // parsed after it. Each source owns what was made for it, so this only fails if the grammar
    // starts keeping something between calls.
    let most_nodes = "//\n".repeat(5_461);
    let most_functions = SHORTEST_FUNCTION.repeat(606);
    let refused = "function main {\n}\n";
    let refusal = problem((Expected(ParenthesisOpen), 1, 15));
    let sources =
        [SMALLEST, &most_nodes, refused, "// x\n", &most_functions, refused, SMALLEST, &most_nodes];
    let mut kept: Vec<(&str, Forms<'_>)> = Vec::new();
    for source in sources {
        match parse(blocks_of(source)) {
            Ok(forms) => kept.push((source, forms)),
            Err(problem) => {
                assert_eq!((source, problem), (refused, refusal), "only that source is refused");
            }
        }
    }
    assert_eq!(kept.len(), 6, "every source but the two refused ones parsed");
    for (source, forms) in &kept {
        assert_eq!(Ok(forms.as_slice().to_vec()), oracle(source), "for {}", excerpt(source));
    }
}

#[test]
fn fuzz_against_the_oracle() {
    // 20,000 random programs laid out the one way, each parsed and checked against what the
    // oracle says of it. Every outcome is counted, so the test fails if the programs stop
    // reaching one.
    let mut rng = Rng::new(0x4752_414D_4D41_5201);
    let kind_floors = [
        (NotAFunction, 2_300_u32),
        (CodeInBody, 1_500),
        (Expected(Name), 700),
        (Expected(ParenthesisOpen), 550),
        (Expected(ParenthesisClose), 450),
        (Expected(Returns), 350),
        (Expected(ReturnType), 200),
        (Expected(BraceOpen), 300),
    ];
    let mut kind_counts = kind_floors.map(|(kind, _)| (kind, 0_u32));
    let (mut with_function_count, mut without_function_count): (u32, u32) = (0, 0);
    let (mut past_line_1_count, mut wide_count): (u32, u32) = (0, 0);
    let mut past_the_end_count: u32 = 0;
    let mut checks = Vec::new();
    for _ in 0_u32..20_000 {
        let source = source_of(&random_program(&mut rng));
        assert!(lays_out(&source), "the generator lays out {}", excerpt(&source));
        let expected = oracle(&source);
        match &expected {
            Ok(forms) if forms.contains(&Function) => with_function_count += 1,
            Ok(_) => without_function_count += 1,
            Err(problem) => {
                count(&mut kind_counts, &problem.kind);
                past_line_1_count += u32::from(problem.position.line() > 1);
                past_the_end_count += u32::from(is_past_the_end(&source, problem.position));
            }
        }
        wide_count += u32::from(source.contains(&"(".repeat(250)));
        checks.push((source, expected));
    }
    // The fuzzing only means something if the sources reach every outcome, often. This is checked
    // before anything is parsed, so it holds whatever `parse` does. Each floor is about half of
    // what the seed reaches.
    for (outcome, count, floor) in [
        ("passed, with a function", with_function_count, 2_700),
        ("passed, with no function", without_function_count, 800),
        ("had a problem past line 1", past_line_1_count, 3_900),
        ("had a line wider than a byte counts", wide_count, 400),
        ("were refused just past a line's end", past_the_end_count, 1_100),
    ] {
        assert!(count > floor, "only {count} {outcome}");
    }
    for ((kind, count), (_, floor)) in kind_counts.into_iter().zip(kind_floors) {
        assert!(count > floor, "only {count} were refused with {kind:?}");
    }
    // The oracle made these answers, so each goes straight to `parse`.
    for (source, expected) in checks {
        assert_eq!(parsed(&source), expected, "for {}", excerpt(&source));
    }
}
