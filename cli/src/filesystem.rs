// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The filesystem, as the command asks it.
//!
//! This is the one module that looks at, opens or lists anything on the filesystem. The walk, the
//! search for a path as it's stored and the reading of a file each ask through it, so each of
//! the filesystem's failures becomes one of the command's own errors in one place, and is worded
//! there. Reading the bytes of a file that's open is `read`'s work, and a failure there comes
//! back here to be made an `Unreached` like any other.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, FileType, Metadata};
use std::io;
use std::path::Path;

use crate::message::shown;

/// A reason is why the operating system refused something asked of the filesystem.
#[derive(Debug)]
pub struct Reason(io::Error);

impl Reason {
    /// Returns the kind of error the operating system gave.
    #[must_use]
    pub fn kind(&self) -> io::ErrorKind {
        self.0.kind()
    }
}

/// It displays as the operating system's own words, made safe to print, for a sentence to end
/// with.
impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = shown(self.0.to_string().as_bytes());
        assert!(!reason.is_empty(), "every error says something");
        f.write_str(&reason)
    }
}

/// Unreached says why the filesystem couldn't give what's at a path.
#[derive(Debug)]
pub enum Unreached {
    /// Nothing is at the path.
    Missing,
    /// What's at the path couldn't be looked at, opened or read.
    Unreadable(Reason),
}

impl Unreached {
    /// Returns why, from the `error` a file that was open gave when it was read. The file was
    /// there to open, so whatever went wrong, it isn't missing.
    pub(crate) const fn from_reading(error: io::Error) -> Self {
        Self::Unreadable(Reason(error))
    }

    /// Returns why, from the `error` the filesystem gave when the path was looked at or opened.
    fn from_reaching(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::NotFound => Self::Missing,
            _ => Self::Unreadable(Reason(error)),
        }
    }
}

/// It displays as the sentence that says what's wrong.
impl fmt::Display for Unreached {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("this file doesn't exist."),
            Self::Unreadable(reason) => write!(f, "this file can't be read: {reason}."),
        }
    }
}

/// A directory entry is one thing a directory lists.
#[derive(Debug)]
pub(crate) struct DirectoryEntry(fs::DirEntry);

impl DirectoryEntry {
    /// Returns the entry's name in its directory.
    pub(crate) fn name(&self) -> OsString {
        self.0.file_name()
    }

    /// Returns what the entry itself is: a file, a directory, a symlink or something else. A
    /// symlink isn't followed.
    pub(crate) fn file_type(&self) -> Result<FileType, Reason> {
        self.0.file_type().map_err(Reason)
    }
}

/// A listing is a directory's entries, in whatever order the filesystem gives them.
#[derive(Debug)]
pub(crate) struct Listing(fs::ReadDir);

impl Iterator for Listing {
    type Item = Result<DirectoryEntry, Reason>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.0.next()?;
        Some(entry.map(DirectoryEntry).map_err(Reason))
    }
}

/// Lists the directory at `path`. A directory that isn't there is refused like any other that
/// can't be listed, since whoever asks was told it was there.
pub(crate) fn list(path: &Path) -> Result<Listing, Reason> {
    fs::read_dir(path).map(Listing).map_err(Reason)
}

/// Looks at what's at `path` itself. If it's a symlink, the symlink is what's looked at.
pub(crate) fn look_at(path: &Path) -> Result<Metadata, Unreached> {
    fs::symlink_metadata(path).map_err(Unreached::from_reaching)
}

/// Looks at what `path` leads to, following a symlink there.
pub(crate) fn look_through(path: &Path) -> Result<Metadata, Unreached> {
    fs::metadata(path).map_err(Unreached::from_reaching)
}

/// Opens the file at `path` to read it.
pub(crate) fn open(path: &Path) -> Result<File, Unreached> {
    File::open(path).map_err(Unreached::from_reaching)
}

#[cfg(test)]
mod tests {
    use std::io::{ErrorKind, Read as _};
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    use stella_test_support::{ModeGuard, Scratch, write};

    use super::{Unreached, list, look_at, look_through, open};

    /// Returns a new, empty directory for the test named `name` to work in.
    fn fresh(name: &str) -> PathBuf {
        Scratch::of("filesystem").fresh(name)
    }

    /// Returns the kind of error that left `unreached` unreadable.
    fn kind_of(unreached: &Unreached) -> ErrorKind {
        let Unreached::Unreadable(reason) = unreached else {
            unreachable!("{unreached:?} should be unreadable, not missing");
        };
        reason.kind()
    }

    #[test]
    fn nothing_at_a_path_is_missing_however_its_asked_for() {
        // The same absent path, looked at, looked through and opened. Each says it's missing,
        // in the one sentence there is for that.
        let directory = fresh("missing");
        let absent = directory.join("absent.stella");
        let looked_at = look_at(&absent).expect_err("nothing is there");
        let looked_through = look_through(&absent).expect_err("nothing is there");
        let opened = open(&absent).expect_err("nothing is there");
        for unreached in [looked_at, looked_through, opened] {
            assert!(matches!(unreached, Unreached::Missing), "{unreached:?}");
            assert_eq!(unreached.to_string(), "this file doesn't exist.");
        }
    }

    #[test]
    fn what_cant_be_reached_keeps_the_systems_reason() {
        // A path through a file, and a file with no permissions. Neither is missing: each is
        // unreadable, with the operating system's kind of error and its words in the sentence.
        let directory = fresh("unreadable");
        write(&directory, "file.stella", b"x");
        let through = directory.join("file.stella/under.stella");
        for unreached in [
            look_at(&through).expect_err("a file holds no files"),
            look_through(&through).expect_err("a file holds no files"),
            open(&through).expect_err("a file holds no files"),
        ] {
            assert_eq!(kind_of(&unreached), ErrorKind::NotADirectory);
            let sentence = unreached.to_string();
            assert!(sentence.starts_with("this file can't be read: Not a directory"), "{sentence}");
            assert!(sentence.ends_with(")."), "{sentence} should end the system's words");
        }
        let locked = directory.join("file.stella");
        let guard = ModeGuard::new(locked.clone(), 0o000);
        // Looking at a file needs nothing of the file itself, and opening it does.
        assert!(look_at(&locked).is_ok(), "a file with no permissions can still be looked at");
        let unreached = open(&locked).expect_err("nobody may read it");
        assert_eq!(kind_of(&unreached), ErrorKind::PermissionDenied);
        drop(guard);
        // With its permissions back, the file opens, and what's read is what it holds.
        let mut file = open(&locked).expect("its permissions are back");
        let mut bytes = [0_u8; 2];
        let read_bytes = file.read(&mut bytes).expect("an open file can be read");
        assert_eq!(bytes.get(..read_bytes), Some(&b"x"[..]));
    }

    #[test]
    fn a_failed_read_of_an_open_file_is_never_missing() {
        // An error of the kind that means missing when a path is opened, given by a file that
        // was already open. The file was there, so it's unreadable, and keeps the kind.
        let unreached = Unreached::from_reading(ErrorKind::NotFound.into());
        assert_eq!(kind_of(&unreached), ErrorKind::NotFound);
        assert!(unreached.to_string().starts_with("this file can't be read: "), "{unreached}");
    }

    #[test]
    fn a_symlink_is_looked_at_or_looked_through() {
        // One symlink to a file and one to nothing. Looking at either sees the symlink. Looking
        // through sees the file, or that nothing is there.
        let directory = fresh("symlink");
        write(&directory, "real.stella", b"");
        symlink("real.stella", directory.join("link.stella")).expect("a symlink can be made");
        symlink("absent.stella", directory.join("dangling.stella")).expect("a symlink can be made");
        let at = look_at(&directory.join("link.stella")).expect("the symlink is there");
        assert!(at.file_type().is_symlink(), "looking at a symlink sees the symlink");
        let through = look_through(&directory.join("link.stella")).expect("it leads to a file");
        assert!(through.is_file(), "looking through a symlink sees what it leads to");
        let at = look_at(&directory.join("dangling.stella")).expect("the symlink is there");
        assert!(at.file_type().is_symlink(), "a symlink to nothing is still a symlink");
        let through = look_through(&directory.join("dangling.stella"));
        assert!(matches!(through, Err(Unreached::Missing)), "{through:?}");
    }

    #[test]
    fn a_directory_lists_each_entry_as_what_it_is() {
        // A file, a directory and a symlink to the directory. Each is listed once by its name,
        // and the symlink is a symlink, not the directory it leads to.
        let directory = fresh("list");
        write(&directory, "file.stella", b"");
        write(&directory, "inner/deep.stella", b"");
        symlink("inner", directory.join("link")).expect("a symlink can be made");
        let mut listed = Vec::new();
        for entry in list(&directory).expect("the tests' own directory can be listed") {
            let entry = entry.expect("each entry can be read");
            let file_type = entry.file_type().expect("each entry says what it is");
            let kind = match (file_type.is_dir(), file_type.is_symlink()) {
                (true, false) => "directory",
                (false, true) => "symlink",
                (false, false) => "file",
                (true, true) => unreachable!("nothing is both a directory and a symlink"),
            };
            listed.push((entry.name().into_string().expect("the names are ASCII"), kind));
        }
        listed.sort();
        let expected = [("file.stella", "file"), ("inner", "directory"), ("link", "symlink")];
        assert_eq!(listed, expected.map(|(name, kind)| (String::from(name), kind)));
    }

    #[test]
    fn a_directory_that_cant_be_listed_gives_the_systems_reason() {
        // A directory that isn't there, a file, and a directory with no permissions. None can
        // be listed, and each reason has the system's kind and displays as its words alone.
        let directory = fresh("unlisted");
        write(&directory, "file.stella", b"");
        write(&directory, "locked/in.stella", b"");
        let _guard = ModeGuard::new(directory.join("locked"), 0o000);
        for (name, kind, words) in [
            ("absent", ErrorKind::NotFound, "No such file or directory (os error 2)"),
            ("file.stella", ErrorKind::NotADirectory, "Not a directory (os error 20)"),
            ("locked", ErrorKind::PermissionDenied, "Permission denied (os error 13)"),
        ] {
            let reason = list(&directory.join(name)).expect_err("it can't be listed");
            assert_eq!(reason.kind(), kind, "for {name}");
            assert_eq!(reason.to_string(), words, "for {name}");
        }
    }
}
