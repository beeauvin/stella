// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Reading and writing the text files the tools work with.

use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::Path;

/// This is the most bytes of a file a tool reads. It's many times the longest manifest in the
/// repository, and it bounds what one read can hold.
pub(crate) const TEXT_BYTES_MAX: u64 = 1024 * 1024;

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
