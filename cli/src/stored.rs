// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A path as it's stored is a path that names each directory and file the way its filesystem
//! does.
//!
//! The path lint checks a path as it was typed. But a filesystem can match a name it stores
//! differently: macOS ignores case, so `hello.stella` reaches a file stored as `Hello.stella`. So
//! each part of a typed path is found as it's stored, and the path lint then checks that too.
//!
//! A part is found by its name among its directory's entries, never by resolving the path, which
//! would follow symlinks and can be too long to resolve. The entry with exactly the name typed is
//! it. Failing that, the entry that differs only in ASCII case is, which is how a filesystem that
//! ignores case stores most such names. Failing that, the name folds to the one typed some way
//! past ASCII, as the Kelvin sign folds to `k`: then it's the entry that isn't ASCII and is the
//! very file the typed name reaches.
//!
//! An entry is never matched by the inode its directory lists for it. A mount point is listed
//! with the inode of the directory it covers, not of what's mounted there, so a listing can't say
//! whether an entry is a given file. Only looking at the entry itself can.
//!
//! A symlink anywhere in the path is refused, before any name in it is checked, just as the walk
//! never follows one.

use std::ffi::OsStr;
use std::fmt;
use std::fs::Metadata;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use stella_linter_path::SourcePath;

use crate::filesystem::{AccessFailure, Reason, list, look_at};
use crate::walk::WALK_ENTRIES_MAX;

/// This is the most entries the search looks at, across every directory on the way to one
/// path. It's the walk's bound, so the compiler has one number for how far it looks.
pub const SEARCH_ENTRIES_MAX: u32 = WALK_ENTRIES_MAX;

const _: () = assert!(
    SEARCH_ENTRIES_MAX == 10_000,
    "`TooManyEntries`'s sentence names the bound, so a new bound needs a new sentence"
);

/// A search failure is why a path couldn't be found as it's stored.
#[derive(Debug)]
pub enum SearchFailure {
    /// A part of the path is a symlink.
    Symlink,
    /// A part of the path couldn't be reached: it names nothing, or did when it was looked at
    /// and has gone since, or it can't be looked at.
    Access(AccessFailure),
    /// A directory on the way couldn't be listed.
    DirectoryUnreadable(Reason),
    /// The directories on the way hold more than `SEARCH_ENTRIES_MAX` entries between them.
    TooManyEntries,
}

/// It displays as the sentence that says what's wrong.
impl fmt::Display for SearchFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symlink => f.write_str(
                "the build path can't be established from a path that goes through a symlink.",
            ),
            Self::Access(failure) => failure.fmt(f),
            Self::DirectoryUnreadable(reason) => {
                write!(f, "a directory on the way to this file can't be read: {reason}.")
            }
            Self::TooManyEntries => f.write_str(
                "the directories on the way to this file hold more than 10,000 entries.",
            ),
        }
    }
}

/// Returns `typed`, a path as it was typed, with each name in it as it's stored under `root`,
/// the directory the command runs in.
///
/// # Errors
///
/// Returns why the path couldn't be found: a part of it is a symlink, names nothing or can't be
/// looked at, or a directory on the way can't be listed or holds too many entries.
///
/// # Panics
///
/// Never, whatever the path or the filesystem holds. The assertions check this function's own
/// work.
pub fn search_for_stored(root: &Path, typed: SourcePath<'_>) -> Result<Vec<u8>, SearchFailure> {
    let mut stored: Vec<u8> = Vec::new();
    // This counts the entries looked at across every directory on the way, to keep it bounded.
    let mut entries_count: u32 = 0;
    for typed in typed.normalized().split('/') {
        assert!(!typed.is_empty(), "the path lint refuses an empty part");
        let directory = root.join(OsStr::from_bytes(&stored));
        let part = Part { directory: &directory, typed };
        let metadata = look_at(&directory.join(typed)).map_err(SearchFailure::Access)?;
        if metadata.file_type().is_symlink() {
            return Err(SearchFailure::Symlink);
        }
        let name = stored_name(part, FileId::of(&metadata), &mut entries_count)?;
        assert!(!name.is_empty(), "an entry has a name");
        if !stored.is_empty() {
            stored.push(b'/');
        }
        stored.extend_from_slice(&name);
    }
    assert!(!stored.is_empty(), "a path has at least one part");
    assert!(entries_count <= SEARCH_ENTRIES_MAX, "the search stopped at the bound");
    Ok(stored)
}

/// A part is one name in a path as it was typed, and the directory it's in.
#[derive(Clone, Copy)]
struct Part<'path> {
    /// The directory the part is in, by a path that can be read.
    directory: &'path Path,
    /// The part's name, as it was typed.
    typed: &'path str,
}

/// A file id is what tells one file from every other: the device it's on, and its inode there.
#[derive(Clone, Copy, PartialEq, Eq)]
struct FileId {
    device: u64,
    inode: u64,
}

impl FileId {
    /// Returns the id of the file `metadata` is about.
    fn of(metadata: &Metadata) -> Self {
        Self { device: metadata.dev(), inode: metadata.ino() }
    }
}

/// Closeness says how closely an entry's name matches the part typed, from least to most.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Closeness {
    /// The name isn't ASCII, and it's a name for the very file the typed part reaches. It may be
    /// the name that folds to the part typed beyond ASCII case, as the Kelvin sign folds to `k`,
    /// or another hard link. Either way it isn't kebab, so the path is refused, though of two
    /// such names it may show the other.
    Folded,
    /// The name differs from the part typed only in ASCII case.
    AsciiCase,
    /// The name is exactly the part typed.
    Exact,
}

/// Returns the name `part` is stored by in its directory. Of several names that match, the
/// closest to the part typed is taken. `reached` is the file the typed part reaches. Every entry
/// is looked at, and counted in `entries_count`, so the bound doesn't depend on the order a
/// directory lists its entries in.
///
/// The part was there when it was looked at, so if no entry is it, it's gone since, and it's
/// missing.
fn stored_name(
    part: Part<'_>,
    reached: FileId,
    entries_count: &mut u32,
) -> Result<Vec<u8>, SearchFailure> {
    assert!(*entries_count <= SEARCH_ENTRIES_MAX, "the search stops at the bound");
    let mut found: Option<(Vec<u8>, Closeness)> = None;
    for entry in list(part.directory).map_err(SearchFailure::DirectoryUnreadable)? {
        let entry = entry.map_err(SearchFailure::DirectoryUnreadable)?;
        *entries_count += 1;
        if *entries_count > SEARCH_ENTRIES_MAX {
            return Err(SearchFailure::TooManyEntries);
        }
        let name = entry.name();
        let Some(closeness) = closeness(part, name.as_bytes(), reached) else {
            continue;
        };
        if found.as_ref().is_none_or(|&(_, best)| closeness > best) {
            found = Some((name.as_bytes().to_vec(), closeness));
        }
    }
    assert!(*entries_count <= SEARCH_ENTRIES_MAX, "the search stopped at the bound");
    let (name, _) = found.ok_or(SearchFailure::Access(AccessFailure::Missing))?;
    Ok(name)
}

/// Returns how closely `name`, an entry of the part's directory, matches the part typed, or
/// nothing when it isn't a name for it. `reached` is the file the typed part reaches.
fn closeness(part: Part<'_>, name: &[u8], reached: FileId) -> Option<Closeness> {
    let typed = part.typed.as_bytes();
    if name == typed {
        return Some(Closeness::Exact);
    }
    if name.eq_ignore_ascii_case(typed) {
        return Some(Closeness::AsciiCase);
    }
    // An ASCII name that matches in neither way is some other file, or another link to this one.
    // It's never the name that was typed, so it's never taken.
    if name.is_ascii() {
        return None;
    }
    // A name that isn't ASCII may fold to the one typed, and only the entry itself can say
    // whether it's the same file. One that can't be looked at isn't.
    let metadata = look_at(&part.directory.join(OsStr::from_bytes(name))).ok()?;
    (FileId::of(&metadata) == reached).then_some(Closeness::Folded)
}
