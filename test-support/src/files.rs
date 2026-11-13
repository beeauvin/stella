// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Ways to make the files and directories a test works in.
//!
//! Each is a helper: it does one fallible thing a test needs done, and fails the test with a
//! plain reason if it can't, since a test directory is the test's own to write.

use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// A scratch directory is where one group of tests keeps the files it works in.
///
/// It's `tmp/<group>` in the directory Cargo builds into, so it's the same from run to run,
/// `cargo clean` removes it, and two checkouts never share one.
#[derive(Debug)]
pub struct Scratch {
    directory: PathBuf,
}

impl Scratch {
    /// Returns the scratch directory of `group`, which is usually a test file's name.
    ///
    /// # Panics
    ///
    /// Panics if `group` isn't one plain name, or if the test isn't where Cargo builds tests.
    #[must_use]
    pub fn of(group: &str) -> Self {
        assert_plain_name(group);
        // Cargo builds a test into `<target>/<profile>/deps`, and says where only while it
        // compiles the test, so the place is worked out from where the test is running.
        let test = env::current_exe().expect("a running test has a path");
        let deps = test.parent().expect("a test is in a directory");
        let is_deps = deps.file_name().is_some_and(|name| name == "deps");
        assert!(is_deps, "Cargo builds a test into `deps`, not {}", deps.display());
        let profile = deps.parent().expect("`deps` is in a profile's directory");
        let target = profile.parent().expect("a profile's directory is in the target directory");
        Self { directory: target.join("tmp").join(group) }
    }

    /// Returns the scratch directory's path. Nothing has made the directory yet.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.directory
    }

    /// Returns a new, empty directory named `name` in the scratch directory, for one test to
    /// work in.
    ///
    /// A directory left there by an earlier run is removed first, so a test starts from nothing
    /// however the last run ended.
    ///
    /// # Panics
    ///
    /// Panics if `name` isn't one plain name, or if the old directory can't be removed or the
    /// new one can't be made.
    #[must_use]
    pub fn fresh(&self, name: &str) -> PathBuf {
        // The name is checked before anything is removed, so only a directory of the tests' own
        // ever is.
        assert_plain_name(name);
        let path = self.directory.join(name);
        let removed = fs::remove_dir_all(&path);
        let is_gone = match &removed {
            Ok(()) => true,
            // No earlier run left one.
            Err(error) => error.kind() == io::ErrorKind::NotFound,
        };
        assert!(is_gone, "an old test directory is the tests' own to remove: {removed:?}");
        fs::create_dir_all(&path).expect("a test directory is the tests' own to make");
        path
    }
}

/// Asserts that `name` is one plain name: not empty, not `.` or `..`, and with no `/` in it. So
/// joining it to a directory names something directly in that directory.
fn assert_plain_name(name: &str) {
    let mut components = Path::new(name).components();
    let is_plain = matches!(components.next(), Some(Component::Normal(_)));
    assert!(is_plain, "{name:?} should be a plain name");
    assert!(components.next().is_none(), "{name:?} should be one name, with no `/` in it");
    assert!(!name.contains('/'), "{name:?} should be one name, with no `/` in it");
}

/// Writes `contents` to `path` under `directory`, making any directories it needs.
///
/// # Panics
///
/// Panics if `path` isn't names joined by `/`: if it's empty, absolute, starts with `./`, or has
/// a `..` in it. So it can't reach outside `directory`. Panics too if a directory can't be made
/// or the file can't be written.
pub fn write(directory: &Path, path: &str, contents: &[u8]) {
    let mut names_count: u32 = 0;
    for component in Path::new(path).components() {
        let is_name = matches!(component, Component::Normal(_));
        assert!(is_name, "{path:?} should be names joined by `/`, so it stays in its directory");
        names_count += 1;
    }
    assert!(names_count >= 1, "{path:?} should name a file");
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
    assert!(status.success(), "nothing is at {} yet, so a FIFO can be made there", path.display());
}

/// A mode guard gives a file or directory other permissions until it's dropped, when it puts back
/// the ones it found.
///
/// So even a test that fails can't leave behind a directory the next run can't remove.
#[derive(Debug)]
pub struct ModeGuard {
    path: PathBuf,
    /// The permissions the path had before the guard, which dropping it puts back.
    permissions_before: fs::Permissions,
}

impl ModeGuard {
    /// Sets the permissions of what's at `path` to `mode` until the guard is dropped.
    ///
    /// # Panics
    ///
    /// Panics if the permissions can't be read or set.
    #[must_use]
    pub fn new(path: PathBuf, mode: u32) -> Self {
        let metadata = fs::metadata(&path).expect("a test's own file can be looked at");
        let permissions_before = metadata.permissions();
        let locked = fs::set_permissions(&path, fs::Permissions::from_mode(mode));
        locked.expect("a test's own file takes the permissions it's given");
        Self { path, permissions_before }
    }
}

impl Drop for ModeGuard {
    fn drop(&mut self) {
        // A test may have failed already, so this mustn't panic too. If the permissions can't be
        // put back, the next run's `Scratch::fresh` says so when it can't remove the directory.
        let _: io::Result<()> = fs::set_permissions(&self.path, self.permissions_before.clone());
    }
}
