// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit and fuzz tests for the path lint. They're a crate of their own, so the compiler lets them
//! reach only what the crate makes public: a path in, a `SourcePath` or a `Problem` out.
//!
//! A name holds only 37 characters, so every code point is tried, in a file's name and in a
//! directory's. The rest of the rules are two tables, one accepted and one refused. A seeded fuzz
//! test checks random paths against an oracle, which states the path lint as plainly as the
//! reference does. Every example checks the oracle too.

use stella_linter_text::Kind::*;
use stella_linter_text::{Kind, Place, Problem, lint_path};

/// These are the 37 characters a name can hold.
const NAME_CHARACTERS: &str = "abcdefghijklmnopqrstuvwxyz0123456789-";

/// Lints `path`, which must pass, and checks what it normalizes to and the program's name. The
/// oracle must agree first.
fn accepts(path: &str, normalized: &str, name: &str) {
    assert_eq!(oracle(path), Ok((normalized, name)), "the oracle, for {path:?}");
    let source_path = lint_path(path).expect("path should pass the path lint");
    assert_eq!(source_path.normalized(), normalized, "for {path:?}");
    assert_eq!(source_path.name(), name, "for {path:?}");
}

/// Lints `path`, which must be refused for `kind`, a problem with the file as a whole. The oracle
/// must agree first.
fn refuses(path: &str, kind: Kind) {
    assert_eq!(oracle(path), Err(kind), "the oracle, for {path:?}");
    let problem = lint_path(path).expect_err("path should be refused");
    let expected = Problem { kind, place: Place::File };
    assert_eq!(problem, expected, "for {path:?}");
}

#[test]
fn every_code_point_in_a_files_name() {
    for character in char::MIN..=char::MAX {
        // `/` splits the path rather than sitting in a name, so the tables test it.
        if character == '/' {
            continue;
        }
        let name = format!("a{character}b");
        let path = format!("{name}.stella");
        if NAME_CHARACTERS.contains(character) {
            accepts(&path, &path, &name);
        } else {
            refuses(&path, NameNotKebab);
        }
    }
}

#[test]
fn every_code_point_in_a_directorys_name() {
    for character in char::MIN..=char::MAX {
        // `/` splits the path rather than sitting in a name, so the tables test it.
        if character == '/' {
            continue;
        }
        let path = format!("a{character}b/hello.stella");
        if NAME_CHARACTERS.contains(character) {
            accepts(&path, &path, "hello");
        } else {
            refuses(&path, DirectoryNotKebab);
        }
    }
}

#[test]
fn accepted_paths() {
    for (path, normalized, name) in [
        ("hello.stella", "hello.stella", "hello"),
        ("hello-world.stella", "hello-world.stella", "hello-world"),
        ("rung-0.stella", "rung-0.stella", "rung-0"),
        ("2048.stella", "2048.stella", "2048"),
        ("programs/hello.stella", "programs/hello.stella", "hello"),
        ("a/b/c/hello.stella", "a/b/c/hello.stella", "hello"),
        ("x86-64/utf8.stella", "x86-64/utf8.stella", "utf8"),
        ("./hello.stella", "hello.stella", "hello"),
        ("./programs/hello.stella", "programs/hello.stella", "hello"),
    ] {
        accepts(path, normalized, name);
    }
}

#[test]
fn refused_paths() {
    for (path, kind) in [
        ("/", PathAbsolute),
        ("/hello.stella", PathAbsolute),
        ("..", PathParent),
        ("../hello.stella", PathParent),
        ("programs/../hello.stella", PathParent),
        ("./../hello.stella", PathParent),
        ("", PathEmptyPart),
        ("./", PathEmptyPart),
        ("programs/", PathEmptyPart),
        ("programs//hello.stella", PathEmptyPart),
        (".//hello.stella", PathEmptyPart),
        (".", PathDotPart),
        ("programs/./hello.stella", PathDotPart),
        ("././hello.stella", PathDotPart),
        // Every directory is checked, not just the first or the last.
        ("A/b/c/hello.stella", DirectoryNotKebab),
        ("a/B/c/hello.stella", DirectoryNotKebab),
        ("a/b/C/hello.stella", DirectoryNotKebab),
        ("hello", NameNotStella),
        ("hello.Stella", NameNotStella),
        ("hello.stella.txt", NameNotStella),
        ("hellostella", NameNotStella),
        (".stella", NameEmpty),
        ("programs/.stella", NameEmpty),
        // A hyphen can't start or end a name, come twice in a row, or be the whole name.
        ("-hello.stella", NameNotKebab),
        ("hello-.stella", NameNotKebab),
        ("hello--world.stella", NameNotKebab),
        ("-.stella", NameNotKebab),
        ("-programs/hello.stella", DirectoryNotKebab),
        ("programs-/hello.stella", DirectoryNotKebab),
        ("my--programs/hello.stella", DirectoryNotKebab),
        ("--/hello.stella", DirectoryNotKebab),
        // `..` is a parent only as a whole part of the path.
        ("..hello.stella", NameNotKebab),
        ("..programs/hello.stella", DirectoryNotKebab),
        // The checks run in order, and each looks at every part before the next runs.
        ("/x/../Hello.txt", PathAbsolute),
        ("Programs/../hello.stella", PathParent),
        ("a//../b.stella", PathParent),
        ("a/./../b.stella", PathParent),
        ("a/.//b.stella", PathEmptyPart),
        ("Programs//hello.stella", PathEmptyPart),
        ("Programs/./hello.stella", PathDotPart),
        ("Programs/hello.txt", DirectoryNotKebab),
        ("Hello.txt", NameNotStella),
    ] {
        refuses(path, kind);
    }
}

// Fuzzing against an oracle, seeded so a failing case replays.

/// Returns what the path lint should say about `path`, worked out the way the reference words
/// it: the normalized path and the name, or the kind of the first problem.
fn oracle(path: &str) -> Result<(&str, &str), Kind> {
    if path.starts_with('/') {
        return Err(PathAbsolute);
    }
    let rest = path.strip_prefix("./").unwrap_or(path);
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.contains(&"..") {
        return Err(PathParent);
    }
    if parts.contains(&"") {
        return Err(PathEmptyPart);
    }
    if parts.contains(&".") {
        return Err(PathDotPart);
    }
    let (file, directories) = parts.split_last().expect("split gives at least one part");
    if !directories.iter().all(|directory| is_kebab(directory)) {
        return Err(DirectoryNotKebab);
    }
    let Some(name) = file.strip_suffix(".stella") else {
        return Err(NameNotStella);
    };
    if name.is_empty() {
        return Err(NameEmpty);
    }
    if !is_kebab(name) {
        return Err(NameNotKebab);
    }
    Ok((rest, name))
}

/// Says whether `name` is words of lowercase letters and digits, joined by single hyphens.
fn is_kebab(name: &str) -> bool {
    name.split('-').all(|word| {
        let letters_and_digits = word
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit());
        !word.is_empty() && letters_and_digits
    })
}

/// This is a small deterministic generator, xorshift64*.
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

/// Returns a random path: one to four parts, each one to three pieces long. Most pieces are
/// valid in a name, so every answer the oracle can give comes up, accepting included.
fn random_path(rng: &mut Rng) -> String {
    let valid = ["a", "b", "z", "0", "9", "hello", "rung", "x86"];
    let invalid = ["--", ".", "..", "", "A", "_", " ", "é", ".stella", ".txt"];
    let starts = ["", "", "", "", "", "./", "/", "../"];
    let mut path = String::from(rng.pick(&starts));
    let parts_count = 1 + rng.below(4);
    for part_index in 0..parts_count {
        if part_index > 0 {
            path.push('/');
        }
        for _ in 0..1 + rng.below(3) {
            let piece = match rng.below(4) {
                0 => "-",
                1 => rng.pick(&invalid),
                _ => rng.pick(&valid),
            };
            path.push_str(piece);
        }
    }
    if rng.below(4) > 0 {
        path.push_str(".stella");
    }
    path
}

#[test]
fn fuzz_against_the_oracle() {
    let mut rng = Rng { state: 0x5745_4C4C_4100_0001 };
    let mut accepted_count: u32 = 0;
    let mut refused_counts = [
        (PathAbsolute, 0_u32),
        (PathParent, 0),
        (PathEmptyPart, 0),
        (PathDotPart, 0),
        (DirectoryNotKebab, 0),
        (NameNotStella, 0),
        (NameEmpty, 0),
        (NameNotKebab, 0),
    ];
    for _ in 0..50_000 {
        let path = random_path(&mut rng);
        match oracle(&path) {
            Ok((normalized, name)) => {
                accepts(&path, normalized, name);
                accepted_count += 1;
            }
            Err(kind) => {
                refuses(&path, kind);
                let (_, count) = refused_counts
                    .iter_mut()
                    .find(|(counted, _)| *counted == kind)
                    .expect("the oracle only gives path kinds");
                *count += 1;
            }
        }
    }
    // The fuzzing only means something if the paths reach every answer, often.
    assert!(accepted_count > 1_000, "only {accepted_count} were accepted");
    for (kind, count) in refused_counts {
        assert!(count > 50, "only {count} were refused for {kind:?}");
    }
}
