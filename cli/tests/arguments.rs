// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the arguments. They're a crate of their own, so the compiler lets them reach
//! only what the crate makes public: arguments in, a `Command` or a `Misuse` out, and the sentence
//! each misuse prints.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt as _;

use stella_cli::{Command, Misuse, USAGE, command};

/// Returns `arguments`, which needn't be UTF-8, as the command is given them.
fn given(arguments: &[&[u8]]) -> Vec<OsString> {
    arguments.iter().map(|argument| OsString::from_vec(argument.to_vec())).collect()
}

#[test]
fn commands() {
    // `lint` alone lints every program, `lint` with one path lints that one, and `build` with
    // one path builds that one. The path is taken as it is, whatever it holds, since checking
    // it is the path lint's to do.
    for (arguments, expected) in [
        (&[&b"build"[..], b"hello.stella"][..], Command::Build(b"hello.stella")),
        (&[b"build", b"./programs/hello.stella"], Command::Build(b"./programs/hello.stella")),
        (&[b"build", b"build"], Command::Build(b"build")),
        (&[b"build", b"a\xFF.stella"], Command::Build(b"a\xFF.stella")),
        (&[b"lint"], Command::LintEvery),
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
        (&[b"build", b"-"], Misuse::NotAnOption(b"-"), "`-` isn't an option; there are none."),
        (
            &[b"lint", b"a.stella", b"b.stella"],
            Misuse::LintTooManyPaths,
            "`lint` takes one path, or none to lint every file.",
        ),
        (
            &[b"lint", b"a.stella", b"b.stella", b"c.stella"],
            Misuse::LintTooManyPaths,
            "`lint` takes one path, or none to lint every file.",
        ),
        // `build` builds one program, so it takes exactly one path.
        (&[b"build"], Misuse::BuildNeedsOnePath, "`build` takes one path."),
        (
            &[b"build", b"a.stella", b"b.stella"],
            Misuse::BuildNeedsOnePath,
            "`build` takes one path.",
        ),
        (&[b"lint", b""], Misuse::EmptyPath, "the path is empty."),
        (&[b"build", b""], Misuse::EmptyPath, "the path is empty."),
        // A command is spelled exactly, and takes its paths after it.
        (&[b"Build", b"a.stella"], Misuse::NotACommand(b"Build"), "`Build` isn't a command."),
        (&[b"a.stella", b"build"], Misuse::NotACommand(b"a.stella"), "`a.stella` isn't a command."),
    ] {
        assert_eq!(command(&given(arguments)), Err(misuse), "for {arguments:?}");
        assert_eq!(misuse.to_string(), sentence, "for {misuse:?}");
    }
}

#[test]
fn the_usage_lines() {
    // The lines every misuse ends with name each command there is, one to a line, with what it
    // takes, and the last ends its line.
    assert_eq!(USAGE, "usage: stella build <path>\n       stella lint [path]\n");
}
