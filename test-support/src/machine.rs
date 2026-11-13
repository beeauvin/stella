// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Asking the system's own tools what machine code is: `as` to make it, and `otool` to read it.
//!
//! A test that checks an instruction's word against these checks it against the tools every
//! other program on the machine was built with, not against a number someone copied down.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::process::Run;

/// The code in an object file or a binary: where it starts, and its instructions' words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    /// The address of the first instruction. In an object file, which isn't linked, it's 0.
    pub address: u64,
    /// The instructions, each as the 32-bit word the processor reads, in order.
    pub words: Vec<u32>,
}

/// Assembles `assembly` for arm64 with the system's `as`, as `<name>.o` in `directory`, and
/// returns the object file's path.
///
/// # Panics
///
/// Panics if the assembly can't be written, or if `as` can't be run, refuses it, or writes
/// anything, such as a warning.
#[must_use]
pub fn assemble(directory: &Path, name: &str, assembly: &str) -> PathBuf {
    let (source, object) =
        (directory.join(format!("{name}.s")), directory.join(format!("{name}.o")));
    fs::write(&source, assembly).expect("a test directory is the tests' own to write files in");
    let mut assembler = Command::new("/usr/bin/as");
    let run = Run::of(assembler.args(["-arch", "arm64", "-o"]).arg(&object).arg(&source));
    assert_eq!(run, Run::exits(0, ""), "`as` should assemble {}", source.display());
    object
}

/// Returns the code in the object file or binary at `path`, as the system's `otool` reads it.
///
/// # Panics
///
/// Panics if `otool` can't be run or fails, or if what it prints isn't an address and then
/// words, in hexadecimal, on each line.
#[must_use]
pub fn text(path: &Path) -> Text {
    let run = Run::of(Command::new("/usr/bin/otool").arg("-t").arg(path));
    let read = run.status == 0_i32;
    assert!(read, "`otool -t` should read {}: {}", path.display(), run.stderr);
    let mut address = None;
    let mut words = Vec::new();
    // The first two lines name the file and the section. Each line after is an address, then
    // up to four words.
    for line in run.stdout.lines().skip(2) {
        let mut parts = line.split_whitespace();
        let line_address = parts.next().expect("a line of code starts with its address");
        let line_address = u64::from_str_radix(line_address, 16);
        let line_address = line_address.expect("an address is written in hexadecimal");
        address.get_or_insert(line_address);
        for word in parts {
            assert_eq!(word.len(), 8, "{word:?} should be one word of eight digits");
            words.push(u32::from_str_radix(word, 16).expect("a word is written in hexadecimal"));
        }
    }
    let address = address.expect("the file should hold code");
    assert!(!words.is_empty(), "{} should hold code", path.display());
    Text { address, words }
}
