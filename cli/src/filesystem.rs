// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The filesystem, as the command asks it.
//!
//! This is the one module that looks at, opens, lists, makes, renames or removes anything on the
//! filesystem. The walk, the search for a path as it's stored, the reading of a file and the
//! saving of a binary each ask through it, so each of the filesystem's failures becomes one of
//! the command's own errors in one place, and is worded there. Reading the bytes of a file that's
//! open is `read`'s work, and a failure there comes back here to be made an `AccessFailure` like
//! any other.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, FileType, Metadata, OpenOptions};
use std::io::{self, Write as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

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

    /// Returns the reason a stream that was open gave, as `error`, when it was written to.
    pub(crate) const fn from_writing(error: io::Error) -> Self {
        Self(error)
    }
}

/// It displays as the error's own words, made safe to print, for a sentence to end with. The
/// operating system's errors always have words. A stream's own error needn't, since whoever
/// wrote the stream made it, so one with none says that.
impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = shown(self.0.to_string().as_bytes());
        if reason.is_empty() {
            return f.write_str("no reason was given");
        }
        f.write_str(&reason)
    }
}

/// An access failure is why the filesystem couldn't give what's at a path.
#[derive(Debug)]
pub enum AccessFailure {
    /// Nothing is at the path.
    Missing,
    /// What's at the path couldn't be looked at, opened or read.
    Unreadable(Reason),
}

impl AccessFailure {
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
impl fmt::Display for AccessFailure {
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
pub(crate) fn look_at(path: &Path) -> Result<Metadata, AccessFailure> {
    fs::symlink_metadata(path).map_err(AccessFailure::from_reaching)
}

/// Looks at what `path` leads to, following a symlink there.
pub(crate) fn look_through(path: &Path) -> Result<Metadata, AccessFailure> {
    fs::metadata(path).map_err(AccessFailure::from_reaching)
}

/// Opens the file at `path` to read it.
pub(crate) fn open(path: &Path) -> Result<File, AccessFailure> {
    File::open(path).map_err(AccessFailure::from_reaching)
}

/// An occupant is what's at a path, as far as putting something there cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Occupant {
    /// Nothing is there.
    Nothing,
    /// A directory is there.
    Directory,
    /// A regular file is there.
    RegularFile,
    /// Something else is there: a symlink, whatever it leads to, a FIFO, a socket or a device.
    Other,
}

/// Returns what's at `path` itself. If it's a symlink, the symlink is what's there.
pub(crate) fn occupant(path: &Path) -> Result<Occupant, Reason> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(Occupant::Directory),
        Ok(metadata) if metadata.is_file() => Ok(Occupant::RegularFile),
        Ok(_) => Ok(Occupant::Other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Occupant::Nothing),
        Err(error) => Err(Reason(error)),
    }
}

/// A creation is how making something at a path came out, when the filesystem let it be tried.
#[derive(Debug)]
pub(crate) enum Creation<Made> {
    /// It was made, where nothing was before.
    Made(Made),
    /// Something is already there, so nothing was made, and what's there wasn't touched.
    AlreadyThere,
}

/// Makes a directory at `path`, in a directory that's there.
pub(crate) fn make_directory(path: &Path) -> Result<Creation<()>, Reason> {
    match fs::create_dir(path) {
        Ok(()) => Ok(Creation::Made(())),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(Creation::AlreadyThere),
        Err(error) => Err(Reason(error)),
    }
}

/// A new file is one this command made, where nothing was before, and can still write to.
///
/// It owns the file at the path it was made at. Nothing else moves the file from there, and a
/// new file that's dropped before it's moved removes its file, so what was half made is never
/// left behind.
#[derive(Debug)]
pub(crate) struct NewFile {
    path: PathBuf,
    file: File,
    /// Whether the file is still at `path`, where dropping this removes it.
    is_at_path: bool,
}

/// Makes a file at `path` to write to, with `mode` as far as the umask lets it be. It's made
/// only if nothing is there, so a symlink at the path is never followed, and what's already
/// there is never touched.
pub(crate) fn make_file(path: PathBuf, mode: u32) -> Result<Creation<NewFile>, Reason> {
    match OpenOptions::new().write(true).create_new(true).mode(mode).open(&path) {
        Ok(file) => Ok(Creation::Made(NewFile { path, file, is_at_path: true })),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(Creation::AlreadyThere),
        Err(error) => Err(Reason(error)),
    }
}

impl NewFile {
    /// Writes all of `bytes` to the file, and waits until its bytes are on the disk. The file
    /// keeps the mode it was made with.
    pub(crate) fn fill(&mut self, bytes: &[u8]) -> Result<(), Reason> {
        self.file.write_all(bytes).map_err(Reason)?;
        self.file.sync_all().map_err(Reason)
    }

    /// Moves the file to `path`, in place of any file that's there, and closes it. Whoever
    /// looks at that path sees the old file or the new one, never part of either. If it can't
    /// be moved, it's removed, and what's at `path` stays as it was.
    pub(crate) fn move_over(mut self, path: &Path) -> Result<(), Reason> {
        // KS-CLOSE-USE: the file is moved by the name it was made under. Nothing of this
        // command's takes that name while the file is there, but someone else who can write in
        // its directory could put another file under it first, and that one would be moved.
        fs::rename(&self.path, path).map_err(Reason)?;
        self.is_at_path = false;
        Ok(())
    }
}

impl Drop for NewFile {
    fn drop(&mut self) {
        if self.is_at_path {
            // KS-CLOSE-USE: the file is removed by the name it was made under, as it's moved.
            // If it can't be removed, it stays, and nothing reads it: whatever stopped the
            // file from being moved is what gets reported.
            let _: io::Result<()> = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{ErrorKind, Read as _};
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
    use std::path::PathBuf;

    use stella_test_support::{ModeGuard, Scratch, fifo, write};

    use super::{
        AccessFailure, Creation, Occupant, list, look_at, look_through, make_directory, make_file,
        occupant, open,
    };

    /// Returns a new, empty directory for the test named `name` to work in.
    fn fresh(name: &str) -> PathBuf {
        Scratch::of("filesystem").fresh(name)
    }

    /// Returns the kind of error that left `failure` unreadable.
    fn kind_of(failure: &AccessFailure) -> ErrorKind {
        let AccessFailure::Unreadable(reason) = failure else {
            unreachable!("{failure:?} should be unreadable, not missing");
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
        for failure in [looked_at, looked_through, opened] {
            assert!(matches!(failure, AccessFailure::Missing), "{failure:?}");
            assert_eq!(failure.to_string(), "this file doesn't exist.");
        }
    }

    #[test]
    fn what_cant_be_reached_keeps_the_systems_reason() {
        // A path through a file, and a file with no permissions. Neither is missing: each is
        // unreadable, with the operating system's kind of error. For the path through a file,
        // the system's words are in the sentence too.
        let directory = fresh("unreadable");
        write(&directory, "file.stella", b"x");
        let through = directory.join("file.stella/under.stella");
        for failure in [
            look_at(&through).expect_err("a file holds no files"),
            look_through(&through).expect_err("a file holds no files"),
            open(&through).expect_err("a file holds no files"),
        ] {
            assert_eq!(kind_of(&failure), ErrorKind::NotADirectory);
            let sentence = failure.to_string();
            assert!(sentence.starts_with("this file can't be read: Not a directory"), "{sentence}");
            assert!(sentence.ends_with(")."), "{sentence} should end the system's words");
        }
        let locked = directory.join("file.stella");
        let guard = ModeGuard::new(locked.clone(), 0o000);
        // Looking at a file needs nothing of the file itself, and opening it does.
        assert!(look_at(&locked).is_ok(), "a file with no permissions can still be looked at");
        let failure = open(&locked).expect_err("nobody may read it");
        assert_eq!(kind_of(&failure), ErrorKind::PermissionDenied);
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
        let failure = AccessFailure::from_reading(ErrorKind::NotFound.into());
        assert_eq!(kind_of(&failure), ErrorKind::NotFound);
        assert!(failure.to_string().starts_with("this file can't be read: "), "{failure}");
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
        assert!(matches!(through, Err(AccessFailure::Missing)), "{through:?}");
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

    #[test]
    fn an_occupant_is_what_the_path_itself_holds() {
        // Nothing, a directory, a file, a FIFO, and a symlink to each of a file, a directory
        // and nothing. A symlink is never looked through, so each is something else.
        let directory = fresh("occupant");
        write(&directory, "file", b"x");
        write(&directory, "inner/deep", b"x");
        fifo(&directory.join("fifo"));
        symlink("file", directory.join("to-file")).expect("a symlink can be made");
        symlink("inner", directory.join("to-directory")).expect("a symlink can be made");
        symlink("absent", directory.join("to-nothing")).expect("a symlink can be made");
        for (name, expected) in [
            ("absent", Occupant::Nothing),
            ("inner", Occupant::Directory),
            ("file", Occupant::RegularFile),
            ("fifo", Occupant::Other),
            ("to-file", Occupant::Other),
            ("to-directory", Occupant::Other),
            ("to-nothing", Occupant::Other),
        ] {
            let found = occupant(&directory.join(name)).expect("the path can be looked at");
            assert_eq!(found, expected, "for {name}");
        }
        // A path that can't be looked at isn't nothing: it's the system's reason.
        let reason = occupant(&directory.join("file/under")).expect_err("a file holds no files");
        assert_eq!(reason.kind(), ErrorKind::NotADirectory);
        let _guard = ModeGuard::new(directory.join("inner"), 0o000);
        let reason = occupant(&directory.join("inner/deep")).expect_err("nobody may look");
        assert_eq!(reason.kind(), ErrorKind::PermissionDenied);
    }

    #[test]
    fn a_directory_is_made_only_where_nothing_is() {
        // Where nothing is, a directory is made. Where anything is, even a symlink to nothing,
        // it's already there and isn't touched. Where the directory it would be in is missing
        // or can't be written, the system says why.
        let directory = fresh("make-directory");
        let made = make_directory(&directory.join("new")).expect("nothing is in the way");
        assert!(matches!(made, Creation::Made(())), "{made:?}");
        assert!(directory.join("new").is_dir(), "the directory was made");
        write(&directory, "file", b"kept");
        symlink("absent", directory.join("to-nothing")).expect("a symlink can be made");
        for name in ["new", "file", "to-nothing"] {
            let made = make_directory(&directory.join(name)).expect("something is there");
            assert!(matches!(made, Creation::AlreadyThere), "for {name}: {made:?}");
        }
        assert_eq!(fs::read(directory.join("file")).expect("the file can be read"), b"kept");
        assert!(!directory.join("absent").exists(), "a symlink to nothing isn't made through");
        let reason = make_directory(&directory.join("absent/new")).expect_err("no parent");
        assert_eq!(reason.kind(), ErrorKind::NotFound);
        let _guard = ModeGuard::new(directory.join("new"), 0o555);
        let reason = make_directory(&directory.join("new/inner")).expect_err("nobody may write");
        assert_eq!(reason.kind(), ErrorKind::PermissionDenied);
    }

    #[test]
    fn a_file_is_made_only_where_nothing_is() {
        // Where anything is, even a symlink, no file is made, and neither what's there nor
        // what a symlink leads to is written. Where nothing is, the file is made with its mode.
        let directory = fresh("make-file");
        write(&directory, "file", b"kept");
        write(&directory, "inner/deep", b"x");
        symlink("file", directory.join("to-file")).expect("a symlink can be made");
        symlink("absent", directory.join("to-nothing")).expect("a symlink can be made");
        for name in ["file", "inner", "to-file", "to-nothing"] {
            let path = directory.join(name);
            let made = make_file(path, 0o755).expect("something is there");
            assert!(matches!(made, Creation::AlreadyThere), "for {name}: {made:?}");
        }
        assert_eq!(fs::read(directory.join("file")).expect("the file can be read"), b"kept");
        assert!(!directory.join("absent").exists(), "a symlink to nothing isn't made through");
        let path = directory.join("new");
        let made = make_file(path.clone(), 0o600).expect("nothing is in the way");
        assert!(matches!(made, Creation::Made(_)), "{made:?}");
        let metadata = fs::symlink_metadata(&path).expect("the file was made");
        assert!(metadata.is_file(), "what's made is a regular file");
        assert_eq!(metadata.len(), 0, "a new file is empty");
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o600, "no umask adds to a mode");
        let reason = make_file(directory.join("absent/new"), 0o755).expect_err("no parent");
        assert_eq!(reason.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn a_new_file_is_filled_and_moved_over_another() {
        // A new file is filled and moved over a file that's there. That path then holds the new
        // file itself, with its bytes and the mode it was made with, and nothing is left where
        // the new file was made.
        let directory = fresh("new-file");
        write(&directory, "old", b"the old bytes");
        let part = directory.join("part");
        let Creation::Made(mut file) =
            make_file(part.clone(), 0o600).expect("nothing is in the way")
        else {
            unreachable!("nothing is at {} yet", part.display());
        };
        file.fill(b"the new bytes").expect("the tests' own file can be written");
        let inode_part = fs::metadata(&part).expect("the file is there").ino();
        assert_eq!(fs::read(&part).expect("the file can be read"), b"the new bytes");
        file.move_over(&directory.join("old")).expect("the tests' own file can be moved");
        let metadata = fs::symlink_metadata(directory.join("old")).expect("the file is there");
        assert_eq!(metadata.ino(), inode_part, "the new file itself is what's there now");
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o600, "the mode is as it was made");
        let bytes = fs::read(directory.join("old")).expect("the file can be read");
        assert_eq!(bytes, b"the new bytes");
        assert!(!part.exists(), "nothing is left where the file was made");
    }

    #[test]
    fn a_new_file_that_isnt_moved_is_removed() {
        // A directory is where the file would go, which a file can't be moved over. The system
        // says why, the file is removed from where it was made, and the directory is as it was.
        let directory = fresh("unmoved");
        write(&directory, "inner/deep", b"x");
        let part = directory.join("part");
        let Creation::Made(mut file) =
            make_file(part.clone(), 0o755).expect("nothing is in the way")
        else {
            unreachable!("nothing is at {} yet", part.display());
        };
        file.fill(b"bytes").expect("the tests' own file can be written");
        let reason = file.move_over(&directory.join("inner")).expect_err("a directory is there");
        assert_eq!(reason.kind(), ErrorKind::IsADirectory);
        assert!(!part.exists(), "a file that couldn't be moved is removed");
        assert_eq!(fs::read(directory.join("inner/deep")).expect("it's still there"), b"x");
        // A new file that's dropped before it's moved at all is removed too, even unfilled.
        let made = make_file(part.clone(), 0o755).expect("nothing is in the way");
        assert!(matches!(made, Creation::Made(_)), "{made:?}");
        assert!(part.exists(), "the file is there while it's held");
        drop(made);
        assert!(!part.exists(), "a file that was never moved is removed");
    }
}
