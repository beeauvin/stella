// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the AST. They're a crate of their own, so the compiler lets them reach only what
//! the crates make public: every source goes through the text lint's `lint`, the lexer's `lex`,
//! the tokens lint's `lint_tokens`, `nest` and `parse`, and then `build`.
//!
//! `build` can't refuse, so every source here passes the grammar, and each row says which
//! functions the AST should hold: for each, the index of its first line in the nodes, the line
//! it's on, and its name and return type, which the one layout puts at columns the row doesn't
//! have to state. An oracle works the same out from the source's text alone, and every row checks
//! the oracle before `build` sees any. A function is read through its tokens, so
//! `every_token_of_the_smallest_program` also states by hand which tokens those are. Every program
//! of a few items, from the kinds of item a program has, is tried, and a seeded fuzz test checks
//! random programs against the oracle. The largest sources sit at each limit a source of 16,384
//! bytes has, so nothing can be counted in anything too small. A function or a token asked of an
//! AST it isn't from is a bug, and each way to ask is tried with one that would fit and one that
//! wouldn't.

#![forbid(unsafe_code)]

use std::ptr;

use stella_ast::{Ast, FUNCTIONS_MAX, Function, Statement, build};
use stella_blocks::nest;
use stella_grammar::{Form, parse};
use stella_lexer::TokenKind::{self, BraceClose, Integer, Name, Return, TypeName};
use stella_lexer::{Token, lex};
use stella_linter_text::lint;
use stella_linter_tokens::lint_tokens;
use stella_source::{Position, SOURCE_BYTES_MAX, u32_from_usize, usize_from_u32};
use stella_test_support::{Example, Examples, Rng, excerpt, indent, sequences, source_of};

/// One function a row expects, laid out the one way: the index of its first line in the nodes,
/// the number of the line it's on, its name and its return type.
type ExpectedFunction<'text> = (u32, u32, &'text str, &'text str);

/// This is the smallest program, laid out the one way.
const SMALLEST: &str = "function main() returns Void {\n}\n";

/// This is the shortest function there can be, 27 bytes.
const SHORTEST: &str = "function a() returns A {\n}\n";

/// A function after a comment. Its first line is the second node, and its name the third token.
const AFTER_A_COMMENT: &str = "// a\nfunction a() returns A {\n}\n";

/// Two functions. The second's first line is the second node too, but its name is the tenth
/// token.
const TWO_FUNCTIONS: &str = "function a() returns A {\n}\nfunction b() returns B {\n}\n";

/// These are the functions' names in every program of a few items, picked by where the function
/// is.
const NAMES: &[&str] = &["main", "a", "x_2", "my_function"];

/// These are their return types, picked the same way.
const TYPES: &[&str] = &["Void", "A", "MessageQueue", "Http"];

/// These are the kinds of item a program has outside every block.
const ITEMS: [Item; 3] = [Item::Comment, Item::EmptyFunction, Item::FunctionWithAComment];

/// These start a random name.
const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";

/// These start each word of a random type's name.
const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// These go on with a random name or type's name, after its start or a word's.
const LOWER_AND_DIGITS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// These follow a comment's `//`. Some read like a function's first line, which a comment never
/// is.
const COMMENT_TEXTS: &[&str] =
    &["", " a", " café", " { } ( )", " }", " function main() returns Void {"];

/// A function with its parts in the open, as the tests state and compare one.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PlainFunction {
    /// The index of the function's first line, in the source's nodes.
    node_index: u32,
    /// The number of the function's first line, in the source's lines.
    line: u32,
    /// The function's name.
    name: String,
    /// The column the name starts at.
    name_column: u32,
    /// The name of the type the function returns.
    return_type: String,
    /// The column the return type starts at.
    return_type_column: u32,
    /// The `return` lines in the function's body, in order.
    returns: Vec<PlainReturn>,
}

/// A `return` line with its parts in the open.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PlainReturn {
    /// The number of the line it's on.
    line: u32,
    /// The integer it gives back, as it's written.
    value: String,
    /// The column the integer starts at.
    value_column: u32,
}

/// An item is one thing a program holds outside every block.
#[derive(Clone, Copy, Debug)]
enum Item {
    /// A comment.
    Comment,
    /// A function with nothing in its body.
    EmptyFunction,
    /// A function with a comment in its body.
    FunctionWithAComment,
}

/// The shapes the fuzzing must reach often, counted over every program.
#[derive(Debug, Default)]
struct ShapeCounts {
    /// How many functions the programs hold.
    functions: u32,
    /// How many programs hold no function.
    programs_without_a_function: u32,
    /// How many programs hold three functions or more.
    programs_of_three_functions: u32,
    /// How many comments are in the functions' bodies.
    comments_in_bodies: u32,
    /// How many blank lines are in the functions' bodies.
    blank_lines_in_bodies: u32,
    /// How many `return` lines are in the functions' bodies.
    returns: u32,
    /// How many `return` lines have a comment after them in their body.
    commented_returns: u32,
    /// How many names are longer than a byte counts.
    long_names: u32,
    /// How many return types are longer than a byte counts.
    long_types: u32,
}

impl ShapeCounts {
    /// Counts the shapes the program of `lines`, whose AST should hold `functions`, reaches.
    fn count(&mut self, lines: &[String], functions: &[PlainFunction]) {
        self.functions += u32_from_usize(functions.len());
        self.programs_without_a_function += u32::from(functions.is_empty());
        self.programs_of_three_functions += u32::from(functions.len() >= 3);
        let (mut in_body, mut returned) = (false, false);
        for line in lines {
            if line.starts_with("function ") {
                (in_body, returned) = (true, false);
            } else if line == "}" {
                in_body = false;
            } else {
                let comment = in_body && line.starts_with("    //");
                self.comments_in_bodies += u32::from(comment);
                self.commented_returns += u32::from(comment && returned);
                self.blank_lines_in_bodies += u32::from(in_body && line.is_empty());
                returned = returned || line.starts_with("    return ");
            }
        }
        self.returns +=
            functions.iter().map(|function| u32_from_usize(function.returns.len())).sum::<u32>();
        let long = |text: &str| u32::from(text.len() > 255);
        self.long_names += functions.iter().map(|function| long(&function.name)).sum::<u32>();
        self.long_types +=
            functions.iter().map(|function| long(&function.return_type)).sum::<u32>();
    }
}

/// Says whether `source` passes the text lint, the lexer, the tokens lint and the grammar, so it
/// reaches the AST.
fn parses(source: &str) -> bool {
    let Ok(text) = lint(source.as_bytes()) else {
        return false;
    };
    let Ok(tokens) = lex(text) else {
        return false;
    };
    let Ok(layout) = lint_tokens(tokens) else {
        return false;
    };
    parse(nest(layout)).is_ok()
}

/// Returns the AST of `source`, which must pass the grammar. The AST must hold the very nodes
/// `nest` made and the very forms `parse` made: the same room, and as many.
fn ast_of(source: &str) -> Ast<'_> {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    let tokens = lex(text).expect("source should lex");
    let layout = lint_tokens(tokens).expect("source should be laid out the one way");
    let forms = parse(nest(layout)).expect("source should pass the grammar");
    let (nodes, each) = (forms.blocks().nodes(), forms.as_slice());
    let (given_nodes, given_nodes_count) = (nodes.as_ptr(), nodes.len());
    let (given_forms, given_forms_count) = (each.as_ptr(), each.len());
    let ast = build(forms);
    let (nodes, each) = (ast.forms().blocks().nodes(), ast.forms().as_slice());
    assert!(ptr::eq(given_nodes, nodes.as_ptr()), "the AST holds the nodes `nest` made");
    assert_eq!(nodes.len(), given_nodes_count, "the AST holds every node `nest` made");
    assert!(ptr::eq(given_forms, each.as_ptr()), "the AST holds the forms `parse` made");
    assert_eq!(each.len(), given_forms_count, "the AST holds every form `parse` made");
    ast
}

/// Returns `function`, one of `ast`'s, with its parts in the open. On the way, it checks that the
/// function's first line is a function's, outside every block, that its `function` is the first
/// token on that line, and that its name and return type are a name and a type's name on it too.
fn plain(ast: &Ast<'_>, function: &Function) -> PlainFunction {
    let (forms, node_index) = (ast.forms(), function.node_index());
    let node = forms.blocks().nodes().get(usize_from_u32(node_index));
    let node = node.expect("a function's first line is one of the nodes");
    let form = forms.as_slice()[usize_from_u32(node_index)];
    assert_eq!(form, Form::Function, "it's a function's first line");
    assert_eq!(node.parent_index(), None, "a function's first line is outside every block");
    let first = forms.blocks().tokens_of(node).first().expect("a node holds a token");
    let keyword = ast.keyword(function);
    assert!(ptr::eq(keyword, first), "a function's `function` starts its first line");
    assert_eq!(keyword.kind(), TokenKind::Function, "a function's `function` is the keyword");
    assert_eq!(ast.text(keyword), "function", "the keyword is written `function`");
    let line = keyword.position().line();
    assert_eq!(keyword.position().column(), 1, "outside every block, a line isn't indented");
    let (name, return_type) = (ast.name(function), ast.return_type(function));
    assert_eq!(name.kind(), Name, "a function's name is a name");
    assert_eq!(name.position().line(), line, "a function's name is on its first line");
    assert_eq!(return_type.kind(), TypeName, "a function's return type is a type's name");
    assert_eq!(return_type.position().line(), line, "and so is its return type");
    // The `}` that closes the body is the block's, alone on a later line, not indented.
    let close = ast.close(function);
    let block = node.block().expect("a function's first line opens its body");
    let close_index = usize_from_u32(block.close_token_index());
    let own_close = &forms.blocks().tokens().as_slice()[close_index];
    assert!(ptr::eq(close, own_close), "it's the very `}}` that closes the body");
    assert_eq!(close.kind(), BraceClose, "a body is closed by `}}`");
    assert!(close.position().line() > line, "a body's `}}` is on a later line");
    assert_eq!(close.position().column(), 1, "and isn't indented");
    let returns = ast.statements(function).iter().map(|statement| plain_return(ast, statement));
    PlainFunction {
        node_index,
        line,
        name: String::from(ast.text(name)),
        name_column: name.position().column(),
        return_type: String::from(ast.text(return_type)),
        return_type_column: return_type.position().column(),
        returns: returns.collect(),
    }
}

/// Returns `statement`, one of `ast`'s, with its parts in the open. On the way, it checks that
/// its `return` starts a line in a body, a level in, and that its value is the integer after it.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "KS-ASSERT-ARGS: the AST checks a statement is its own by where it is, which a copy \
              would lose"
)]
fn plain_return(ast: &Ast<'_>, statement: &Statement) -> PlainReturn {
    let (keyword, value) = (ast.keyword_of(statement), ast.value(statement));
    assert_eq!(keyword.kind(), Return, "a `return` line starts with `return`");
    assert_eq!(keyword.position().column(), 5, "a `return` line is a level in");
    assert_eq!(value.kind(), Integer, "its value is an integer");
    assert_eq!(value.position().line(), keyword.position().line(), "on the same line");
    assert_eq!(value.position().column(), 12, "right after `return `");
    PlainReturn {
        line: keyword.position().line(),
        value: String::from(ast.text(value)),
        value_column: value.position().column(),
    }
}

/// Returns `ast`'s functions with their parts in the open, in the order the AST holds them.
fn functions_of(ast: &Ast<'_>) -> Vec<PlainFunction> {
    ast.functions().iter().map(|function| plain(ast, function)).collect()
}

/// Builds the AST of `source`, which must pass the grammar, and returns the functions it holds.
fn built(source: &str) -> Vec<PlainFunction> {
    functions_of(&ast_of(source))
}

/// Returns `expected` with its parts in the open. The one layout puts the name at column 10,
/// after `function `, and the return type after the name and `() returns `.
fn laid_out(expected: ExpectedFunction<'_>) -> PlainFunction {
    let (node_index, line, name, return_type) = expected;
    let name_column = 10;
    let return_type_column = name_column + u32_from_usize(name.len() + "() returns ".len());
    let (name, return_type) = (String::from(name), String::from(return_type));
    let returns = Vec::new();
    PlainFunction { node_index, line, name, name_column, return_type, return_type_column, returns }
}

/// Returns each of `functions` with its parts in the open.
fn all_laid_out(functions: &[ExpectedFunction<'_>]) -> Vec<PlainFunction> {
    functions.iter().copied().map(laid_out).collect()
}

/// Adds `source` to `examples`. It must pass the grammar, and its AST must hold `functions`.
#[track_caller]
fn builds(
    examples: &mut Examples<Vec<PlainFunction>>,
    source: &str,
    functions: &[ExpectedFunction<'_>],
) {
    examples.add(source, all_laid_out(functions));
}

/// Checks that every example passes the grammar, as `build` needs, and that the oracle agrees
/// with it. Then checks every one against `build`. So every example checks the oracle before
/// `build` sees any, whatever `build` does.
fn check(examples: Examples<Vec<PlainFunction>>) {
    let examples = examples.take();
    for Example { source, expected, at } in &examples {
        let excerpt = excerpt(source);
        assert!(parses(source), "{at}: {excerpt} should pass the grammar");
        assert_eq!(oracle(source), *expected, "{at}: the oracle, for {excerpt}");
    }
    for Example { source, expected, at } in &examples {
        assert_eq!(built(source), *expected, "{at}: for {}", excerpt(source));
    }
}

/// Returns the functions `source`'s AST should hold, worked out from its text alone. `source`
/// must pass the grammar, so a line that starts with `function ` is a function's first line, laid
/// out the one way, one that starts with `    return ` is a `return` line in the body of the
/// function before it, and every other line that isn't blank or a `}` is a comment. Each of those
/// lines is a node. Columns are counted in bytes, since code is ASCII.
fn oracle(source: &str) -> Vec<PlainFunction> {
    let mut functions = Vec::new();
    let mut node_index: u32 = 0;
    for (content, line) in source.split_terminator('\n').zip(1_u32..) {
        let code = content.trim_start_matches(' ');
        if code.is_empty() || code == "}" {
            continue;
        }
        if let Some(value) = content.strip_prefix("    return ") {
            let function: &mut PlainFunction =
                functions.last_mut().expect("a `return` line is in a function's body");
            let value_column = u32_from_usize("    return ".len() + 1);
            let value = String::from(value);
            function.returns.push(PlainReturn { line, value, value_column });
        }
        if let Some(rest) = content.strip_prefix("function ") {
            let (name, rest) = rest.split_once("() returns ").expect("a function's first line");
            let return_type = rest.strip_suffix(" {").expect("a function's first line ends `{`");
            let return_type_start = content.len() - " {".len() - return_type.len();
            functions.push(PlainFunction {
                node_index,
                line,
                name: String::from(name),
                name_column: u32_from_usize("function ".len() + 1),
                return_type: String::from(return_type),
                return_type_column: u32_from_usize(return_type_start + 1),
                returns: Vec::new(),
            });
        }
        node_index += 1;
    }
    functions
}

/// Returns the index of `token` in `tokens`, which must hold the very token, and not only one
/// equal to it.
fn index_in(tokens: &[Token], token: &Token) -> usize {
    let index = tokens.iter().position(|own| ptr::eq(own, token));
    index.expect("the token is one of the tokens, in their own room")
}

/// Returns the lines of the program whose items outside every block are `items`, in that order.
fn program_of(items: &[Item]) -> Vec<String> {
    let mut lines = Vec::new();
    for (place, item) in items.iter().enumerate() {
        let (name, return_type) = (NAMES[place % NAMES.len()], TYPES[place % TYPES.len()]);
        let first_line = format!("function {name}() returns {return_type} {{");
        match item {
            Item::Comment => lines.push(format!("// {place}")),
            Item::EmptyFunction => lines.extend([first_line, String::from("}")]),
            Item::FunctionWithAComment => {
                let comment = format!("{}// in {name}", indent(1));
                lines.extend([first_line, comment, String::from("}")]);
            }
        }
    }
    lines
}

/// Hands `ask` the AST of `TWO_FUNCTIONS`, and a function from another source that would fit it.
/// The function's first line is the second node, as the second of the AST's own is, and its
/// tokens are the third and the seventh, which the AST's source has too. It still isn't one of
/// the AST's functions.
fn ask_of_a_function_that_would_fit(ask: impl FnOnce(&Ast<'_>, &Function)) {
    let other = ast_of(AFTER_A_COMMENT);
    let ast = ast_of(TWO_FUNCTIONS);
    let function = other.functions().first().expect("the other source has a function");
    let own = ast.functions().iter().find(|own| own.node_index() == function.node_index());
    assert!(own.is_some(), "one of the AST's own functions starts at the same node");
    assert_ne!(own, Some(function), "the two that start at the same node aren't the same");
    let tokens_count = |ast: &Ast<'_>| ast.forms().blocks().tokens().as_slice().len();
    assert!(tokens_count(&other) <= tokens_count(&ast), "the AST has a token wherever it has");
    ask(&ast, function);
}

/// Hands `ask` the AST of the smallest program, and the function of another AST of the very
/// same source. The two functions are equal, part for part, and each is only its own AST's.
fn ask_of_a_function_equal_to_its_own(ask: impl FnOnce(&Ast<'_>, &Function)) {
    let other = ast_of(SMALLEST);
    let ast = ast_of(SMALLEST);
    let function = other.functions().first().expect("the other source has a function");
    assert_eq!(ast.functions().first(), Some(function), "the two functions are equal");
    ask(&ast, function);
}

/// Hands `ask` the AST of the smallest program, and a function from a longer source that doesn't
/// fit it: the function's first line is the second node, and the AST's source has one node.
fn ask_of_a_function_from_a_longer_source(ask: impl FnOnce(&Ast<'_>, &Function)) {
    let longer = ast_of(TWO_FUNCTIONS);
    let ast = ast_of(SMALLEST);
    let function = longer.functions().last().expect("the longer source has functions");
    let nodes_count = ast.forms().blocks().nodes().len();
    assert!(usize_from_u32(function.node_index()) >= nodes_count, "the node is past the AST's");
    ask(&ast, function);
}

/// Returns `count` random characters from `alphabet`.
fn run(rng: &mut Rng, alphabet: &[u8], count: usize) -> String {
    std::iter::repeat_with(|| char::from(alphabet[rng.below(alphabet.len())])).take(count).collect()
}

/// Returns how many words a random name or type's name has: a few, and now and then, a lot.
fn random_words_count(rng: &mut Rng) -> usize {
    if rng.below(40) == 0 { 60 + rng.below(40) } else { 1 + rng.below(3) }
}

/// Returns a random name in snake case, and now and then a long one: a lowercase letter, then
/// words of lowercase letters and digits joined by single `_`. It's never a keyword.
fn random_name(rng: &mut Rng) -> String {
    let words_count = random_words_count(rng);
    let mut name = run(rng, LOWER, 1);
    for word in 0..words_count {
        if word > 0 {
            name.push('_');
        }
        let length = 1 + rng.below(5);
        // The first word already has its first letter.
        name.push_str(&run(rng, LOWER_AND_DIGITS, if word == 0 { length - 1 } else { length }));
    }
    if matches!(name.as_str(), "function" | "returns") {
        name.push('s');
    }
    name
}

/// Returns a random type's name in Pascal case, and now and then a long one: words that each
/// start with one capital, then lowercase letters and digits. Every word but the last has at
/// least one after its capital, so two capitals never come in a row.
fn random_type(rng: &mut Rng) -> String {
    let words_count = random_words_count(rng);
    let mut name = String::new();
    for word in 0..words_count {
        name.push_str(&run(rng, UPPER, 1));
        let length_min = usize::from(word + 1 < words_count);
        let length = length_min + rng.below(5 - length_min);
        name.push_str(&run(rng, LOWER_AND_DIGITS, length));
    }
    name
}

/// Returns a random program that passes the grammar, as its lines: functions with random names
/// and types and comments and blank lines in their bodies, and comments and blank lines outside
/// them.
fn random_program(rng: &mut Rng) -> Vec<String> {
    let mut lines = Vec::new();
    for _ in 0..=rng.below(6) {
        match rng.below(10) {
            0 => lines.push(String::new()),
            1..=3 => lines.push(format!("//{}", rng.pick(COMMENT_TEXTS))),
            _ => {
                let (name, return_type) = (random_name(rng), random_type(rng));
                lines.push(format!("function {name}() returns {return_type} {{"));
                let mut returned = false;
                for _ in 0..rng.below(5) {
                    match rng.below(5) {
                        0 => lines.push(String::new()),
                        // A body holds one `return` at most.
                        1 if !returned => {
                            let value = run(rng, b"123456789", 1);
                            let rest_count = rng.below(4);
                            let rest = run(rng, b"0123456789", rest_count);
                            lines.push(format!("{}return {value}{rest}", indent(1)));
                            returned = true;
                        }
                        _ => lines.push(format!("{}//{}", indent(1), rng.pick(COMMENT_TEXTS))),
                    }
                }
                lines.push(String::from("}"));
            }
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
    // The smallest program holds one function, stated here with its columns by hand, which also
    // pins `laid_out`, the way every other row gets its columns.
    let mut examples = Examples::new();
    let main = PlainFunction {
        node_index: 0,
        line: 1,
        name: String::from("main"),
        name_column: 10,
        return_type: String::from("Void"),
        return_type_column: 25,
        returns: Vec::new(),
    };
    assert_eq!(laid_out((0, 1, "main", "Void")), main, "`laid_out` agrees with the one layout");
    examples.add(SMALLEST, vec![main]);
    check(examples);
}

#[test]
fn comments() {
    // A comment means nothing to the checks after the AST, so a source of only comments builds
    // nothing, and comments anywhere else change nothing but which node a function starts at.
    let mut examples = Examples::new();
    builds(&mut examples, "// only\n", &[]);
    builds(&mut examples, "// a\n\n// b\n", &[]);
    builds(&mut examples, "// function main() returns Void {\n", &[]);
    let main_second = [(1, 2, "main", "Void")];
    builds(&mut examples, "// before\nfunction main() returns Void {\n}\n", &main_second);
    builds(&mut examples, "// before\nfunction main() returns Void {\n}\n// after\n", &main_second);
    let main_first = [(0, 1, "main", "Void")];
    builds(&mut examples, "function main() returns Void {\n    // inside\n}\n", &main_first);
    let source = "// a\n\n// b\nfunction main() returns Void {\n    // inside\n\n    // more\n}\n";
    builds(&mut examples, source, &[(2, 4, "main", "Void")]);
    // A comment that reads like a function's first line is still a comment.
    let source = "function main() returns Void {\n    // function x() returns A {\n}\n";
    builds(&mut examples, source, &main_first);
    // Between two functions, and in both their bodies.
    let source = "function a() returns A {\n    // in a\n}\n// between\n\n\
                  function b() returns B {\n    // in b\n    // and more\n}\n";
    builds(&mut examples, source, &[(0, 1, "a", "A"), (3, 6, "b", "B")]);
    check(examples);
}

#[test]
fn many_functions() {
    // The functions are in the order they're in the source. Whether a program has exactly one,
    // and whether it's `main`, is for the checks after the AST, so many build, the same name
    // twice too.
    let mut examples = Examples::new();
    let two_mains = format!("{SMALLEST}{SMALLEST}");
    builds(&mut examples, &two_mains, &[(0, 1, "main", "Void"), (1, 3, "main", "Void")]);
    let source = "function one() returns A {\n}\n\nfunction two() returns B {\n}\n\n\n\
                  function three() returns C {\n}\n";
    builds(&mut examples, source, &[(0, 1, "one", "A"), (1, 4, "two", "B"), (2, 8, "three", "C")]);
    let named: Vec<(String, String)> =
        (0_u32..10).map(|number| (format!("f_{number}"), format!("T{number}"))).collect();
    let ten: Vec<String> = named
        .iter()
        .flat_map(|(name, return_type)| {
            [format!("function {name}() returns {return_type} {{"), String::from("}")]
        })
        .collect();
    let functions: Vec<ExpectedFunction<'_>> = named
        .iter()
        .zip(0_u32..)
        .map(|((name, return_type), number)| {
            (number, 2 * number + 1, name.as_str(), return_type.as_str())
        })
        .collect();
    builds(&mut examples, &source_of(&ten), &functions);
    check(examples);
}

#[test]
fn names_and_types() {
    // Names and types of every shape the lexer allows, short and long, are read whole, each
    // name with each type.
    let mut examples = Examples::new();
    let long_name = format!("a{}", "_b2".repeat(100));
    let long_type = format!("Ab{}", "Bc2".repeat(100));
    for name in ["a", "x_2", "my_function", "main", "a1_2b", long_name.as_str()] {
        for return_type in ["A", "Http", "MessageQueue", "Void", "A2", long_type.as_str()] {
            let source = format!("function {name}() returns {return_type} {{\n}}\n");
            builds(&mut examples, &source, &[(0, 1, name, return_type)]);
        }
    }
    check(examples);
}

#[test]
fn every_token_of_the_smallest_program() {
    // `text` gives the text of every token the source has, the last one too, the `}` that no
    // function names. And a function's `function`, name and return type are the very tokens the
    // lexer made, stated here by hand: the first, the second and the sixth.
    let ast = ast_of(SMALLEST);
    let tokens = ast.forms().blocks().tokens().as_slice();
    let expected: [(&str, u32, u32); 8] = [
        ("function", 1, 1),
        ("main", 1, 10),
        ("(", 1, 14),
        (")", 1, 15),
        ("returns", 1, 17),
        ("Void", 1, 25),
        ("{", 1, 30),
        ("}", 2, 1),
    ];
    assert_eq!(tokens.len(), 8, "the smallest program has eight tokens");
    for (token, (text, line, column)) in tokens.iter().zip(expected) {
        assert_eq!(token.position(), Position::on_line(line).at_column(column), "for {text:?}");
        assert_eq!(ast.text(token), text, "the text of the token at {line}:{column}");
    }
    let functions = ast.functions();
    assert_eq!(functions.len(), 1, "the smallest program has one function");
    let main = &functions[0];
    assert_eq!(main.node_index(), 0);
    assert_eq!(index_in(tokens, ast.keyword(main)), 0);
    assert_eq!(index_in(tokens, ast.name(main)), 1);
    assert_eq!(index_in(tokens, ast.return_type(main)), 5);
}

#[test]
fn every_program() {
    // Every program of one to six items outside every block, of each kind, so every way a
    // function can follow a comment or another function, with or without a comment in its body,
    // is tried. Each is checked against the oracle, which the tables above pin.
    let programs: Vec<String> = (1_u32..=6)
        .flat_map(|items_count| sequences(ITEMS.len(), items_count))
        .map(|sequence| {
            let items: Vec<Item> = sequence.iter().map(|&item| ITEMS[item]).collect();
            source_of(&program_of(&items))
        })
        .collect();
    assert_eq!(programs.len(), 1_092, "3 + 9 + 27 + 81 + 243 + 729 programs");
    // Each program passes the grammar, and the oracle finds exactly as many functions as the
    // programs hold, before anything is built.
    let mut functions_count = 0;
    let mut checks = Vec::new();
    for source in programs {
        assert!(parses(&source), "{} should pass the grammar", excerpt(&source));
        let expected = oracle(&source);
        functions_count += expected.len();
        checks.push((source, expected));
    }
    assert_eq!(functions_count, 4_010, "two of every three items, in every place, are functions");
    for (source, expected) in checks {
        assert_eq!(built(&source), expected, "for {}", excerpt(&source));
    }
}

#[test]
fn the_largest_sources() {
    // Each limit goes as far as the largest source lets it, and one more doesn't fit. So nothing
    // the AST counts can be counted in anything smaller.
    let mut examples = Examples::new();
    let size = usize_from_u32(SOURCE_BYTES_MAX);
    // The most functions: 606 of the shortest. The AST has room for a function in every four
    // bytes, which is more, since that holds for any function there could be.
    assert_eq!(SHORTEST.len(), 27);
    assert!(SHORTEST.repeat(606).len() <= size);
    assert!(SHORTEST.repeat(607).len() > size);
    const { assert!(FUNCTIONS_MAX == SOURCE_BYTES_MAX / 4, "room for a function in four bytes") };
    let functions: Vec<ExpectedFunction<'_>> =
        (0_u32..606).map(|number| (number, 2 * number + 1, "a", "A")).collect();
    assert!(functions.len() <= usize_from_u32(FUNCTIONS_MAX));
    builds(&mut examples, &SHORTEST.repeat(606), &functions);
    // The most nodes, all comments, which build nothing.
    assert!("//\n".repeat(5_461).len() <= size);
    assert!("//\n".repeat(5_462).len() > size);
    builds(&mut examples, &"//\n".repeat(5_461), &[]);
    // The most nodes with a function among them: the shortest function, last, after the most
    // comments.
    let function_after = |count: usize| format!("{}{SHORTEST}", "//\n".repeat(count));
    assert!(function_after(5_452).len() <= size);
    assert!(function_after(5_453).len() > size);
    builds(&mut examples, &function_after(5_452), &[(5_452, 5_453, "a", "A")]);
    // The longest name, and the longest return type.
    let named = |length: usize| format!("function {}() returns A {{\n}}\n", "a".repeat(length));
    let name_length_max = size - named(0).len();
    assert_eq!(name_length_max, 16_358, "the longest name");
    assert!(named(name_length_max + 1).len() > size);
    builds(&mut examples, &named(name_length_max), &[(0, 1, &"a".repeat(name_length_max), "A")]);
    let typed = |length: usize| format!("function a() returns A{} {{\n}}\n", "a".repeat(length));
    let tail_length_max = size - typed(0).len();
    assert_eq!(tail_length_max + 1, 16_358, "the longest return type, with its capital");
    assert!(typed(tail_length_max + 1).len() > size);
    let longest_type = format!("A{}", "a".repeat(tail_length_max));
    builds(&mut examples, &typed(tail_length_max), &[(0, 1, "a", &longest_type)]);
    // The most comments in a body, alone, and then with a function after it, so stepping over the
    // body lands on the function after it.
    let alone =
        |count: usize| format!("function a() returns A {{\n{}}}\n", "    //\n".repeat(count));
    assert!(alone(2_336).len() <= size);
    assert!(alone(2_337).len() > size);
    builds(&mut examples, &alone(2_336), &[(0, 1, "a", "A")]);
    let bodied = |count: usize| format!("{}{SHORTEST}", alone(count));
    assert!(bodied(2_332).len() <= size);
    assert!(bodied(2_333).len() > size);
    builds(&mut examples, &bodied(2_332), &[(0, 1, "a", "A"), (2_333, 2_335, "a", "A")]);
    // A function far down, after the most blank lines.
    let far = format!("{}{SHORTEST}", "\n".repeat(size - SHORTEST.len()));
    assert_eq!(far.len(), size);
    builds(&mut examples, &far, &[(0, 16_358, "a", "A")]);
    check(examples);
}

#[test]
fn statements() {
    // A function's statements are its body's `return` lines, in its own run of the table, and
    // comments aren't among them. A function with none has an empty run, wherever it is.
    let returns_3 = "function main() returns U8 {\n    return 3\n}\n";
    assert_eq!(oracle(returns_3), built(returns_3), "the oracle, for {returns_3:?}");
    let functions = built(returns_3);
    let expected = PlainReturn { line: 2, value: String::from("3"), value_column: 12 };
    assert_eq!(functions[0].returns, [expected]);
    // Comments around it, and a long integer, read whole.
    let long = "9".repeat(300);
    let source =
        format!("function a() returns U8 {{\n    // x\n\n    return {long}\n    // y\n}}\n");
    let functions = built(&source);
    assert_eq!(oracle(&source), functions, "the oracle, for {}", excerpt(&source));
    let expected = PlainReturn { line: 4, value: long, value_column: 12 };
    assert_eq!(functions[0].returns, [expected]);
    // Each function's run is its own: one with a `return`, one without, then one with.
    let source = "function a() returns U8 {\n    return 1\n}\nfunction b() returns Void {\n    // \
                  none\n}\nfunction c() returns U8 {\n    return 22\n}\n";
    let ast = ast_of(source);
    let counts: Vec<usize> =
        ast.functions().iter().map(|function| ast.statements(function).len()).collect();
    assert_eq!(counts, [1, 0, 1]);
    let values: Vec<&str> = ast
        .functions()
        .iter()
        .flat_map(|function| ast.statements(function))
        .map(|statement| ast.text(ast.value(statement)))
        .collect();
    assert_eq!(values, ["1", "22"]);
    assert_eq!(oracle(source), functions_of(&ast), "the oracle, for {}", excerpt(source));
}

#[test]
#[should_panic(expected = "a statement is one of its source's")]
fn value_of_a_copy_of_a_statement_is_a_bug() {
    // A copy of a statement is equal to it and is somewhere else, so it's no statement of the
    // AST's.
    let ast = ast_of("function main() returns U8 {\n    return 3\n}\n");
    let function = ast.functions().first().expect("the source has a function");
    let copy = *ast.statements(function).first().expect("the function has a statement");
    let _: &Token = ast.value(&copy);
}

#[test]
#[should_panic(expected = "a statement is one of its source's")]
fn keyword_of_a_statement_from_another_ast_is_a_bug() {
    // Two ASTs of one source hold equal statements. Each still means something only in its own.
    let source = "function main() returns U8 {\n    return 3\n}\n";
    let (first, second) = (ast_of(source), ast_of(source));
    let function = second.functions().first().expect("the source has a function");
    let statement = second.statements(function).first().expect("the function has a statement");
    let _: &Token = first.keyword_of(statement);
}

#[test]
fn every_source_owns_its_functions() {
    // Sources built one after another, the most functions a source can hold among them, with
    // every one's AST kept while the next is built. Each source's functions are still its own at
    // the end, however many more or fewer the sources built after it hold.
    let most_functions = SHORTEST.repeat(606);
    let most: Vec<ExpectedFunction<'_>> =
        (0_u32..606).map(|number| (number, 2 * number + 1, "a", "A")).collect();
    let main = [(0, 1, "main", "Void")];
    let rows: [(&str, &[ExpectedFunction<'_>]); 6] = [
        (&most_functions, &most),
        (SMALLEST, &main),
        ("// x\n", &[]),
        (&most_functions, &most),
        ("// x\n", &[]),
        (SMALLEST, &main),
    ];
    for (source, functions) in rows {
        assert_eq!(oracle(source), all_laid_out(functions), "the oracle, for {}", excerpt(source));
    }
    let kept: Vec<Ast<'_>> = rows.iter().map(|(source, _)| ast_of(source)).collect();
    for ((source, functions), ast) in rows.iter().zip(&kept) {
        assert_eq!(functions_of(ast), all_laid_out(functions), "for {}", excerpt(source));
    }
}

#[test]
#[should_panic(expected = "a function is one of its source's functions")]
fn keyword_of_a_function_equal_to_its_own_is_a_bug() {
    // Two ASTs of one source hold equal functions. Each still means something only in its own.
    ask_of_a_function_equal_to_its_own(|ast, function| {
        let _: &Token = ast.keyword(function);
    });
}

#[test]
#[should_panic(expected = "a function is one of its source's functions")]
fn name_of_a_function_equal_to_its_own_is_a_bug() {
    // Asking for the name of another AST's equal function is refused the same way.
    ask_of_a_function_equal_to_its_own(|ast, function| {
        let _: &Token = ast.name(function);
    });
}

#[test]
#[should_panic(expected = "a function is one of its source's functions")]
fn return_type_of_a_function_equal_to_its_own_is_a_bug() {
    // And so is asking for its return type.
    ask_of_a_function_equal_to_its_own(|ast, function| {
        let _: &Token = ast.return_type(function);
    });
}

#[test]
#[should_panic(expected = "a function is one of its source's functions")]
fn keyword_of_a_copy_of_a_function_is_a_bug() {
    // A copy of a function is equal to it and is somewhere else, so it's no function of the
    // AST's.
    let ast = ast_of(SMALLEST);
    let copy = *ast.functions().first().expect("the source has a function");
    let _: &Token = ast.keyword(&copy);
}

#[test]
#[should_panic(expected = "a function is one of its source's functions")]
fn keyword_of_a_function_that_would_fit_another_source_is_a_bug() {
    // A function only means something in the AST it was built in. This one starts at a node
    // where the AST has a function of its own, and it's still refused.
    ask_of_a_function_that_would_fit(|ast, function| {
        let _: &Token = ast.keyword(function);
    });
}

#[test]
#[should_panic(expected = "a function is one of its source's functions")]
fn name_of_a_function_that_would_fit_another_source_is_a_bug() {
    // Where the same function's name is, the AST's source has a `(`. Asking for it is refused for
    // whose function it is, before any token is read.
    ask_of_a_function_that_would_fit(|ast, function| {
        let _: &Token = ast.name(function);
    });
}

#[test]
#[should_panic(expected = "a function is one of its source's functions")]
fn return_type_of_a_function_that_would_fit_another_source_is_a_bug() {
    // Where the same function's return type is, the AST's source has a `{`. Asking for it is
    // refused the same way.
    ask_of_a_function_that_would_fit(|ast, function| {
        let _: &Token = ast.return_type(function);
    });
}

#[test]
#[should_panic(expected = "a function is one of its source's functions")]
fn keyword_of_a_function_from_a_longer_source_is_a_bug() {
    // Here the function's first line is past the last node the AST's source has, so it's refused
    // before any node is read.
    ask_of_a_function_from_a_longer_source(|ast, function| {
        let _: &Token = ast.keyword(function);
    });
}

#[test]
#[should_panic(expected = "a function is one of its source's functions")]
fn name_of_a_function_from_a_longer_source_is_a_bug() {
    // The same function's name is past the last token the AST's source has. Asking for it is
    // refused for whose function it is, before any token is read.
    ask_of_a_function_from_a_longer_source(|ast, function| {
        let _: &Token = ast.name(function);
    });
}

#[test]
#[should_panic(expected = "a function is one of its source's functions")]
fn return_type_of_a_function_from_a_longer_source_is_a_bug() {
    // And so is asking for its return type.
    ask_of_a_function_from_a_longer_source(|ast, function| {
        let _: &Token = ast.return_type(function);
    });
}

#[test]
#[should_panic(expected = "a token is one of its source's tokens")]
fn text_of_a_token_that_would_fit_another_source_is_a_bug() {
    // The name in the other source would fit in the AST's, where the bytes it spans are the
    // AST's own name and the `(` after it. It still isn't one of the AST's tokens.
    let other = ast_of("function ab() returns A {\n}\n");
    let ast = ast_of(SHORTEST);
    let function = other.functions().first().expect("the other source has a function");
    let _: &str = ast.text(other.name(function));
}

#[test]
#[should_panic(expected = "a token is one of its source's tokens")]
fn text_of_a_token_from_a_longer_source_is_a_bug() {
    // A token only means something in the source it was lexed from. Here the last token of a
    // longer source is asked of a shorter one's AST, which it reaches past the end of.
    let longer = ast_of(TWO_FUNCTIONS);
    let ast = ast_of(SMALLEST);
    let last = longer.forms().blocks().tokens().as_slice().last().expect("a source has tokens");
    let _: &str = ast.text(last);
}

#[test]
fn fuzz_against_the_oracle() {
    // 20,000 random programs that pass the grammar, each built and checked against what the
    // oracle says of it. The shapes that matter are counted, so the test fails if the programs
    // stop reaching one.
    let mut rng = Rng::new(0x4153_545F_4255_494C);
    let mut reached = ShapeCounts::default();
    let mut checks = Vec::new();
    for _ in 0_u32..20_000 {
        let lines = random_program(&mut rng);
        let source = source_of(&lines);
        assert!(parses(&source), "the generator passes the grammar with {}", excerpt(&source));
        let expected = oracle(&source);
        reached.count(&lines, &expected);
        checks.push((source, expected));
    }
    // The fuzzing only means something if the programs reach the shapes that matter, often. This
    // is checked before anything is built, so it holds whatever `build` does. Each floor is about
    // half of what the seed reaches.
    for (shape, count, floor) in [
        ("functions", reached.functions, 21_000),
        ("programs with no function", reached.programs_without_a_function, 1_000),
        ("programs with three functions or more", reached.programs_of_three_functions, 3_600),
        ("comments in bodies", reached.comments_in_bodies, 31_000),
        ("blank lines in bodies", reached.blank_lines_in_bodies, 10_000),
        ("names longer than a byte counts", reached.long_names, 450),
        ("types longer than a byte counts", reached.long_types, 350),
        ("`return` lines", reached.returns, 7_000),
        ("`return` lines with a comment after them", reached.commented_returns, 6_000),
    ] {
        assert!(count > floor, "only {count} reached {shape}; reached {reached:?}");
    }
    // The oracle made these answers, so each goes straight to `build`.
    for (source, expected) in checks {
        assert_eq!(built(&source), expected, "for {}", excerpt(&source));
    }
}
