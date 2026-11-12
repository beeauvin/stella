// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Ways to make the files and directories a test works in.
//!
//! Each is a fixture: it does one fallible thing a test needs done, and fails the test with a
//! plain reason if it can't, since a test directory is the test's own to write.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns a new, empty directory named `name` under `parent`, for one test to work in.
///
/// A directory left there by an earlier run is removed first, so a test starts from nothing
/// however the last run ended.
///
/// # Panics
///
/// Panics if the old directory can't be removed or the new one can't be made.
#[must_use]
pub fn fresh_directory(parent: &Path, name: &str) -> PathBuf {
    let path = parent.join(name);
    if path.exists() {
        fs::remove_dir_all(&path).expect("an old test directory is the tests' own to remove");
    }
    fs::create_dir_all(&path).expect("a test directory is the tests' own to make");
    path
}

/// Writes `contents` to `path` under `directory`, making any directories it needs.
///
/// # Panics
///
/// Panics if a directory can't be made or the file can't be written.
pub fn write(directory: &Path, path: &str, contents: &[u8]) {
    let path = directory.join(path);
    let parent = path.parent().expect("a file under a directory is in a directory");
    fs::create_dir_all(parent).expect("a test directory is the tests' own to make directories in");
    fs::write(path, contents).expect("a test directory is the tests' own to write files in");
}

/// Makes a FIFO at `path`. Std has no way to, so this runs `mkfifo`.
///
/// # Panics
///
/// Panics if `mkfifo` can't be run or fails.
pub fn fifo(path: &Path) {
    let status = Command::new("/usr/bin/mkfifo").arg(path).status();
    let status = status.expect("`/usr/bin/mkfifo` is there to run on macOS and Linux");
    assert!(status.success(), "mkfifo {} should succeed", path.display());
}

/// A lock takes away a file's or directory's permissions until it's dropped.
///
/// So even a test that fails can't leave behind a directory the next run can't remove.
#[derive(Debug)]
pub struct Lock {
    path: PathBuf,
}

impl Lock {
    /// Sets the permissions of what's at `path` to `mode` until the lock is dropped.
    ///
    /// # Panics
    ///
    /// Panics if the permissions can't be set.
    #[must_use]
    pub fn new(path: PathBuf, mode: u32) -> Self {
        let locked = fs::set_permissions(&path, fs::Permissions::from_mode(mode));
        locked.expect("a test's own file takes the permissions it's given");
        Self { path }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // A test may have failed already, so this mustn't panic too. If the permissions can't be
        // put back, the next run's `fresh_directory` says so when it can't remove the directory.
        let _: std::io::Result<()> =
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o755));
    }
}
