// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the fit check. They're a crate of their own, so the compiler lets them reach
//! only what the crates make public: every source goes through every check before it, and then
//! `check_fit`.
//!
//! Every source here passes the types, and each row says what the fit should make of it: what
//! each function gives back, or the problem and where it is. An oracle works the same out from
//! the text alone, reading an integer as a number wide enough for any it meets, and every row
//! checks the oracle before `check_fit` sees any. Every integer from 0 to 1,000 is tried, so the
//! edge of `U8` is crossed in every way it can be, and a seeded fuzz test checks random programs
//! against the oracle.

#![forbid(unsafe_code)]

use std::ptr;

use stella_ast::build;
use stella_blocks::nest;
use stella_grammar::parse;
use stella_lexer::lex;
use stella_linter_text::lint;
use stella_linter_tokens::lint_tokens;
use stella_meaning::Kind::{DoesntFit, MissingReturn, ReturnInVoid};
use stella_meaning::{IntegerType, Kind, Problem, Returned, Types, check_fit, resolve};
use stella_source::{Position, SOURCE_BYTES_MAX, usize_from_u32};
use stella_test_support::{Example, Examples, Rng, excerpt, source_of};

/// What the fit should make of a source: what each function gives back, or the first problem.
type Outcome = Result<Vec<Returned>, Problem>;

/// These follow a comment's `//`. Some read like a `return`, which a comment never is.
const COMMENT_TEXTS: &[&str] = &["", " a", " return 3", " return 256"];

/// Returns what the types found in `source`, which must pass them.
fn types_of(source: &str) -> Types<'_> {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    let tokens = lex(text).expect("source should lex");
    let layout = lint_tokens(tokens).expect("source should be laid out the one way");
    let forms = parse(nest(layout)).expect("source should pass the grammar");
    resolve(build(forms)).expect("source should pass the types")
}

/// Fits `source`, which must pass the types, and returns what the fit made of it. On the way, it
/// checks that what `check_fit` returns holds the very AST and types it was given.
fn fitted(source: &str) -> Outcome {
    let types = types_of(source);
    let (functions, found) = (ptr::from_ref(types.ast().functions()), types.as_slice().to_vec());
    let fitted = check_fit(types)?;
    let ast = fitted.types().ast();
    assert!(ptr::eq(ast.functions(), functions), "it holds the AST it was given");
    assert_eq!(fitted.types().as_slice(), found, "it holds the types it was given");
    assert_eq!(fitted.as_slice().len(), found.len(), "each function gives back one thing");
    for (given, &found) in fitted.as_slice().iter().zip(&found) {
        assert_eq!(given.type_of(), found, "what's given back is of the function's type");
    }
    Ok(fitted.as_slice().to_vec())
}

/// Returns a problem of `kind` at `line` and `column`.
// KS-DISTINCT-ARGS: a line then a column is the order every message prints them in, and an
// expectation with the two swapped fails its test.
const fn problem_at(kind: Kind, line: u32, column: u32) -> Problem {
    Problem::At { kind, position: Position::on_line(line).at_column(column) }
}

/// Works out what the fit should make of `source` from its text alone. `source` must pass the
/// types, so a line that starts with `function ` is a function's first line, laid out the one
/// way, and names `Void` or `U8`; a line that starts with `    return ` is its body's `return`;
/// and a line that's `}` closes its body. An integer is read into a `u128`, which holds every
/// integer short enough to fit, and one too long to read is too large for `U8` all the same.
fn oracle(source: &str) -> Outcome {
    let mut returned = Vec::new();
    // The current function's return type, and its `return`'s value and line, once it has one.
    let mut current: Option<(&str, Option<(&str, u32)>)> = None;
    for (content, number) in source.split_terminator('\n').zip(1_u32..) {
        if let Some(rest) = content.strip_prefix("function ") {
            let (_, rest) = rest.split_once("() returns ").expect("a function's first line");
            let return_type = rest.strip_suffix(" {").expect("a function's first line ends `{`");
            current = Some((return_type, None));
            continue;
        }
        if let Some(value) = content.strip_prefix("    return ") {
            let (_, given) = current.as_mut().expect("a `return` is in a function's body");
            *given = Some((value, number));
            continue;
        }
        // Anything else is a comment, a blank line, or the `}` that closes a body.
        if content == "}" {
            let (return_type, given) = current.take().expect("a `}` closes a function's body");
            returned.push(oracle_function(return_type, given, number)?);
        }
    }
    Ok(returned)
}

/// Returns what a function that returns `return_type`, whose body gives back `given` and is
/// closed on line `close_line`, gives back, or its problem.
fn oracle_function(
    return_type: &str,
    given: Option<(&str, u32)>,
    close_line: u32,
) -> Result<Returned, Problem> {
    match (return_type, given) {
        ("Void", None) => Ok(Returned::Void),
        ("Void", Some((_, line))) => Err(problem_at(ReturnInVoid, line, 5)),
        ("U8", None) => Err(problem_at(MissingReturn(IntegerType::U8), close_line, 1)),
        ("U8", Some((value, line))) => {
            let read = value.parse::<u128>().ok().and_then(|value| u8::try_from(value).ok());
            read.map(Returned::U8).ok_or_else(|| problem_at(DoesntFit(IntegerType::U8), line, 12))
        }
        (other, _) => unreachable!("the types passed, so `{other}` is `Void` or `U8`"),
    }
}

/// Checks that every example agrees with the oracle, then checks every one against `check_fit`.
fn check(examples: Examples<Outcome>) {
    let examples = examples.take();
    for Example { source, expected, at } in &examples {
        assert_eq!(oracle(source), *expected, "{at}: the oracle, for {}", excerpt(source));
    }
    for Example { source, expected, at } in &examples {
        assert_eq!(fitted(source), *expected, "{at}: for {}", excerpt(source));
    }
}

/// Returns `main`, returning `return_type`, with `body` as its body's lines.
fn main_of(return_type: &str, body: &str) -> String {
    format!("function main() returns {return_type} {{\n{body}}}\n")
}

/// Returns a random program that passes the types, as its lines: functions that return `Void` or
/// `U8`, each with a `return` or none, its integer sometimes too large, and comments around it.
fn random_program(rng: &mut Rng) -> Vec<String> {
    let mut lines = Vec::new();
    for _ in 0..=rng.below(4) {
        let return_type = rng.pick(&["Void", "U8", "U8"]);
        lines.push(format!("function main() returns {return_type} {{"));
        if rng.below(3) == 0 {
            lines.push(format!("    //{}", rng.pick(COMMENT_TEXTS)));
        }
        if rng.below(5) > 0 {
            let value = match rng.below(4) {
                0 => format!("{}", 256 + rng.below(10_000)),
                1 => {
                    let nines = rng.below(40);
                    format!("{}{}", 1 + rng.below(9), "9".repeat(nines))
                }
                _ => format!("{}", rng.below(256)),
            };
            lines.push(format!("    return {value}"));
        }
        if rng.below(3) == 0 {
            lines.push(format!("    //{}", rng.pick(COMMENT_TEXTS)));
        }
        lines.push(String::from("}"));
    }
    lines
}

#[test]
fn void() {
    // A function that returns `Void` returns when its body ends, so it has no `return`, and one
    // that has one is refused at it.
    let mut examples = Examples::new();
    examples.add(&main_of("Void", ""), Ok(vec![Returned::Void]));
    examples.add(&main_of("Void", "    // return 3\n"), Ok(vec![Returned::Void]));
    let refused = Err(problem_at(ReturnInVoid, 2, 5));
    examples.add(&main_of("Void", "    return 3\n"), refused.clone());
    examples.add(&main_of("Void", "    return 300\n"), refused);
    examples
        .add(&main_of("Void", "    // a\n\n    return 0\n"), Err(problem_at(ReturnInVoid, 4, 5)));
    check(examples);
}

#[test]
fn every_integer_up_to_a_thousand() {
    // Every integer from 0 to 1,000, in a function that returns `U8`. Those from 0 to 255 fit,
    // and each is given back as itself; those past it are refused at the integer.
    let mut examples = Examples::new();
    let mut fit_count: u32 = 0;
    for value in 0_u32..=1_000 {
        let source = main_of("U8", &format!("    return {value}\n"));
        match u8::try_from(value) {
            Ok(fits) => {
                examples.add(&source, Ok(vec![Returned::U8(fits)]));
                fit_count += 1;
            }
            Err(_) => examples.add(&source, Err(problem_at(DoesntFit(IntegerType::U8), 2, 12))),
        }
    }
    assert_eq!(fit_count, 256);
    check(examples);
}

#[test]
fn long_integers() {
    // An integer of any length is read without overflowing, and refused when it's too large,
    // however much too large: past what a `u64` holds, and as long as a source can hold.
    let mut examples = Examples::new();
    let too_large = Err(problem_at(DoesntFit(IntegerType::U8), 2, 12));
    for digits in [
        String::from("18446744073709551616"),
        "9".repeat(40),
        format!("1{}", "0".repeat(30)),
        String::from("2556"),
    ] {
        examples.add(&main_of("U8", &format!("    return {digits}\n")), too_large.clone());
    }
    // The longest integer a source holds: everything but the rest of the function.
    let around = main_of("U8", "    return \n").len();
    let longest = "9".repeat(usize_from_u32(SOURCE_BYTES_MAX) - around);
    let source = main_of("U8", &format!("    return {longest}\n"));
    assert_eq!(source.len(), usize_from_u32(SOURCE_BYTES_MAX), "the source is the largest");
    examples.add(&source, too_large);
    check(examples);
}

#[test]
fn missing_returns() {
    // A function that returns `U8` ends with a `return`, and one with none is refused at the `}`
    // that closes its body, wherever that is.
    let mut examples = Examples::new();
    let missing = |line| Err(problem_at(MissingReturn(IntegerType::U8), line, 1));
    examples.add(&main_of("U8", ""), missing(2));
    examples.add(&main_of("U8", "    // return 3\n"), missing(3));
    examples.add(&main_of("U8", "    // a\n\n    // b\n"), missing(5));
    check(examples);
}

#[test]
fn the_first_problem_wins() {
    // The functions are checked in the order they're in the source, and each has one fault at
    // most, so the first in the file is the one reported, whatever its kind.
    let mut examples = Examples::new();
    let fine = main_of("U8", "    return 1\n");
    let too_large = main_of("U8", "    return 256\n");
    let missing = main_of("U8", "");
    let void_returns = main_of("Void", "    return 1\n");
    examples
        .add(&format!("{too_large}{missing}"), Err(problem_at(DoesntFit(IntegerType::U8), 2, 12)));
    examples.add(
        &format!("{missing}{too_large}"),
        Err(problem_at(MissingReturn(IntegerType::U8), 2, 1)),
    );
    examples.add(&format!("{fine}{void_returns}"), Err(problem_at(ReturnInVoid, 5, 5)));
    let both = format!("{fine}{}", main_of("Void", ""));
    examples.add(&both, Ok(vec![Returned::U8(1), Returned::Void]));
    check(examples);
}

#[test]
fn every_source_owns_what_it_gives_back() {
    // Sources fitted one after another, each kept while the next is fitted. Each still holds what
    // its own functions give back at the end.
    let sources: Vec<String> =
        (0_u32..=255).map(|value| main_of("U8", &format!("    return {value}\n"))).collect();
    let kept: Vec<_> =
        sources.iter().map(|source| check_fit(types_of(source)).expect("it fits")).collect();
    for (value, fitted) in (0_u8..=255).zip(&kept) {
        assert_eq!(fitted.as_slice(), [Returned::U8(value)], "for {value}");
    }
}

#[test]
fn fuzz_against_the_oracle() {
    // 20,000 random programs that pass the types, each fitted and checked against what the oracle
    // says of it. The outcomes are counted, so the test fails if the programs stop reaching one.
    let mut rng = Rng::new(0x4649_545F_5538_0001);
    let mut counts =
        [("passes", 0_u32), ("ReturnInVoid", 0), ("MissingReturn", 0), ("DoesntFit", 0)];
    let mut checks = Vec::new();
    for _ in 0_u32..20_000 {
        let source = source_of(&random_program(&mut rng));
        let expected = oracle(&source);
        let name = match &expected {
            Ok(_) => "passes",
            Err(Problem::At { kind: ReturnInVoid, .. }) => "ReturnInVoid",
            Err(Problem::At { kind: MissingReturn(_), .. }) => "MissingReturn",
            Err(Problem::At { kind: DoesntFit(_), .. }) => "DoesntFit",
            Err(other) => unreachable!("the fit finds no {other:?}"),
        };
        let (_, count) = counts.iter_mut().find(|(each, _)| *each == name).expect("a count");
        *count += 1;
        checks.push((source, expected));
    }
    // The fuzzing only means something if the programs reach every outcome, often. Each floor is
    // about half of what the seed reaches.
    let floors = [1_250, 3_500, 1_700, 3_400];
    for ((name, count), floor) in counts.into_iter().zip(floors) {
        assert!(count > floor, "only {count} reached {name}");
    }
    for (source, expected) in checks {
        assert_eq!(fitted(&source), expected, "for {}", excerpt(&source));
    }
}
