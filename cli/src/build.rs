// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `stella build` builds one program: it checks it, generates its code, writes it as an
//! executable, and saves that in `.build/`.
//!
//! A program goes through every check `stella lint` runs, in the same order, with the same
//! messages. One that's refused touches nothing: nothing is made or written until every check
//! has passed. The report says what the program needs, and it's returned, not written, so
//! whoever runs the command says where it goes.

use std::path::Path;

use stella_arm64::{Code, INSTRUCTION_BYTES, STACK_ALIGNMENT_BYTES, generate};
use stella_macho::{BinaryName, write};

use crate::check::check;
use crate::message::{Place, message};
use crate::save::save;
use crate::source::ProgramPaths;
use crate::stop::Stop;

const _: () = assert!(
    INSTRUCTION_BYTES > 1,
    "the report says `bytes` of code, which is right while the number can't be 1: code comes \
     in whole instructions"
);
const _: () = assert!(
    STACK_ALIGNMENT_BYTES > 1,
    "the report says `bytes` of running memory, which is right while the number can't be 1: \
     the stack comes in whole alignments"
);

/// Builds the program at `typed`, a path as it was typed, under `root`, the directory the
/// command runs in, and returns its report: one line, which says what the program needs.
///
/// # Errors
///
/// Returns the stop: the program is refused, with the message `stella lint` gives it or for a
/// name its binary can't have, or its file can't be read, or its binary can't be saved.
///
/// # Panics
///
/// Never, whatever the program and whatever is on the filesystem. The assertions check the
/// compiler's own work.
pub fn build_one(root: &Path, typed: &[u8]) -> Result<String, Stop> {
    let paths = ProgramPaths::find(root, typed)?;
    let (stored_path, source) = paths.read(root)?;
    let program = check(&source).map_err(|refusal| Stop::Refused(refusal.message(typed)))?;
    // The name comes from the file's own, which the filesystem bounds, not the compiler. So a
    // name the binary can't have refuses the program, like any other problem with its name.
    let name = BinaryName::new(stored_path.name())
        .map_err(|problem| Stop::Refused(message(typed, Place::File, problem)))?;
    let code = generate(&program);
    let executable = write(&code, name);
    save(root, stored_path, executable.bytes())
        .map_err(|failure| Stop::Failed(failure.message()))?;
    Ok(build_report(name, &code))
}

/// Returns the report for the program named `name`, whose code is `code`: one line that says
/// what it needs to run.
fn build_report(name: BinaryName<'_>, code: &Code) -> String {
    let (name, stack_bytes, code_bytes) = (name.as_str(), code.stack_bytes(), code.size_bytes());
    let report = format!(
        "{name} needs {stack_bytes} bytes of running memory and {code_bytes} bytes of code, on \
         top of what macOS needs to run any program.\n"
    );
    assert!(report.ends_with(".\n"), "the report is a sentence on a line of its own");
    assert_eq!(report.matches('\n').count(), 1, "the report is one line");
    report
}
