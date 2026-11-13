// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the program. They're a crate of their own, so the compiler lets them reach only
//! what the crates make public: every source goes through `lint`, `lex`, `lint_tokens`, `nest`,
//! `parse`, `build` and `resolve`, and then `check_program`.
//!
//! Every source here passes the types, so it reaches the program, and each row says what it
//! should make of it: the line `main` is on, or the problem and where it is. An oracle works the
//! same out from the functions' first lines alone, and every row checks the oracle before
//! `check_program` sees any. Every program of a few functions, from a few names, is tried, and a
//! seeded fuzz test checks random programs against the oracle. The largest sources sit at each
//! limit a source of 16,384 bytes has.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::ptr;

use stella_ast::build;
use stella_blocks::nest;
use stella_grammar::parse;
use stella_lexer::lex;
use stella_linter_text::lint;
use stella_linter_tokens::lint_tokens;
use stella_meaning::Kind::{AnotherFunction, NotMain};
use stella_meaning::Type::Void;
use stella_meaning::{Kind, Problem, Types, check_program, resolve};
use stella_source::{Position, SOURCE_BYTES_MAX, usize_from_u32};
use stella_test_support::{Example, Examples, Rng, excerpt, indent, programs_of, source_of};

/// What the program check should make of a source: the line `main` is on, or the first problem.
type Outcome = Result<u32, Problem>;

/// This is the smallest program, laid out the one way.
const SMALLEST: &str = "function main() returns Void {\n}\n";

/// These are the items a program is made of, in `every_program`.
const ITEMS: [&str; 4] = [
    "// a comment",
    "function main() returns Void {",
    "function a() returns Void {",
    "function mains() returns Void {",
];

/// These are a function's names: `main` most often, and names close to it.
const NAMES: &[&str] =
    &["main", "main", "main", "a", "mains", "main_2", "ma", "my_main", "xain", "maix"];

/// Says whether `source` passes every check through the types, so it reaches the program.
fn resolves(source: &str) -> bool {
    let Ok(text) = lint(source.as_bytes()) else {
        return false;
    };
    let Ok(tokens) = lex(text) else {
        return false;
    };
    let Ok(layout) = lint_tokens(tokens) else {
        return false;
    };
    let Ok(forms) = parse(nest(layout)) else {
        return false;
    };
    resolve(build(forms)).is_ok()
}

/// Returns what the types found in `source`, which must pass them.
fn types_of(source: &str) -> Types<'_> {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    let tokens = lex(text).expect("source should lex");
    let layout = lint_tokens(tokens).expect("source should be laid out the one way");
    let forms = parse(nest(layout)).expect("source should pass the grammar");
    resolve(build(forms)).expect("source should pass the types")
}

/// Checks the program `source` holds, which must pass the types, and returns what the program
/// check made of it. On the way, it checks that what `check_program` returns holds the very AST
/// the types were found for, and the types, and that its `main` is the source's one function,
/// named `main`, which returns `Void`.
fn checked(source: &str) -> Outcome {
    let types = types_of(source);
    let functions = ptr::from_ref(types.ast().functions());
    let found = types.as_slice().to_vec();
    let program = check_program(types)?;
    let types = program.types();
    let ast = types.ast();
    let main = &ast.functions()[usize_from_u32(program.main_function_index())];
    assert!(ptr::eq(ast.functions(), functions), "it holds the AST the types were found for");
    assert_eq!(types.as_slice(), found, "it holds the types found");
    assert_eq!(ast.functions(), [*main], "`main` is the program's one function");
    assert_eq!(program.main_function_index(), 0, "so `main` is the first of the functions");
    assert_eq!(ast.text(ast.name(main)), "main", "`main` is named `main`");
    assert_eq!(types.as_slice(), [Void], "`main` returns `Void`");
    Ok(ast.name(main).position().line())
}

/// Returns a problem of `kind` at `line` and `column`.
// KS-DISTINCT-ARGS: a line then a column is the order every message prints them in, and an
// expectation with the two swapped fails its test.
const fn problem_at(kind: Kind, line: u32, column: u32) -> Problem {
    Problem::At { kind, position: Position::on_line(line).at_column(column) }
}

/// Checks that every example passes the types, as the program check needs, and that the oracle
/// agrees with it. Then checks every one against `check_program`. So every example checks the
/// oracle before `check_program` sees any, whatever `check_program` does.
fn check(examples: Examples<Outcome>) {
    let examples = examples.take();
    for Example { source, expected, at } in &examples {
        let excerpt = excerpt(source);
        assert!(resolves(source), "{at}: {excerpt} should pass the types");
        assert_eq!(oracle(source), *expected, "{at}: the oracle, for {excerpt}");
    }
    for Example { source, expected, at } in &examples {
        assert_eq!(checked(source), *expected, "{at}: for {}", excerpt(source));
    }
}

/// Returns a function named `name` that returns `Void`, with an empty body.
fn function(name: &str) -> String {
    format!("function {name}() returns Void {{\n}}\n")
}

/// Works out what the program check should make of `source` from its text alone. `source` must
/// pass the types, so a line that starts with `function ` is a function's first line, laid out
/// the one way, and its name runs from after `function ` to `(`. So a name starts at column 10.
fn oracle(source: &str) -> Outcome {
    let mut main = None;
    for (content, number) in source.split_terminator('\n').zip(1_u32..) {
        let Some(rest) = content.strip_prefix("function ") else {
            continue;
        };
        if main.is_some() {
            return Err(problem_at(AnotherFunction, number, 1));
        }
        let (name, _) = rest.split_once('(').expect("a function's first line");
        if name != "main" {
            return Err(problem_at(NotMain, number, 10));
        }
        main = Some(number);
    }
    main.ok_or(Problem::NoMain)
}

/// Returns what `outcome` is called where outcomes are counted: `passes`, or the problem's kind.
fn outcome_name(outcome: &Outcome) -> String {
    match outcome {
        Ok(_) => String::from("passes"),
        Err(Problem::NoMain) => String::from("NoMain"),
        Err(Problem::At { kind, .. }) => format!("{kind:?}"),
    }
}

/// Returns the line `outcome` is on: `main`'s, or the problem's. A problem with the file as a
/// whole is on none.
const fn outcome_line(outcome: &Outcome) -> Option<u32> {
    match outcome {
        Ok(line) => Some(*line),
        Err(Problem::NoMain) => None,
        Err(Problem::At { position, .. }) => Some(position.line()),
    }
}

/// Returns a random program that passes the types, as its lines: functions with random names that
/// return `Void`, with comments and blank lines in their bodies, and comments and blank lines
/// outside them.
fn random_program(rng: &mut Rng) -> Vec<String> {
    let mut lines = Vec::new();
    for _ in 0..=rng.below(5) {
        match rng.below(10) {
            0 => lines.push(String::new()),
            1..=3 => lines.push(String::from("// function main() returns Void {")),
            _ => {
                lines.push(format!("function {}() returns Void {{", rng.pick(NAMES)));
                for _ in 0..rng.below(3) {
                    lines.push(format!("{}// main", indent(1)));
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
    // The smallest program is one function, `main`, on line 1.
    let mut examples = Examples::new();
    examples.add(SMALLEST, Ok(1));
    check(examples);
}

#[test]
fn the_main_function() {
    // `main` is found wherever it is, among comments and blank lines, with comments in its body.
    let mut examples = Examples::new();
    examples.add("// a\n\n// b\nfunction main() returns Void {\n    // in\n}\n", Ok(4));
    examples.add("function main() returns Void {\n    // in\n\n    // more\n}\n// after\n", Ok(1));
    check(examples);
}

#[test]
fn no_main() {
    // A source with no function has no `main`, and nothing to point at, so it's refused as a
    // whole, even when its comments read like a function.
    let mut examples = Examples::new();
    examples.add("// only\n", Err(Problem::NoMain));
    examples.add("// a\n\n// b\n", Err(Problem::NoMain));
    examples.add("// function main() returns Void {\n// }\n", Err(Problem::NoMain));
    check(examples);
}

#[test]
fn not_main() {
    // A program's one function is named `main`, whole, so a name that's almost `main` is no more
    // `main` than any other.
    let mut examples = Examples::new();
    for name in
        ["a", "mains", "main_2", "ma", "mai", "my_main", "x_main", "maim", "m", "xain", "mein"]
    {
        examples.add(&function(name), Err(problem_at(NotMain, 1, 10)));
    }
    examples.add(&format!("// a\n\n{}", function("start")), Err(problem_at(NotMain, 3, 10)));
    check(examples);
}

#[test]
fn another_function() {
    // After `main`, any function is one too many, whatever it's named.
    let mut examples = Examples::new();
    examples.add(&format!("{SMALLEST}{SMALLEST}"), Err(problem_at(AnotherFunction, 3, 1)));
    examples.add(&format!("{SMALLEST}{}", function("a")), Err(problem_at(AnotherFunction, 3, 1)));
    let spaced = format!("{SMALLEST}\n// between\n\n{}", function("helper"));
    examples.add(&spaced, Err(problem_at(AnotherFunction, 6, 1)));
    check(examples);
}

#[test]
fn the_first_problem_wins() {
    // The functions are checked in order, so the first fault is the one reported, however many
    // follow it.
    let mut examples = Examples::new();
    let source = format!("{}{SMALLEST}", function("start"));
    examples.add(&source, Err(problem_at(NotMain, 1, 10)));
    let source = format!("{}{}", function("a"), function("b"));
    examples.add(&source, Err(problem_at(NotMain, 1, 10)));
    let source = format!("{SMALLEST}{}{}", function("a"), function("b"));
    examples.add(&source, Err(problem_at(AnotherFunction, 3, 1)));
    let source = format!("{SMALLEST}{SMALLEST}{SMALLEST}");
    examples.add(&source, Err(problem_at(AnotherFunction, 3, 1)));
    check(examples);
}

#[test]
fn the_largest_sources() {
    // Each limit goes as far as the largest source lets it, and one more doesn't fit. So nothing
    // the program check counts can be counted in anything smaller.
    let mut examples = Examples::new();
    let size = usize_from_u32(SOURCE_BYTES_MAX);
    // The most functions there can be, when they pass the types: the first isn't `main`.
    let shortest = function("a");
    assert!(shortest.repeat(546).len() <= size);
    assert!(shortest.repeat(547).len() > size);
    examples.add(&shortest.repeat(546), Err(problem_at(NotMain, 1, 10)));
    // `main`, then as many more as fit.
    let after_main = |count: usize| format!("{SMALLEST}{}", shortest.repeat(count));
    assert!(after_main(545).len() <= size);
    assert!(after_main(546).len() > size);
    examples.add(&after_main(545), Err(problem_at(AnotherFunction, 3, 1)));
    // The most `main`s.
    assert!(SMALLEST.repeat(496).len() <= size);
    assert!(SMALLEST.repeat(497).len() > size);
    examples.add(&SMALLEST.repeat(496), Err(problem_at(AnotherFunction, 3, 1)));
    // The most lines, all comments: no `main`.
    assert!("//\n".repeat(5_461).len() <= size);
    assert!("//\n".repeat(5_462).len() > size);
    examples.add(&"//\n".repeat(5_461), Err(Problem::NoMain));
    // `main` after the most comments.
    let main_after = |count: usize| format!("{}{SMALLEST}", "//\n".repeat(count));
    assert!(main_after(5_450).len() <= size);
    assert!(main_after(5_451).len() > size);
    examples.add(&main_after(5_450), Ok(5_451));
    // A function that isn't `main` on the last line a function can be on, and `main` on the last
    // line it can be on.
    let far = format!("{}{shortest}", "\n".repeat(size - shortest.len()));
    assert_eq!(far.len(), size);
    examples.add(&far, Err(problem_at(NotMain, 16_355, 10)));
    let far = format!("{}{SMALLEST}", "\n".repeat(size - SMALLEST.len()));
    assert_eq!(far.len(), size);
    examples.add(&far, Ok(16_352));
    // The longest name, which isn't `main`.
    let named = |length: usize| function(&"a".repeat(length));
    let name_most = size - named(0).len();
    assert_eq!(name_most, 16_355, "the longest name");
    assert!(named(name_most + 1).len() > size);
    examples.add(&named(name_most), Err(problem_at(NotMain, 1, 10)));
    check(examples);
}

#[test]
fn every_program() {
    // Every program of one to five items, so `main` comes first, later, more than once, or not at
    // all, among comments and other functions.
    let sources = programs_of(&ITEMS, 5);
    assert_eq!(sources.len(), 1_364, "4 + 16 + 64 + 256 + 1,024 programs");
    // Each passes the types, and the oracle's outcomes are counted exactly, before anything is
    // checked.
    let mut reached: BTreeMap<String, u32> = BTreeMap::new();
    let mut expected = Vec::new();
    for source in &sources {
        assert!(resolves(source), "{} should pass the types", excerpt(source));
        let outcome = oracle(source);
        *reached.entry(outcome_name(&outcome)).or_default() += 1;
        expected.push(outcome);
    }
    let counts: Vec<(&str, u32)> =
        reached.iter().map(|(key, &count)| (key.as_str(), count)).collect();
    let expected_counts =
        [("AnotherFunction", 438), ("NoMain", 5), ("NotMain", 906), ("passes", 15)];
    assert_eq!(counts, expected_counts, "the outcomes, counted");
    for (source, expected) in sources.iter().zip(expected) {
        assert_eq!(checked(source), expected, "for {}", excerpt(source));
    }
}

#[test]
fn fuzz_against_the_oracle() {
    // 20,000 random programs that pass the types, each checked against what the oracle says of
    // it. The outcomes are counted, on line 1 and past it, so the test fails if the programs stop
    // reaching one.
    let mut rng = Rng::new(0x5052_4F47_5241_4D31);
    let mut reached: BTreeMap<String, u32> = BTreeMap::new();
    let mut checks = Vec::new();
    for _ in 0_u32..20_000 {
        let source = source_of(&random_program(&mut rng));
        assert!(resolves(&source), "the generator passes the types with {}", excerpt(&source));
        let expected = oracle(&source);
        let name = outcome_name(&expected);
        let is_past_line_1 = outcome_line(&expected).is_some_and(|line| line > 1);
        let key = if is_past_line_1 { format!("{name}, past line 1") } else { name };
        *reached.entry(key).or_default() += 1;
        checks.push((source, expected));
    }
    // The fuzzing only means something if the sources reach every outcome, often. This is checked
    // before anything is checked, so it holds whatever `check_program` does. Each floor is about
    // half of what the seed reaches.
    let reached_by = |key: &str| reached.get(key).copied().unwrap_or(0);
    for (outcome, floor) in [
        ("passes", 575),
        ("passes, past line 1", 385),
        ("NoMain", 1_300),
        ("NotMain", 4_200),
        ("NotMain, past line 1", 1_850),
        ("AnotherFunction, past line 1", 1_600),
    ] {
        let count = reached_by(outcome);
        assert!(count > floor, "only {count} reached {outcome}; reached {reached:?}");
    }
    // The oracle made these answers, so each goes straight to `check_program`.
    for (source, expected) in checks {
        assert_eq!(checked(&source), expected, "for {}", excerpt(&source));
    }
}
