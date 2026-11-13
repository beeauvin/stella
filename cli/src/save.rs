// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Saving a binary: putting its bytes in `.build/`, at its source's path without `.stella`.
//!
//! The directories on the way are made as needed. Each one that's already there must be a
//! directory itself, not a symlink to one, so a symlink in `.build/` never leads a build out of
//! it. Anything at the binary's path that isn't a regular file is left alone, and stops the
//! build. So a binary and a directory that want one path stop whichever is built second.
//!
//! The bytes are written beside the binary, to a part of this save's own, and the part is
//! renamed over the binary's path. So a binary is never rewritten in place, which macOS can
//! refuse to run, and nobody ever sees half of one.
//!
//! A part is named `.<process>.<count>.part`: the number of the process that makes it, and a
//! count from 0. No binary's name starts with a dot, so a part is never taken for one. A save
//! makes its part only where nothing is, at the first count that's free, and never looks at,
//! writes or removes a part it didn't make. So saves at once never share a part, even saves of
//! one program, in one process or in many, and every one of them saves. A save that fails
//! removes its part. One that's killed part-way leaves it, and nothing reads or removes it:
//! the next save makes its own beside it.

use std::fmt;
use std::path::Path;
use std::process;

use stella_linter_path::SourcePath;

use crate::filesystem::{Creation, NewFile, Occupant, Reason, make_directory, make_file, occupant};
use crate::message::{Place, message};

/// This is the directory binaries are built into, in the directory the command runs in. Its
/// name starts with a `.`, so linting every file never goes into it.
pub const BUILD_DIRECTORY: &str = ".build";

/// A part's name ends with this, after a dot, its process's number, a dot and its count.
const PART_SUFFIX: &str = ".part";

/// This is the most parts with one process's number a directory can hold before a save there
/// stops: the parts of that many saves at once, or that many left by builds that were killed.
pub const PARTS_MAX: u32 = 64;

/// A binary can be read and run by everyone, and written only by its owner, less whatever the
/// umask takes away.
const BINARY_MODE: u32 = 0o755;

/// A save failure is why a binary couldn't be saved, and where under the directory the command
/// runs in.
#[derive(Debug)]
pub struct SaveFailure {
    /// The path where it went wrong, from the directory the command runs in.
    path: String,
    /// What went wrong there.
    kind: SaveFailureKind,
}

impl SaveFailure {
    /// Returns a failure of `kind` at `path`.
    fn at(path: &str, kind: SaveFailureKind) -> Self {
        Self { path: String::from(path), kind }
    }

    /// Returns the path where it went wrong, from the directory the command runs in.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns what went wrong.
    #[must_use]
    pub const fn kind(&self) -> &SaveFailureKind {
        &self.kind
    }

    /// Returns the message that says this failure, at the path where it went wrong.
    #[must_use]
    pub fn message(&self) -> String {
        message(self.path.as_bytes(), Place::File, &self.kind)
    }
}

/// A save failure kind is one of the ways saving a binary can go wrong.
#[derive(Debug)]
pub enum SaveFailureKind {
    /// Something on the way to the binary is there and isn't a directory.
    NotADirectory,
    /// Something at the binary's path is there and isn't a regular file.
    NotAFile,
    /// The directory holds `PARTS_MAX` parts with this process's number, so no part was made.
    TooManyParts,
    /// What's at the path changed between two looks at it, so something else is writing there.
    Changing,
    /// The filesystem refused, for this reason.
    Unwritable(Reason),
}

/// It displays as the sentence that says what's wrong.
impl fmt::Display for SaveFailureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotADirectory => {
                f.write_str("this isn't a directory, so the build can't go through it.")
            }
            Self::NotAFile => f.write_str("this isn't a regular file, so the build left it alone."),
            Self::TooManyParts => {
                f.write_str("this holds too many `.part` files, so the build can't make its own.")
            }
            Self::Changing => {
                f.write_str("something else is changing this, so the build left it alone.")
            }
            Self::Unwritable(reason) => write!(f, "the binary can't be written: {reason}."),
        }
    }
}

/// Saves `bytes` as the binary of the program at `source`, under `root`, the directory the
/// command runs in: in `.build/`, at the source's path without `.stella`.
///
/// # Errors
///
/// Returns where and why the binary couldn't be saved. The binary's path then holds what it
/// held before.
///
/// # Panics
///
/// Never, whatever is on the filesystem. The assertions check what the path lint proved of the
/// source's path.
pub fn save(root: &Path, source: SourcePath<'_>, bytes: &[u8]) -> Result<(), SaveFailure> {
    let directory = match source.directory() {
        "" => String::from(BUILD_DIRECTORY),
        within => format!("{BUILD_DIRECTORY}/{within}"),
    };
    let binary = format!("{directory}/{}", source.name());
    let climbs = binary.split('/').any(|part| part == "..");
    assert!(!climbs, "a path that passed the lint never climbs, so a binary stays in `.build/`");
    assert!(
        !source.name().starts_with('.'),
        "no binary's name starts with a dot, as a part's does"
    );
    save_make_directories(root, &directory)?;
    // KS-CLOSE-USE: what's at the binary's path is looked at before the part is written, not
    // when it's renamed over, since a rename can't be told to refuse a symlink or a FIFO. So
    // something swapped in between the two is replaced. Only someone who can already write in
    // `.build/` could do that, and what they'd lose is what they put there.
    match occupant(&root.join(&binary)) {
        Ok(Occupant::Nothing | Occupant::RegularFile) => {}
        Ok(Occupant::Directory | Occupant::Other) => {
            return Err(SaveFailure::at(&binary, SaveFailureKind::NotAFile));
        }
        Err(reason) => return Err(SaveFailure::at(&binary, SaveFailureKind::Unwritable(reason))),
    }
    let mut part = save_make_part(root, &directory)?;
    part.file
        .fill(bytes)
        .map_err(|reason| SaveFailure::at(&part.path, SaveFailureKind::Unwritable(reason)))?;
    part.file
        .move_over(&root.join(&binary))
        .map_err(|reason| SaveFailure::at(&binary, SaveFailureKind::Unwritable(reason)))
}

/// A part is the file a binary's bytes are written to before it's renamed over the binary.
struct Part {
    /// The part's path, from the directory the command runs in.
    path: String,
    /// The file, which this save made and nothing else holds.
    file: NewFile,
}

/// Makes this save's part in `directory`, a path from `root` that's there, and returns it to
/// write to. It's made where nothing is, at the first count that's free, so it's this save's
/// alone. Whatever is at a count that's taken, even something that isn't a file, is another
/// save's or was left behind, and isn't touched.
fn save_make_part(root: &Path, directory: &str) -> Result<Part, SaveFailure> {
    let process = process::id();
    for count in 0..PARTS_MAX {
        let path = format!("{directory}/.{process}.{count}{PART_SUFFIX}");
        match make_file(root.join(&path), BINARY_MODE) {
            Ok(Creation::Made(file)) => return Ok(Part { path, file }),
            Ok(Creation::AlreadyThere) => {}
            Err(reason) => {
                return Err(SaveFailure::at(&path, SaveFailureKind::Unwritable(reason)));
            }
        }
    }
    Err(SaveFailure::at(directory, SaveFailureKind::TooManyParts))
}

/// Makes each directory of `directory`, a path from `root` that starts with `.build`, that
/// isn't there yet. Each one that's there must be a directory itself.
fn save_make_directories(root: &Path, directory: &str) -> Result<(), SaveFailure> {
    let mut made = String::with_capacity(directory.len());
    for part in directory.split('/') {
        assert!(!part.is_empty(), "a path that passed the lint has no empty part");
        if !made.is_empty() {
            made.push('/');
        }
        made.push_str(part);
        let path = root.join(&made);
        let kind = match make_directory(&path) {
            Ok(Creation::Made(())) => continue,
            // KS-CLOSE-USE: what's there is looked at once it's known to be there, and then
            // gone through by its name. Something swapped in after the look could still be a
            // symlink that's followed. Closing that takes opening each directory from the one
            // before it, which std can't do, and only someone who can already write in
            // `.build/` could open the gap.
            Ok(Creation::AlreadyThere) => match occupant(&path) {
                Ok(Occupant::Directory) => continue,
                Ok(Occupant::RegularFile | Occupant::Other) => SaveFailureKind::NotADirectory,
                Ok(Occupant::Nothing) => SaveFailureKind::Changing,
                Err(reason) => SaveFailureKind::Unwritable(reason),
            },
            Err(reason) => SaveFailureKind::Unwritable(reason),
        };
        return Err(SaveFailure::at(&made, kind));
    }
    Ok(())
}
