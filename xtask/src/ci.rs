// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `ci` command: every check that must pass before anything merges.

use std::path::Path;
use std::process::Command;

use crate::lints;

/// A step is one check `ci` runs: the arguments it gives `cargo`.
type Step = &'static [&'static str];

/// These are the checks, in the order they run. A format problem is the quickest to find, and
/// the documentation is built last, once the code it documents is known to be sound.
pub(crate) const STEPS: [Step; 4] = [
    &["fmt", "--check"],
    &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    &["test", "--workspace"],
    &["doc", "--workspace", "--no-deps"],
];

/// Runs every step in the workspace at `root`, then proves the lints bite. Stops at the first
/// that fails.
pub(crate) fn run(root: &Path) -> Result<(), String> {
    for step in STEPS {
        // `-D warnings` is Clippy's own flag in its step. This makes a warning in the
        // documentation fail its step too, and the other steps never run rustdoc on their own.
        let status = Command::new("cargo")
            .args(step)
            .current_dir(root)
            .env("RUSTDOCFLAGS", "-D warnings")
            .status()
            .map_err(|error| format!("`cargo` can't be run: {error}."))?;
        if !status.success() {
            return Err(format!("`cargo {}` failed.", step.join(" ")));
        }
    }
    lints::run(root)
}
