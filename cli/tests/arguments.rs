// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the arguments. They're a crate of their own, so the compiler lets them reach
//! only what the crate makes public: arguments in, a `Command` or a `Misuse` out, and the sentence
//! each misuse prints.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt as _;

use stella_cli::{Command, Misuse, USAGE, command};

/// Returns `arguments`, which needn't be UTF-8, as the command is given them.
fn given(arguments: &[&[u8]]) -> Vec<OsString> {
    arguments.iter().map(|argument| OsString::from_vec(argument.to_vec())).collect()
}

#[test]
fn commands() {
    // `lint` alone lints every program, and `lint` with one path lints that one. The path is
    // taken as it is, whatever it holds, since checking it is the path lint's to do.
    for (arguments, expected) in [
        (&[&b"lint"[..]][..], Command::LintEvery),
        (&[b"lint", b"hello.stella"], Command::LintOne(b"hello.stella")),
        (&[b"lint", b"./programs/hello.stella"], Command::LintOne(b"./programs/hello.stella")),
        (&[b"lint", b"/"], Command::LintOne(b"/")),
        (&[b"lint", b"lint"], Command::LintOne(b"lint")),
        (&[b"lint", b"a\xFF.stella"], Command::LintOne(b"a\xFF.stella")),
    ] {
        assert_eq!(command(&given(arguments)), Ok(expected), "for {arguments:?}");
    }
}

#[test]
fn misuses() {
    // Each wrong way to call the command, with the misuse it is and the sentence that prints.
    for (arguments, misuse, sentence) in [
        (&[][..], Misuse::NoCommand, "a command is needed."),
        (&[&b""[..]], Misuse::NoCommand, "a command is needed."),
        (&[b"bild"], Misuse::NotACommand(b"bild"), "`bild` isn't a command."),
        (&[b"Lint"], Misuse::NotACommand(b"Lint"), "`Lint` isn't a command."),
        // An argument is shown safely, whether or not it's UTF-8.
        (
            &[b"bi\x1B[31mld"],
            Misuse::NotACommand(b"bi\x1B[31mld"),
            "`bi\\u{1B}[31mld` isn't a command.",
        ),
        (&[b"\xFF"], Misuse::NotACommand(b"\xFF"), "`\\xFF` isn't a command."),
        // There are no options, so anything that looks like one is refused wherever it is, and
        // before anything else is.
        (&[b"--help"], Misuse::NotAnOption(b"--help"), "`--help` isn't an option; there are none."),
        (
            &[b"lint", b"--fix"],
            Misuse::NotAnOption(b"--fix"),
            "`--fix` isn't an option; there are none.",
        ),
        (
            &[b"bild", b"hello.stella", b"--fix"],
            Misuse::NotAnOption(b"--fix"),
            "`--fix` isn't an option; there are none.",
        ),
        // Source can't be piped in, so `-` is an option like any other.
        (&[b"lint", b"-"], Misuse::NotAnOption(b"-"), "`-` isn't an option; there are none."),
        (
            &[b"lint", b"a.stella", b"b.stella"],
            Misuse::TooManyPaths,
            "`lint` takes one path, or none to lint every file.",
        ),
        (
            &[b"lint", b"a.stella", b"b.stella", b"c.stella"],
            Misuse::TooManyPaths,
            "`lint` takes one path, or none to lint every file.",
        ),
        (&[b"lint", b""], Misuse::EmptyPath, "the path is empty."),
    ] {
        assert_eq!(command(&given(arguments)), Err(misuse), "for {arguments:?}");
        assert_eq!(misuse.to_string(), sentence, "for {misuse:?}");
    }
}

#[test]
fn the_usage_line() {
    // The line every misuse ends with names the one command there is, and ends its line.
    assert_eq!(USAGE, "usage: stella lint [path]\n");
}
