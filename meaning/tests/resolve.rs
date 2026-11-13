// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the types. They're a crate of their own, so the compiler lets them reach only
//! what the crates make public: every source goes through `lint`, `lex`, `lint_tokens`, `nest`,
//! `parse` and `build`, and then `resolve`.
//!
//! Every source here passes the grammar, so it reaches the types, and each row says what they
//! should make of it: the type of each function, or the problem and where it is. An oracle works
//! the same out from each function's first line alone, and every row checks the oracle before
//! `resolve` sees any. The oracle finds a type in a table of the tests' own, so each type is
//! tried by its name, and so is every name one step from it. Every program of a few functions,
//! from a few names and types, is tried, and a seeded fuzz test checks random programs against
//! the oracle. The largest sources sit at each limit a source of 16,384 bytes has.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::ptr;

use stella_ast::{Ast, build};
use stella_blocks::nest;
use stella_grammar::parse;
use stella_lexer::lex;
use stella_linter_text::lint;
use stella_linter_tokens::lint_tokens;
use stella_meaning::Kind::NoSuchType;
use stella_meaning::Type::Void;
use stella_meaning::{Problem, Type, Types, resolve};
use stella_source::{Position, SOURCE_BYTES_MAX, u32_from_usize, usize_from_u32};
use stella_test_support::{Example, Examples, Rng, excerpt, indent, programs_of, source_of};

/// What the types should make of a source: the type of each function, or the first problem.
type Outcome = Result<Vec<Type>, Problem>;

/// This is the smallest program, laid out the one way.
const SMALLEST: &str = "function main() returns Void {\n}\n";

/// Every type there is, each with the name the types page gives it.
const TYPES: [(Type, &str); 1] = [(Void, "Void")];

/// These are the items a program is made of, in `every_program`.
const ITEMS: [&str; 4] = [
    "// a comment",
    "function main() returns Void {",
    "function main() returns Bar {",
    "function a() returns Void {",
];

/// These are the capitals, one of which starts each word of a type's name.
const CAPITALS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// These are the characters that follow a capital in a type's name.
const LOWERCASE_AND_DIGITS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// These are a function's names.
const NAMES: &[&str] = &["main", "a", "x_2", "my_function"];

/// These are names close to `Void`, which are no more a type than any other.
const NEAR_VOID: &[&str] =
    &["V", "Vo", "Voi", "Voids", "Void2", "VoidVoid", "Avoid", "Vvoid", "VoiD", "VoId", "Xoid"];

/// Returns `found`'s row in `TYPES`. It names every type, so a type that's added to `Type` stops
/// the tests building until it's given a row, and with the row the name the oracle finds it by.
const fn row_of(found: Type) -> usize {
    match found {
        Void => 0,
    }
}

/// Says whether `source` passes every check through the grammar, so it reaches the types.
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

/// Returns the AST of `source`, which must pass the grammar.
fn ast_of(source: &str) -> Ast<'_> {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    let tokens = lex(text).expect("source should lex");
    let layout = lint_tokens(tokens).expect("source should be laid out the one way");
    build(parse(nest(layout)).expect("source should pass the grammar"))
}

/// Resolves `source`, which must pass the grammar, and returns what the types made of it. On the
/// way, it checks that what `resolve` returns holds the very AST `build` made, and that its types
/// are its functions'.
fn resolved(source: &str) -> Outcome {
    let ast = ast_of(source);
    let functions = ptr::from_ref(ast.functions());
    let types = resolve(ast)?;
    assert!(ptr::eq(types.ast().functions(), functions), "it holds the AST `build` made");
    assert_types_are_the_functions(&types);
    Ok(types.as_slice().to_vec())
}

/// Asserts that `types` holds one type for each of its functions, and that each type's name is
/// its function's return type, as the source writes it.
fn assert_types_are_the_functions(types: &Types<'_>) {
    let (ast, each) = (types.ast(), types.as_slice());
    assert_eq!(each.len(), ast.functions().len(), "each function has one type");
    for (function, found) in ast.functions().iter().zip(each) {
        let written = ast.text(ast.return_type(function));
        assert_eq!(found.name(), written, "a type's name is its function's return type");
    }
}

/// Returns the problem of a type that doesn't exist, at `line` and `column`.
// KS-DISTINCT-ARGS: a line then a column is the order every message prints them in, and an
// expectation with the two swapped fails its test.
const fn no_such_type_at(line: u32, column: u32) -> Problem {
    Problem::At { kind: NoSuchType, position: Position::on_line(line).at_column(column) }
}

/// Returns the line of the type's name that `problem` refuses.
fn refused_line(problem: &Problem) -> u32 {
    match problem {
        Problem::At { position, .. } => position.line(),
        Problem::NoMain => unreachable!("only the program is refused as a whole, never a type"),
    }
}

/// Checks that every example passes the grammar, as the types need, and that the oracle agrees
/// with it. Then checks every one against `resolve`. So every example checks the oracle before
/// `resolve` sees any, whatever `resolve` does.
fn check(examples: Examples<Outcome>) {
    let examples = examples.take();
    for Example { source, expected, at } in &examples {
        let excerpt = excerpt(source);
        assert!(parses(source), "{at}: {excerpt} should pass the grammar");
        assert_eq!(oracle(source), *expected, "{at}: the oracle, for {excerpt}");
    }
    for Example { source, expected, at } in &examples {
        assert_eq!(resolved(source), *expected, "{at}: for {}", excerpt(source));
    }
}

/// Returns a function named `name` that returns `return_type`, with an empty body.
// KS-DISTINCT-ARGS: a name then a return type is the order a function's first line has them in,
// and a source with the two swapped doesn't pass the grammar, which every source here must.
fn function(name: &str, return_type: &str) -> String {
    format!("function {name}() returns {return_type} {{\n}}\n")
}

/// Works out what the types should make of `source` from its text alone. `source` must pass the
/// grammar, so a line that starts with `function ` is a function's first line, laid out the one
/// way, and its return type runs from after `() returns ` to ` {`. That's a type when it's the
/// whole of a name in `TYPES`, and otherwise it's refused where it starts. Columns are counted
/// in bytes, since code is ASCII.
fn oracle(source: &str) -> Outcome {
    let mut types = Vec::new();
    for (content, number) in source.split_terminator('\n').zip(1_u32..) {
        let Some(rest) = content.strip_prefix("function ") else {
            continue;
        };
        let (_, rest) = rest.split_once("() returns ").expect("a function's first line");
        let return_type = rest.strip_suffix(" {").expect("a function's first line ends in `{`");
        let Some(&(found, _)) = TYPES.iter().find(|(_, name)| *name == return_type) else {
            let column = content.len() - " {".len() - return_type.len() + 1;
            return Err(no_such_type_at(number, u32_from_usize(column)));
        };
        types.push(found);
    }
    Ok(types)
}

/// Returns the names one step from `name`, a type's: `name` cut short, with one character taken
/// out, with one character changed to any other letter or digit, so to its other case too, with
/// one more character after it, with a word before it, and written twice. Not all of them are a
/// type's name to the lexer.
fn names_near(name: &str) -> BTreeSet<String> {
    let characters: Vec<char> = name.chars().collect();
    let others = || CAPITALS.iter().chain(LOWERCASE_AND_DIGITS).copied().map(char::from);
    let mut near = BTreeSet::new();
    for index in 0..characters.len() {
        // The name cut short before this character, when that leaves any of it.
        if index > 0 {
            near.insert(characters.iter().take(index).collect());
        }
        // The name without this character.
        let mut shorter = characters.clone();
        let _: char = shorter.remove(index);
        near.insert(shorter.iter().collect());
        // The name with this character changed.
        for other in others() {
            let mut changed = characters.clone();
            changed[index] = other;
            near.insert(changed.iter().collect());
        }
    }
    // The name with one more character after it, with a word before it, and written twice.
    for other in others() {
        near.insert(format!("{name}{other}"));
        near.insert(format!("A{other}{name}"));
    }
    near.insert(format!("{name}{name}"));
    // Changing a character to itself is no step at all.
    assert!(near.remove(name), "each character was changed to every character, so to itself");
    near
}

/// Returns `count` random characters from `alphabet`.
fn random_run(rng: &mut Rng, alphabet: &[u8], count: usize) -> String {
    std::iter::repeat_with(|| char::from(alphabet[rng.below(alphabet.len())])).take(count).collect()
}

/// Returns a random type's name in Pascal case, and now and then a long one: words that each start
/// with one capital, then lowercase letters and digits. Every word but the last has at least one
/// after its capital, so two capitals never come in a row.
fn random_type(rng: &mut Rng) -> String {
    let words_count = if rng.below(4) == 0 { 60 + rng.below(40) } else { 1 + rng.below(3) };
    let mut name = String::new();
    for word in 0..words_count {
        name.push_str(&random_run(rng, CAPITALS, 1));
        let least = usize::from(word + 1 < words_count);
        let length = least + rng.below(5 - least);
        name.push_str(&random_run(rng, LOWERCASE_AND_DIGITS, length));
    }
    name
}

/// Returns a function's return type: mostly `Void`, sometimes a name close to it, and sometimes
/// any type's name.
fn random_return_type(rng: &mut Rng) -> String {
    match rng.below(20) {
        0 => String::from(rng.pick(NEAR_VOID)),
        1 => random_type(rng),
        _ => String::from("Void"),
    }
}

/// Returns a random program that passes the grammar, as its lines: functions with random names
/// and return types, with comments and blank lines in their bodies, and comments and blank lines
/// outside them.
fn random_program(rng: &mut Rng) -> Vec<String> {
    let mut lines = Vec::new();
    for _ in 0..=rng.below(6) {
        match rng.below(10) {
            0 => lines.push(String::new()),
            1 | 2 => lines.push(String::from("// returns Bar")),
            _ => {
                let return_type = random_return_type(rng);
                lines.push(format!("function {}() returns {return_type} {{", rng.pick(NAMES)));
                for _ in 0..rng.below(3) {
                    lines.push(format!("{}// returns Bar", indent(1)));
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

/// Returns the outcome `expected` is, as the fuzzing counts them.
fn reached_outcome(expected: &Outcome) -> &'static str {
    match expected {
        Ok(types) if types.is_empty() => "passes, with no function",
        Ok(_) => "passes",
        Err(problem) if refused_line(problem) > 1 => "refused, past line 1",
        Err(_) => "refused on line 1",
    }
}

#[test]
fn the_smallest_program() {
    // The smallest program's one function returns `Void`.
    let mut examples = Examples::new();
    examples.add(SMALLEST, Ok(vec![Void]));
    check(examples);
}

#[test]
fn comments() {
    // A comment names no type, even one that reads like a function's first line, so a source of
    // only comments has no function and nothing to find. Whether it's a program is the next
    // check's.
    let mut examples = Examples::new();
    examples.add("// only\n", Ok(vec![]));
    examples.add("// function main() returns Bar {\n", Ok(vec![]));
    examples.add(
        "// a\n\nfunction main() returns Void {\n    // returns Bar\n}\n// b\n",
        Ok(vec![Void]),
    );
    check(examples);
}

#[test]
fn every_type_has_one_name() {
    // Each type is in the tests' table once, in its own row, with the name the types page gives
    // it. `name` returns that name, no other type has it, and a function that returns it returns
    // that type.
    let mut examples = Examples::new();
    for (index, (found, name)) in TYPES.into_iter().enumerate() {
        assert_eq!(row_of(found), index, "{found:?} is in its own row");
        assert_eq!(found.name(), name, "for {found:?}");
        let named_count = TYPES.iter().filter(|(_, other)| *other == name).count();
        assert_eq!(named_count, 1, "only {found:?} is named `{name}`");
        examples.add(&function("main", name), Ok(vec![found]));
    }
    check(examples);
}

#[test]
fn names_near_a_type() {
    // A type is found by its whole name, and by its case. So a name one step from a type's is no
    // more a type than `Bar` is: cut short, missing a character, with a character changed, even
    // only in its case, or with more before or after it. Every such name that's still a type's
    // name to the lexer is tried, and they're counted, so the test can't pass by trying none.
    let mut examples = Examples::new();
    let mut near_count: u32 = 0;
    for (_, name) in TYPES {
        for near in names_near(name) {
            let source = function("main", &near);
            if parses(&source) {
                examples.add(&source, Err(no_such_type_at(1, 25)));
                near_count += 1;
            }
        }
    }
    // `Void` has 286. Cut short, it's `V`, `Vo` and `Voi`, and missing a character it's `Vid`
    // and `Vod` too, since `oid` is no type's name. Its `V` changes to the 25 other capitals.
    // Its `o` changes to the 35 other lowercase letters and digits, since a capital there would
    // follow another. Its `i` and its `d` each change to the 61 other letters and digits. Any of
    // the 62 can follow it. The word before it is `A` and one of the 36 lowercase letters and
    // digits. And there's `VoidVoid`.
    assert_eq!(near_count, 3 + 2 + 25 + 35 + 61 + 61 + 62 + 36 + 1, "the names near a type");
    assert_eq!(near_count, 286, "the names near a type, added up");
    check(examples);
}

#[test]
fn unknown_types() {
    // A type's name is found whole, so a name that's almost `Void` is no more a type than `Bar`.
    let mut examples = Examples::new();
    let near = [
        "Bar", "V", "Vo", "Voi", "Voids", "Void2", "Vo1d", "VoidVoid", "Avoid", "Vvoid", "VoiD",
        "VoId", "Xoid", "Vaid", "Voix",
    ];
    for return_type in near {
        examples.add(&function("main", return_type), Err(no_such_type_at(1, 25)));
    }
    // The problem is at the type's name, wherever the function's name puts it.
    examples.add(&function("a", "Bar"), Err(no_such_type_at(1, 22)));
    examples.add(&function("my_function", "MessageQueue"), Err(no_such_type_at(1, 32)));
    check(examples);
}

#[test]
fn many_functions() {
    // Every function's type is found, in order, whatever the functions are named.
    let mut examples = Examples::new();
    examples.add(&format!("{SMALLEST}{SMALLEST}"), Ok(vec![Void, Void]));
    let names = ["a", "main", "x_2", "my_function", "main"];
    let source: String = names.iter().map(|name| function(name, "Void")).collect();
    examples.add(&source, Ok(vec![Void; 5]));
    let spaced = format!("{}\n// between\n{}\n\n{}", function("a", "Void"), SMALLEST, SMALLEST);
    examples.add(&spaced, Ok(vec![Void; 3]));
    check(examples);
}

#[test]
fn the_first_problem_wins() {
    // The functions are looked up in order, so the first type that doesn't exist is the one
    // reported, however many follow it.
    let mut examples = Examples::new();
    let two = format!("{}{}", function("a", "Bar"), function("b", "Baz"));
    examples.add(&two, Err(no_such_type_at(1, 22)));
    let second = format!("{}{}", function("a", "Void"), function("b", "Baz"));
    examples.add(&second, Err(no_such_type_at(3, 22)));
    let later =
        format!("{}// x\n\n{}{}", function("a", "Void"), function("b", "Void"), function("c", "Q"));
    examples.add(&later, Err(no_such_type_at(7, 22)));
    check(examples);
}

#[test]
fn the_largest_sources() {
    // Each limit goes as far as the largest source lets it, and one more doesn't fit. So nothing
    // the types count can be counted in anything smaller.
    let mut examples = Examples::new();
    let size = usize_from_u32(SOURCE_BYTES_MAX);
    // The most functions that return `Void`.
    let void = function("a", "Void");
    assert!(void.repeat(546).len() <= size);
    assert!(void.repeat(547).len() > size);
    examples.add(&void.repeat(546), Ok(vec![Void; 546]));
    // The most functions there can be, every one returning a type that doesn't exist: the first
    // is the one reported.
    let shortest = function("a", "B");
    assert!(shortest.repeat(606).len() <= size);
    assert!(shortest.repeat(607).len() > size);
    examples.add(&shortest.repeat(606), Err(no_such_type_at(1, 22)));
    // The most functions that return `Void`, then one that returns a type that doesn't exist.
    let last_unknown = |count: usize| format!("{}{shortest}", void.repeat(count));
    assert!(last_unknown(545).len() <= size);
    assert!(last_unknown(546).len() > size);
    examples.add(&last_unknown(545), Err(no_such_type_at(1_091, 22)));
    // The longest type that doesn't exist.
    let typed = |length: usize| function("a", &format!("B{}", "b".repeat(length)));
    let type_most = size - typed(0).len();
    assert_eq!(type_most + 1, 16_358, "the longest type, with its capital");
    assert!(typed(type_most + 1).len() > size);
    examples.add(&typed(type_most), Err(no_such_type_at(1, 22)));
    // A type that doesn't exist after the most comments.
    let after = |count: usize| format!("{}{shortest}", "//\n".repeat(count));
    assert!(after(5_452).len() <= size);
    assert!(after(5_453).len() > size);
    examples.add(&after(5_452), Err(no_such_type_at(5_453, 22)));
    // A type that doesn't exist on the last line a function can be on, and `Void` on the last line
    // a function that returns it can be on.
    let far = format!("{}{shortest}", "\n".repeat(size - shortest.len()));
    assert_eq!(far.len(), size);
    examples.add(&far, Err(no_such_type_at(16_358, 22)));
    let far = format!("{}{void}", "\n".repeat(size - void.len()));
    assert_eq!(far.len(), size);
    examples.add(&far, Ok(vec![Void]));
    check(examples);
}

#[test]
fn every_source_owns_its_types() {
    // Sources resolved one after another, the most types a source can hold among them, with
    // a refused one between, and every one's types kept while the next is resolved. Each
    // source's types are still what the oracle says of it at the end, whatever was resolved
    // after it. There's only one type, so the types alone would only show how many there are:
    // each is also still named by its own function's return type, read from the source that
    // was kept with it.
    let most = function("a", "Void").repeat(546);
    let refused = function("a", "Bar");
    let sources = [SMALLEST, &most, &refused, "// x\n", &most, &refused, SMALLEST];
    let mut kept = Vec::new();
    for source in sources {
        match resolve(ast_of(source)) {
            Ok(types) => kept.push((source, types)),
            Err(problem) => {
                assert_eq!((source, problem), (refused.as_str(), no_such_type_at(1, 22)));
            }
        }
    }
    assert_eq!(kept.len(), 5, "every source but the two refused ones resolved");
    for (source, types) in &kept {
        assert_eq!(oracle(source), Ok(types.as_slice().to_vec()), "for {}", excerpt(source));
        assert_types_are_the_functions(types);
    }
}

#[test]
fn every_program() {
    // Every program of one to five items, so every way a type that doesn't exist can come before
    // or after one that does, or be the only one, is tried.
    let sources = programs_of(&ITEMS, 5);
    assert_eq!(sources.len(), 1_364, "4 + 16 + 64 + 256 + 1,024 programs");
    // Each passes the grammar, and the oracle's outcomes are counted exactly, before anything is
    // resolved.
    let mut reached: BTreeMap<String, u32> = BTreeMap::new();
    let mut expected = Vec::new();
    for source in &sources {
        assert!(parses(source), "{} should pass the grammar", excerpt(source));
        let outcome = oracle(source);
        let key = match &outcome {
            Ok(types) => format!("{} types", types.len()),
            Err(problem) => format!("at {}", refused_line(problem)),
        };
        *reached.entry(key).or_default() += 1;
        expected.push(outcome);
    }
    let counts: Vec<(&str, u32)> =
        reached.iter().map(|(key, &count)| (key.as_str(), count)).collect();
    let expected_counts = [
        ("0 types", 5),
        ("1 types", 30),
        ("2 types", 80),
        ("3 types", 120),
        ("4 types", 96),
        ("5 types", 32),
        ("at 1", 341),
        ("at 2", 85),
        ("at 3", 191),
        ("at 4", 89),
        ("at 5", 115),
        ("at 6", 68),
        ("at 7", 64),
        ("at 8", 32),
        ("at 9", 16),
    ];
    assert_eq!(counts, expected_counts, "the outcomes, counted");
    for (source, expected) in sources.iter().zip(expected) {
        assert_eq!(resolved(source), expected, "for {}", excerpt(source));
    }
}

#[test]
fn fuzz_against_the_oracle() {
    // 20,000 random programs that pass the grammar, each resolved and checked against what the
    // oracle says of it. The outcomes are counted, so the test fails if the programs stop
    // reaching one.
    let mut rng = Rng::new(0x5459_5045_535F_3031);
    let mut reached: BTreeMap<&str, u32> = BTreeMap::new();
    let mut checks = Vec::new();
    for _ in 0_u32..20_000 {
        let source = source_of(&random_program(&mut rng));
        assert!(parses(&source), "the generator passes the grammar with {}", excerpt(&source));
        let expected = oracle(&source);
        *reached.entry(reached_outcome(&expected)).or_default() += 1;
        if source.lines().any(|line| line.starts_with("function ") && line.len() > 300) {
            *reached.entry("a type longer than a byte counts").or_default() += 1;
        }
        checks.push((source, expected));
    }
    // The fuzzing only means something if the sources reach every outcome, often. This is checked
    // before anything is resolved, so it holds whatever `resolve` does. Each floor is about half of
    // what the seed reaches.
    let reached_by = |key: &str| reached.get(key).copied().unwrap_or(0);
    for (outcome, floor) in [
        ("passes", 7_000),
        ("passes, with no function", 700),
        ("refused on line 1", 650),
        ("refused, past line 1", 1_450),
        ("a type longer than a byte counts", 160),
    ] {
        let count = reached_by(outcome);
        assert!(count > floor, "only {count} reached {outcome}; reached {reached:?}");
    }
    // The oracle made these answers, so each goes straight to `resolve`.
    for (source, expected) in checks {
        assert_eq!(resolved(&source), expected, "for {}", excerpt(&source));
    }
}
