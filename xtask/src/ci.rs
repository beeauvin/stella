// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `ci` command: everything that must pass before anything merges.

use std::path::Path;

use crate::cargo::{CLIPPY, REFUSE_WARNINGS, Run};
use crate::lints;

/// A step is one thing `ci` runs: the arguments it gives Cargo, in the pieces they're made of.
type Step = &'static [&'static [&'static str]];

/// These are the steps, in the order they run. A format problem is the quickest to find, and
/// the documentation is built last, once the code it documents is known to be sound.
const STEPS: [Step; 4] = [
    &[&["fmt", "--check"]],
    &[&CLIPPY, &REFUSE_WARNINGS],
    &[&["test", "--workspace"]],
    &[&["doc", "--workspace", "--no-deps"]],
];

/// Runs every step in the workspace at `root`, then proves the lints bite. Stops at the first
/// that fails.
pub(crate) fn run_steps(root: &Path) -> Result<(), String> {
    for step in STEPS {
        let arguments = step.concat();
        let run = Run { directory: root, arguments: &arguments };
        if !run.succeeds()? {
            return Err(format!("`{}` failed.", run.shown()));
        }
    }
    lints::prove(root)
}
