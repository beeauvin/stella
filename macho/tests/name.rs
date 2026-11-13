// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for a binary's name. A name comes from outside the compiler, so what's wrong with
//! one is an error, and every test here checks what `BinaryName::new` returns.
//!
//! An oracle states the rule apart from the code: `PLAIN` lists, one by one, the 93 characters a
//! name can hold, and a name is refused for the first of three reasons, in the order the errors
//! are checked. Every character there is, is tried against it on its own, every length around
//! the limit is tried, and a seeded fuzz test checks random names made of plain and refused
//! pieces. Each problem's sentence is pinned exactly, because people read them.

#![forbid(unsafe_code)]

use std::ptr;

use stella_macho::BinaryNameProblem::{Empty, NotPlain, TooLong};
use stella_macho::{BinaryName, BinaryNameProblem, NAME_BYTES_MAX};
use stella_test_support::Rng;

/// These are the characters a name can hold, each written out: every ASCII character that leaves
/// a mark, in ASCII's order, but `/`, which would make the name a path. A space leaves no mark,
/// so it isn't here.
const PLAIN: &str = concat!(
    "!\"#$%&'()*+,-.",
    "0123456789",
    ":;<=>?@",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "[\\]^_`",
    "abcdefghijklmnopqrstuvwxyz",
    "{|}~",
);

/// This is the longest a name can be, in bytes, stated here on its own: a file's name is at most
/// 255 bytes on macOS, and a source file's ends with the 7 of `.stella`.
const LONGEST_BYTES: usize = 255 - 7;

/// These are what a random name is made of. Most are plain. The rest are each refused for not
/// being plain: `/`, a space, a tab, a line's end, NUL, DEL, and characters that aren't ASCII, of
/// two, three and four bytes.
const PIECES: &[&str] = &[
    "a",
    "Z",
    "0",
    "hello",
    "-",
    "_",
    ".",
    "~",
    "!",
    "{|}",
    "\\",
    "/",
    " ",
    "\t",
    "\n",
    "\0",
    "\u{7F}",
    "é",
    "\u{2028}",
    "\u{1F3F3}",
];

/// How many random names got each answer, so the fuzzing can't stop reaching one unnoticed.
#[derive(Debug, Default)]
struct AnswerCounts {
    named: u32,
    empty: u32,
    too_long: u32,
    not_plain: u32,
}

/// Says what `BinaryName::new` should make of `text`, worked out apart from it: how many bytes
/// the name takes, or the first thing wrong with it. No bytes is `Empty`. More than the longest
/// is `TooLong`, whatever the bytes are. Then any character that isn't in `PLAIN` is `NotPlain`.
fn expected_of(text: &str) -> Result<u32, BinaryNameProblem> {
    let size_bytes = text.len();
    if size_bytes == 0 {
        return Err(Empty);
    }
    if size_bytes > LONGEST_BYTES {
        return Err(TooLong);
    }
    if text.chars().all(|character| PLAIN.contains(character)) {
        Ok(u32::try_from(size_bytes).expect("at most 248 fits in a `u32`"))
    } else {
        Err(NotPlain)
    }
}

/// Checks `text` against the oracle: `BinaryName::new` refuses it for the reason the oracle
/// gives, or makes a name that holds the very text it was given, and measures it in bytes.
/// Returns what the oracle said.
fn check(text: &str) -> Result<u32, BinaryNameProblem> {
    let expected = expected_of(text);
    match (BinaryName::new(text), expected) {
        (Ok(name), Ok(size_bytes)) => {
            assert_eq!(name.as_str(), text, "a name holds the text it was given");
            assert!(ptr::eq(name.as_str(), text), "a name borrows its text, and copies nothing");
            assert_eq!(name.size_bytes(), size_bytes, "the size of {text:?}, in bytes");
        }
        (Err(problem), Err(expected_problem)) => {
            assert_eq!(problem, expected_problem, "what's wrong with {text:?}");
        }
        (found @ Ok(_), Err(_)) | (found @ Err(_), Ok(_)) => {
            unreachable!("{text:?} should give {expected:?}, not {found:?}");
        }
    }
    expected
}

/// Returns the sentence `problem` should print, each written out whole. The match names every
/// problem, so a new one can't go without its sentence.
const fn sentence_of(problem: BinaryNameProblem) -> &'static str {
    match problem {
        Empty => "a binary's name can't be empty.",
        TooLong => "a binary's name can be at most 248 bytes.",
        NotPlain => "a binary's name can hold only ASCII, with no space and no `/`.",
    }
}

#[test]
fn the_oracle_states_the_rule() {
    // The oracle is checked before the code is: `PLAIN` is the 94 ASCII characters that leave a
    // mark, less `/`, each once and in order, and the longest name is the limit the crate gives.
    assert_eq!(PLAIN.len(), 93);
    assert!(PLAIN.is_ascii());
    assert!(PLAIN.as_bytes().is_sorted_by(|before, after| before < after), "each once, in order");
    assert_eq!(PLAIN.chars().next(), Some('!'));
    assert_eq!(PLAIN.chars().last(), Some('~'));
    assert!(!PLAIN.contains('/'));
    assert_eq!(usize::try_from(NAME_BYTES_MAX), Ok(LONGEST_BYTES));
    assert_eq!(NAME_BYTES_MAX, 248);
    // What the oracle says of the names the tests lean on, by hand.
    assert_eq!(expected_of("hello"), Ok(5));
    assert_eq!(expected_of(""), Err(Empty));
    assert_eq!(expected_of("a/b"), Err(NotPlain));
    assert_eq!(expected_of("a b"), Err(NotPlain));
    assert_eq!(expected_of(&"a".repeat(248)), Ok(248));
    assert_eq!(expected_of(&"a".repeat(249)), Err(TooLong));
}

#[test]
fn every_character() {
    // Every character there is, as a name of its own: the 93 plain ones are each a name of one
    // byte, and every other is refused for not being plain. That's every control character, the
    // space, `/`, DEL, and everything that isn't ASCII.
    let mut named_count: u32 = 0;
    let mut refused_count: u32 = 0;
    for character in '\0'..=char::MAX {
        let mut buffer = [0_u8; 4];
        let text: &str = character.encode_utf8(&mut buffer);
        match check(text) {
            Ok(size_bytes) => {
                assert_eq!(size_bytes, 1, "a plain character is one byte: {character:?}");
                named_count += 1;
            }
            Err(problem) => {
                assert_eq!(problem, NotPlain, "for {character:?}");
                refused_count += 1;
            }
        }
    }
    assert_eq!(named_count, 93);
    // There are 0x11_0000 code points, and 0x800 of them are surrogates, which aren't characters.
    assert_eq!(refused_count, 0x11_0000 - 0x800 - 93);
    // The ones on either side of the plain characters, and the one left out of them, by hand.
    for refused in ["\0", "\u{1F}", " ", "/", "\u{7F}", "\u{80}", "é", "\u{FFFD}", "\u{10FFFF}"] {
        assert_eq!(BinaryName::new(refused), Err(NotPlain), "for {refused:?}");
    }
    for named in ["!", ".", "0", "A", "a", "~"] {
        assert_eq!(BinaryName::new(named).map(|name| name.as_str()), Ok(named));
    }
}

#[test]
fn a_character_anywhere_in_a_name() {
    // One character that isn't plain refuses a name wherever it is: first, in the middle, or
    // last. Every character of one or two bytes is tried in each place, between plain ones.
    for character in '\0'..='\u{7FF}' {
        let is_plain = PLAIN.contains(character);
        for text in
            [format!("{character}name"), format!("na{character}me"), format!("name{character}")]
        {
            let expected = check(&text);
            assert_eq!(expected.is_ok(), is_plain, "for {text:?}");
        }
    }
    // Every plain character at once is a name, and it's still one with any of them doubled.
    assert_eq!(check(PLAIN), Ok(93));
    for character in PLAIN.chars() {
        assert_eq!(check(&format!("{PLAIN}{character}")), Ok(94), "for {character:?}");
    }
}

#[test]
fn every_length() {
    // Names of one plain character over and over, at every length from none to 300 bytes: none
    // is empty, 1 to 248 are names of that many bytes, and 249 on are too long. The first and
    // last characters of `PLAIN` are both tried, so the limit isn't one character's.
    for character in ["!", "a", "~"] {
        for size_bytes in 0..=300 {
            let text = character.repeat(size_bytes);
            let found = BinaryName::new(&text).map(|name| name.size_bytes());
            assert_eq!(check(&text), found, "{size_bytes} of {character:?}");
            let expected = match size_bytes {
                0 => Err(Empty),
                1..=248 => Ok(u32::try_from(size_bytes).expect("at most 248 fits in a `u32`")),
                _ => Err(TooLong),
            };
            assert_eq!(found, expected, "{size_bytes} of {character:?}");
        }
    }
    // The edges, by hand: the shortest name, the longest, and one byte more.
    assert_eq!(BinaryName::new("a").map(|name| name.size_bytes()), Ok(1));
    assert_eq!(BinaryName::new(&"a".repeat(248)).map(|name| name.size_bytes()), Ok(248));
    assert_eq!(BinaryName::new(&"a".repeat(249)), Err(TooLong));
    assert_eq!(BinaryName::new(""), Err(Empty));
}

#[test]
fn a_name_is_measured_in_bytes() {
    // `é` is one character of two bytes. 124 of them are 248 bytes, which fits, so the name is
    // refused for what it holds. 125 are 250 bytes, which is too long, though it's far fewer
    // than 248 characters.
    assert_eq!(check(&"é".repeat(124)), Err(NotPlain));
    assert_eq!(check(&"é".repeat(125)), Err(TooLong));
    // A character of four bytes: 62 fit, and 63 don't.
    assert_eq!(check(&"\u{1F3F3}".repeat(62)), Err(NotPlain));
    assert_eq!(check(&"\u{1F3F3}".repeat(63)), Err(TooLong));
}

#[test]
fn the_first_problem_is_the_one_reported() {
    // A name with two things wrong is refused for its length, which is checked before what it
    // holds: 249 bytes of `/`, of spaces, or of plain characters with one that isn't.
    for text in ["/".repeat(249), " ".repeat(249), format!("{}\n", "a".repeat(248))] {
        assert_eq!(check(&text), Err(TooLong), "for {} bytes", text.len());
    }
    // At the longest length the same names are refused for what they hold.
    for text in ["/".repeat(248), " ".repeat(248), format!("{}\n", "a".repeat(247))] {
        assert_eq!(check(&text), Err(NotPlain), "for {} bytes", text.len());
    }
}

#[test]
fn sentences() {
    // Each problem prints its sentence, written out whole here. The sentence for a name that's
    // too long says the limit, so it's checked against the limit too.
    for problem in [Empty, TooLong, NotPlain] {
        assert_eq!(problem.to_string(), sentence_of(problem), "for {problem:?}");
    }
    assert!(sentence_of(TooLong).contains(&NAME_BYTES_MAX.to_string()));
    // A refused name prints its problem's sentence, as the command prints it.
    let refused = BinaryName::new("a/b").expect_err("a name can't hold `/`");
    assert_eq!(
        refused.to_string(),
        "a binary's name can hold only ASCII, with no space and no `/`."
    );
}

#[test]
fn names_compare_by_their_text() {
    // A name is a value: two made from the same text are equal, a copy is equal to what it was
    // copied from, and names of different text aren't equal. Problems compare the same way.
    let hello = BinaryName::new("hello").expect("`hello` is a name");
    let copy = hello;
    assert_eq!(hello, copy);
    assert_eq!(Ok(hello), BinaryName::new(&String::from("hello")));
    assert_ne!(Ok(hello), BinaryName::new("world"));
    assert_ne!(Ok(hello), BinaryName::new("Hello"), "a name's case is part of it");
    assert_eq!(BinaryName::new(""), BinaryName::new(""));
    assert_ne!(BinaryName::new(""), BinaryName::new("/"));
}

#[test]
fn fuzz_against_the_oracle() {
    // 20,000 random names, each made of plain and refused pieces, with lengths drawn to land on
    // either side of the limit often. Each is checked against the oracle, and every answer is
    // counted, so the test fails if the names stop reaching one.
    let mut rng = Rng::new(0x4E41_4D45_0000_0001);
    let mut counts = AnswerCounts::default();
    for _ in 0..20_000_u32 {
        // One name in four is all one plain piece, so that long names are often plain.
        let plain_only = rng.below(4) == 0;
        let size_bytes_target = match rng.below(4) {
            0 => rng.below(4),
            1 => 244 + rng.below(10),
            _ => rng.below(300),
        };
        let mut text = String::new();
        while text.len() < size_bytes_target {
            text.push_str(if plain_only { "a" } else { rng.pick(PIECES) });
        }
        match check(&text) {
            Ok(_) => counts.named += 1,
            Err(Empty) => counts.empty += 1,
            Err(TooLong) => counts.too_long += 1,
            Err(NotPlain) => counts.not_plain += 1,
        }
    }
    assert!(counts.named >= 1_000, "{counts:?}");
    assert!(counts.empty >= 1_000, "{counts:?}");
    assert!(counts.too_long >= 1_000, "{counts:?}");
    assert!(counts.not_plain >= 1_000, "{counts:?}");
}
