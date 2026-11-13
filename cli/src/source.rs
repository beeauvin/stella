// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Finding a program's source: from the path it was typed as, to the bytes of its file.
//!
//! A program's path is checked twice before its file is read. First as it was typed, so a path
//! the compiler can't build from is refused without anything being looked at. Then as it's
//! stored, since a filesystem that ignores case can reach `Hello.stella` by `hello.stella`. A
//! problem with the path as it's stored is shown at that path, and any other at the path as it
//! was typed.

use std::borrow::Cow;
use std::path::Path;
use std::str;

use stella_linter_path::{Problem, SourcePath, lint_path};

use crate::message::{Place, message};
use crate::read::read;
use crate::stop::Stop;
use crate::stored::{SearchFailure, search_for_stored};

/// A program's paths are the two it goes by: as it was typed, which messages show, and as it's
/// stored, which its file is read by.
///
/// Only `find` and `found` make one, so the path it's stored by is always the one the
/// filesystem gave for the path it was typed as, and the two can't be given the wrong way round.
#[derive(Clone, Debug)]
pub(crate) struct ProgramPaths<'path> {
    typed: &'path [u8],
    stored: Cow<'path, [u8]>,
}

impl<'path> ProgramPaths<'path> {
    /// Returns the paths of the program at `typed`, a path as it was typed, under `root`. The
    /// path is checked before anything is looked at, and then found as it's stored.
    ///
    /// # Errors
    ///
    /// Returns the stop: the path is refused as it was typed, or it goes through a symlink, or
    /// it can't be found.
    pub(crate) fn find(root: &Path, typed: &'path [u8]) -> Result<Self, Stop> {
        let typed_path = lint_path_bytes(typed)
            .map_err(|problem| Stop::Refused(message(typed, Place::File, problem)))?;
        // On macOS, which ignores case, `hello.stella` can name `Hello.stella`, so the path is
        // found as it's stored before it's checked again.
        match search_for_stored(root, typed_path) {
            Ok(stored) => Ok(Self { typed, stored: Cow::Owned(stored) }),
            Err(failure @ SearchFailure::Symlink) => {
                Err(Stop::Refused(message(typed, Place::File, failure)))
            }
            Err(
                failure @ (SearchFailure::Access(_)
                | SearchFailure::DirectoryUnreadable(_)
                | SearchFailure::TooManyEntries),
            ) => Err(Stop::Failed(message(typed, Place::File, failure))),
        }
    }

    /// Returns the paths of a program the walk found at `path`. The walk finds each name as
    /// it's stored, and never follows a symlink, so the one path is both.
    pub(crate) const fn found(path: &'path [u8]) -> Self {
        Self { typed: path, stored: Cow::Borrowed(path) }
    }

    /// Returns the program's path as it was typed, or as the walk found it.
    pub(crate) const fn typed(&self) -> &'path [u8] {
        self.typed
    }

    /// Checks the path the program is stored by, and reads the file there, under `root`.
    /// Returns the path, which passed the path lint, and the file's bytes, as many as a source
    /// can be and one more.
    ///
    /// # Errors
    ///
    /// Returns the stop: the path is refused as it's stored, which is shown at that path, or
    /// the file can't be read.
    pub(crate) fn read(&self, root: &Path) -> Result<(SourcePath<'_>, Vec<u8>), Stop> {
        let stored_path = lint_path_bytes(&self.stored)
            .map_err(|problem| Stop::Refused(message(&self.stored, Place::File, problem)))?;
        // Joining a path that's absolute would put the root aside, and one that climbs with
        // `..` would leave it. The path lint refuses both, and the file is read on its word.
        let normalized = stored_path.normalized();
        assert!(!normalized.starts_with('/'), "a path that passed the lint isn't absolute");
        let climbs = normalized.split('/').any(|part| part == "..");
        assert!(!climbs, "a path that passed the lint never climbs with `..`");
        let source = read(&root.join(normalized))
            .map_err(|failure| Stop::Failed(message(self.typed, Place::File, failure)))?;
        Ok((stored_path, source))
    }
}

/// Lints `path`, in the bytes it was typed or stored in.
///
/// A path that passes was UTF-8 as it came, so the path that's returned is those very bytes. One
/// that isn't UTF-8 is linted with U+FFFD in place of each run of bytes that isn't. No name may
/// hold that, so the path is refused by whichever check meets it first, like any other bad name.
fn lint_path_bytes(path: &[u8]) -> Result<SourcePath<'_>, Problem> {
    if let Ok(text) = str::from_utf8(path) {
        return lint_path(text);
    }
    let replaced = String::from_utf8_lossy(path);
    Err(lint_path(&replaced).expect_err("no path that passes holds U+FFFD"))
}
