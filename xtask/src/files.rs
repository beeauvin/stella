// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The filesystem, as the tools use it.
//!
//! This is the one module that calls the filesystem. Each of its failures is turned here into a
//! sentence that names the path and gives the operating system's reason, so nothing else holds
//! an error of the filesystem's.

use std::fs::{self, File};
use std::io::{self, Read as _};
use std::os::unix::fs::symlink;
use std::path::Path;

/// This is the most bytes of a file a tool reads. It's many times the longest manifest in the
/// repository, and it bounds what one read can hold.
pub(crate) const TEXT_BYTES_MAX: u64 = 1024 * 1024;

/// An entry is one thing a directory holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The entry's name.
    pub name: String,
    /// Whether the entry is a directory, as the listing says, without following a symlink.
    pub is_directory: bool,
}

/// A link is a symlink to make: where it goes, and what it leads to.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Link<'path> {
    /// Where the symlink is made.
    pub at: &'path Path,
    /// What the symlink leads to.
    pub to: &'path Path,
}

impl Link<'_> {
    /// Makes the symlink.
    ///
    /// # Errors
    ///
    /// Returns why when it can't be made, such as when something is already there.
    pub(crate) fn make(self) -> Result<(), String> {
        symlink(self.to, self.at).map_err(|error| {
            let (at, to) = (self.at.display(), self.to.display());
            format!("{at} can't be made a link to {to}: {error}.")
        })
    }
}

/// Returns the text of the file at `path`.
///
/// # Errors
///
/// Returns why when the file isn't there, can't be read, isn't UTF-8, or is longer than
/// `TEXT_BYTES_MAX`.
pub(crate) fn read_text(path: &Path) -> Result<String, String> {
    let text = read_text_if_there(path)?;
    text.ok_or_else(|| format!("{} isn't there.", path.display()))
}

/// Returns the text of the file at `path`, or `None` when nothing is there.
///
/// # Errors
///
/// Returns why when the file can't be read, isn't UTF-8, or is longer than `TEXT_BYTES_MAX`.
pub(crate) fn read_text_if_there(path: &Path) -> Result<Option<String>, String> {
    let unreadable = |error: io::Error| format!("{} can't be read: {error}.", path.display());
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unreadable(error)),
    };
    let mut text = String::new();
    // One byte past the most is asked for, so a longer file is told from one that just fits.
    file.take(TEXT_BYTES_MAX + 1).read_to_string(&mut text).map_err(unreadable)?;
    if u64::try_from(text.len()).is_ok_and(|length| length <= TEXT_BYTES_MAX) {
        Ok(Some(text))
    } else {
        Err(format!("{} is longer than {TEXT_BYTES_MAX} bytes.", path.display()))
    }
}

/// Writes `text` to the file at `path`, making the directories it needs.
///
/// # Errors
///
/// Returns why when a directory can't be made or the file can't be written.
pub(crate) fn write_text(path: &Path, text: &str) -> Result<(), String> {
    let unwritable = |error| format!("{} can't be written: {error}.", path.display());
    let directory =
        path.parent().ok_or_else(|| format!("{} is in no directory.", path.display()))?;
    fs::create_dir_all(directory).map_err(unwritable)?;
    fs::write(path, text).map_err(unwritable)
}

/// Makes the file at `path`, empty, and returns it open to be written to. A file already there
/// is emptied.
///
/// # Errors
///
/// Returns why when the file can't be made.
pub(crate) fn create(path: &Path) -> Result<File, String> {
    File::create(path).map_err(|error| format!("{} can't be written: {error}.", path.display()))
}

/// Says whether a file is at `path`, or at what a symlink there leads to. Nothing there is no
/// file.
///
/// # Errors
///
/// Returns why when what's there can't be looked at.
pub(crate) fn is_file(path: &Path) -> Result<bool, String> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("{} can't be looked at: {error}.", path.display())),
    }
}

/// Returns what's in `directory`, sorted by name. A directory that isn't there holds nothing.
///
/// # Errors
///
/// Returns why when the directory can't be read, holds a name that isn't UTF-8, or holds more
/// than `entries_max` entries.
pub(crate) fn entries_in(directory: &Path, entries_max: usize) -> Result<Vec<Entry>, String> {
    let unreadable = |error: io::Error| format!("{} can't be read: {error}.", directory.display());
    let listing = match fs::read_dir(directory) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(unreadable(error)),
    };
    let mut entries = Vec::new();
    for entry in listing {
        let entry = entry.map_err(unreadable)?;
        let is_directory = entry.file_type().map_err(unreadable)?.is_dir();
        let name = entry.file_name().to_str().map(String::from);
        let name =
            name.ok_or_else(|| format!("{} holds a name that isn't UTF-8.", directory.display()))?;
        if entries.len() == entries_max {
            return Err(format!("{} holds more than {entries_max} entries.", directory.display()));
        }
        entries.push(Entry { name, is_directory });
    }
    assert!(entries.len() <= entries_max, "the listing stopped at the bound");
    entries.sort_by(|one, other| one.name.cmp(&other.name));
    Ok(entries)
}

/// Removes the directory at `path` and all it holds. Nothing there is already removed.
///
/// # Errors
///
/// Returns why when something there can't be removed.
pub(crate) fn remove_directory(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{} can't be removed: {error}.", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use stella_test_support::{Scratch, write};

    use super::{
        Entry, Link, TEXT_BYTES_MAX, create, entries_in, is_file, read_text, read_text_if_there,
        remove_directory, write_text,
    };

    /// Returns a new, empty directory for the test named `name` to work in.
    fn fresh(name: &str) -> PathBuf {
        Scratch::of("xtask-files").fresh(name)
    }

    /// Returns the most bytes a file may hold and still be read, as a length in memory.
    fn text_bytes_max() -> usize {
        usize::try_from(TEXT_BYTES_MAX).expect("the most is far less than memory can address")
    }

    #[test]
    fn a_file_is_read_up_to_the_most_and_no_further() {
        // A file of exactly the most bytes is read whole. One byte more is refused, and the
        // sentence names the file and the bound.
        let directory = fresh("bound");
        write(&directory, "fits", &vec![b'a'; text_bytes_max()]);
        write(&directory, "over", &vec![b'a'; text_bytes_max() + 1]);
        let fits = read_text(&directory.join("fits")).expect("a file of the most bytes is read");
        assert_eq!(fits.len(), text_bytes_max());
        assert!(fits.bytes().all(|byte| byte == b'a'), "what's read is what was written");
        let over = directory.join("over");
        let sentence = format!("{} is longer than {TEXT_BYTES_MAX} bytes.", over.display());
        assert_eq!(read_text(&over), Err(sentence.clone()));
        assert_eq!(read_text_if_there(&over), Err(sentence));
    }

    #[test]
    fn a_file_that_isnt_there_is_told_from_one_that_cant_be_read() {
        // Nothing at the path is `None` for a file that may be absent, and an error for one that
        // must be there. A file that isn't UTF-8, and a directory, are errors either way.
        let directory = fresh("absent");
        let absent = directory.join("absent");
        assert_eq!(read_text_if_there(&absent), Ok(None));
        assert_eq!(read_text(&absent), Err(format!("{} isn't there.", absent.display())));
        write(&directory, "empty", b"");
        assert_eq!(read_text(&directory.join("empty")), Ok(String::new()));
        write(&directory, "latin", b"caf\xE9");
        let can_not_be_read = format!("{} can't be read: ", directory.join("latin").display());
        for read in [read_text(&directory.join("latin")), read_text(&directory)] {
            let reason = read.expect_err("neither is a file of text");
            let names_it = reason.starts_with(&can_not_be_read)
                || reason.starts_with(&format!("{} can't be read: ", directory.display()));
            assert!(names_it, "{reason} should name what can't be read");
            assert!(reason.ends_with('.'), "{reason} should be a sentence");
        }
    }

    #[test]
    fn text_is_written_with_the_directories_it_needs() {
        // The file's directories don't exist yet. What's written is what's read back, and a
        // second write replaces the first.
        let directory = fresh("write");
        let path = directory.join("one/two/text");
        assert_eq!(write_text(&path, "first\n"), Ok(()));
        assert_eq!(read_text(&path).as_deref(), Ok("first\n"));
        assert_eq!(write_text(&path, "second\n"), Ok(()));
        assert_eq!(read_text(&path).as_deref(), Ok("second\n"));
        // A file can't be written through a file, and the error names the path.
        let through = path.join("under");
        let reason = write_text(&through, "x").expect_err("a file holds no files");
        assert!(reason.starts_with(&format!("{} can't be written: ", through.display())));
    }

    #[test]
    fn a_file_is_made_empty_and_told_from_a_directory() {
        // A file that's made is there and empty, even if it held something. A directory and
        // nothing at all are no file, and neither is an error.
        let directory = fresh("create");
        let path = directory.join("log");
        assert_eq!(is_file(&path), Ok(false));
        write(&directory, "log", b"left from the last run");
        drop(create(&path).expect("a file can be made in the tests' own directory"));
        assert_eq!(read_text(&path), Ok(String::new()));
        assert_eq!(is_file(&path), Ok(true));
        assert_eq!(is_file(&directory), Ok(false));
        let reason = create(&directory.join("none/log")).expect_err("its directory isn't there");
        assert!(reason.contains("can't be written: "), "{reason}");
    }

    #[test]
    fn a_directory_is_listed_in_order_up_to_its_bound() {
        // Two files and a directory, written out of order, are listed by name, with which is a
        // directory. The bound is how many entries are allowed, so three pass at three and
        // fail at two. A directory that isn't there holds nothing.
        let directory = fresh("list");
        write(&directory, "b.rs", b"");
        write(&directory, "c/lib.rs", b"");
        write(&directory, "a.rs", b"");
        let entry = |name: &str, is_directory| Entry { name: String::from(name), is_directory };
        let listed = [entry("a.rs", false), entry("b.rs", false), entry("c", true)];
        assert_eq!(entries_in(&directory, 3), Ok(listed.to_vec()));
        let too_many = format!("{} holds more than 2 entries.", directory.display());
        assert_eq!(entries_in(&directory, 2), Err(too_many));
        assert_eq!(entries_in(&directory.join("absent"), 3), Ok(Vec::new()));
        let file = directory.join("a.rs");
        let reason = entries_in(&file, 3).expect_err("a file holds no entries");
        assert!(reason.starts_with(&format!("{} can't be read: ", file.display())), "{reason}");
    }

    #[test]
    fn a_link_leads_to_what_it_names_and_a_directory_is_removed_whole() {
        // A directory of one file is linked to. The file is read through the link, and a second
        // link in the same place is refused. Removing the directory the link is in leaves what
        // it led to, and removing what's already gone is no error.
        let directory = fresh("link");
        write(&directory, "real/lib.rs", b"//! A library.\n");
        write(&directory, "crate/Cargo.toml", b"");
        let link = Link { at: &directory.join("crate/src"), to: &directory.join("real") };
        assert_eq!(link.make(), Ok(()));
        assert_eq!(
            read_text(&directory.join("crate/src/lib.rs")).as_deref(),
            Ok("//! A library.\n")
        );
        // The sentence names the place the link couldn't be made, which is what's in the way.
        let again = link.make().expect_err("something is already there");
        let names_both = format!(
            "{} can't be made a link to {}: ",
            directory.join("crate/src").display(),
            directory.join("real").display()
        );
        assert!(again.starts_with(&names_both), "{again}");
        assert_eq!(remove_directory(&directory.join("crate")), Ok(()));
        assert_eq!(is_file(&directory.join("crate/Cargo.toml")), Ok(false));
        assert_eq!(is_file(&directory.join("real/lib.rs")), Ok(true));
        assert_eq!(remove_directory(&directory.join("crate")), Ok(()));
    }
}
