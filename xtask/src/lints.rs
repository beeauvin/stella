// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `lints` command: the proof that the workspace's lints bite.
//!
//! A configuration that's never seen to refuse anything proves nothing by passing. So the proof
//! builds a scratch workspace under `target/`, with the root's own lint table and `clippy.toml`
//! and one crate for each fixture, and runs the command CI runs on it. Then every fixture must
//! have done what it's there for: the ones that break a rule reported as that rule's lint, and
//! the ones that break none reported as nothing. A table line with no fixture fails the proof
//! too, so a lint can't be claimed without being shown.
//!
//! What a pass shows is that CI refuses each fixture. For most lints, that's the table's line at
//! work, since the lint is off without it. A few are warnings even without their line, such as
//! `unknown_lints`, and the `correctness`, `suspicious`, `style`, `complexity` and `perf` groups
//! are too. `-D warnings` would refuse those on its own, so for them a pass doesn't tell the two
//! apart. And `allowed-duplicate-crates` in `clippy.toml` has no fixture, since it takes two
//! versions of a dependency to show, and the workspace has no dependencies.

use std::fs;
use std::io::{self, Write as _};
use std::os::unix::fs::symlink;
use std::path::Path;

use crate::cargo::{CLIPPY, REFUSE_WARNINGS, REPORT_ALL_AS_JSON, Run};
use crate::diagnostics::{Diagnostic, diagnostic};
use crate::files::{read_text, write_text};
use crate::fixtures::{Expected, Fixture, Kind, fixtures, uncovered};
use crate::table::{Tool, claims};

/// This is where the scratch workspace is, from the root.
const SCRATCH: &str = "target/lint-proof";

/// This is where the scratch workspace of the `cargo` group's fixtures is, from the root.
const SCRATCH_CARGO: &str = "target/lint-proof-cargo";

/// These are what every fixture's manifest takes from the scratch workspace, as the crates of
/// the real one do.
const PACKAGE: &str = "version.workspace = true\nedition.workspace = true\n\
                       rust-version.workspace = true\nlicense.workspace = true\n\
                       repository.workspace = true\npublish.workspace = true\n";

/// Proves that the lints of the workspace at `root` bite.
pub(crate) fn prove(root: &Path) -> Result<(), String> {
    let claims = claims(&read_text(&root.join("Cargo.toml"))?)?;
    let fixtures = fixtures(&root.join("xtask/fixtures"))?;
    let problems = uncovered(&claims, &fixtures);
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    // Clippy's `cargo` lints read every manifest in a workspace, and report what they find in
    // any of them against each crate in it. So their fixtures get a workspace of their own. In
    // with the rest, they'd have every other fixture reported as well.
    let is_cargo = |fixture: &Fixture| fixture.kind == Kind::Group(String::from("cargo"));
    let (apart, together): (Vec<Fixture>, Vec<Fixture>) =
        fixtures.iter().cloned().partition(is_cargo);
    let clippy = [&CLIPPY[..], &REPORT_ALL_AS_JSON, &REFUSE_WARNINGS].concat();
    let mut diagnostics = Vec::new();
    for (directory, members) in [(SCRATCH, &together), (SCRATCH_CARGO, &apart)] {
        if members.is_empty() {
            continue;
        }
        let scratch = Scratch { root, directory: &root.join(directory) };
        scratch.write(members)?;
        diagnostics.extend(scratch.diagnostics(&clippy)?);
    }
    // Rustdoc's lints are only reported when the documentation is built, so those fixtures are
    // built again that way, all in one run.
    let is_rustdoc = |fixture: &&Fixture| fixture.kind == Kind::Lint(Tool::Rustdoc);
    let packages: Vec<String> = together.iter().filter(is_rustdoc).map(package_name).collect();
    if !packages.is_empty() {
        let mut doc = [&["doc", "--no-deps"][..], &REPORT_ALL_AS_JSON].concat();
        for package in &packages {
            doc.extend(["--package", package.as_str()]);
        }
        let scratch = Scratch { root, directory: &root.join(SCRATCH) };
        diagnostics.extend(scratch.diagnostics(&doc)?);
    }
    let failures = failures(&fixtures, &diagnostics);
    if !failures.is_empty() {
        return Err(failures.join("\n"));
    }
    let said =
        format!("xtask: each of the {} lint fixtures did what it's there for.\n", fixtures.len());
    io::stderr().lock().write_all(said.as_bytes()).map_err(|error| format!("stderr: {error}."))
}

/// Returns every fixture in `fixtures` that didn't do what it's there for, going by
/// `diagnostics`, each as a sentence. None means the proof holds.
fn failures(fixtures: &[Fixture], diagnostics: &[Diagnostic]) -> Vec<String> {
    let mut failures = Vec::new();
    for fixture in fixtures {
        let directory = fixture.crate_directory();
        let own: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.crate_directory == directory)
            .collect();
        let mut codes: Vec<&str> = own.iter().filter_map(|own| own.code.as_deref()).collect();
        codes.sort_unstable();
        codes.dedup();
        match fixture.expected() {
            Expected::Reported(code) if codes.contains(&code.as_str()) => {}
            Expected::Reported(code) => {
                let found =
                    if codes.is_empty() { String::from("nothing") } else { codes.join(", ") };
                failures
                    .push(format!("`{directory}` should be reported as `{code}`, not {found}."));
            }
            Expected::Silent if own.is_empty() => {}
            Expected::Silent => {
                let found =
                    if codes.is_empty() { String::from("an error") } else { codes.join(", ") };
                failures.push(format!("`{directory}` should pass, and was reported as {found}."));
            }
        }
    }
    assert!(failures.len() <= fixtures.len(), "a fixture fails once at most");
    failures
}

/// A scratch workspace is where the proof builds fixtures. Each path is named, so the workspace
/// that's made anew can't be mistaken for the real one it's made from.
#[derive(Clone, Copy)]
struct Scratch<'proof> {
    /// The real workspace's root, which the lint table and `clippy.toml` are read from.
    root: &'proof Path,
    /// The scratch workspace's own directory, which is under the real one's `target`.
    directory: &'proof Path,
}

impl Scratch<'_> {
    /// Makes the scratch workspace, anew: the root's manifest with the fixtures' crates as its
    /// members, its `clippy.toml`, and a crate for each of `fixtures`.
    fn write(self, fixtures: &[Fixture]) -> Result<(), String> {
        let is_own = self.directory.ends_with(SCRATCH) || self.directory.ends_with(SCRATCH_CARGO);
        assert!(is_own, "only the proof's own directories are replaced");
        assert!(self.directory.starts_with(self.root), "the proof builds inside the workspace");
        let crates = self.directory.join("crates");
        // The crates are made anew, so a fixture that's gone leaves no crate behind. What was
        // built is kept, since it's beside them.
        match fs::remove_dir_all(&crates) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{} can't be removed: {error}.", crates.display())),
        }
        let manifest = scratch_manifest(&read_text(&self.root.join("Cargo.toml"))?)?;
        write_text(&self.directory.join("Cargo.toml"), &manifest)?;
        let clippy = read_text(&self.root.join("clippy.toml"))?;
        write_text(&self.directory.join("clippy.toml"), &clippy)?;
        for fixture in fixtures {
            let directory = crates.join(fixture.crate_directory());
            write_text(&directory.join("Cargo.toml"), &crate_manifest(fixture)?)?;
            // A fixture of several files is linked in as its crate's `src`, so its files are
            // inside the workspace. Clippy's lints about where a module's files are only look
            // there.
            if let Some(files) = &fixture.directory {
                symlink(files, directory.join("src"))
                    .map_err(|error| format!("{} can't be linked to: {error}.", files.display()))?;
            }
        }
        Ok(())
    }

    /// Runs Cargo with `arguments` in the scratch workspace and returns every diagnostic it
    /// reports. That Cargo fails is expected, since most fixtures break a rule: the diagnostics
    /// say what happened.
    fn diagnostics(self, arguments: &[&str]) -> Result<Vec<Diagnostic>, String> {
        let run = Run { directory: self.directory, arguments };
        let log = self.directory.join("cargo-stderr.log");
        let messages = run.messages(&log)?;
        let mut diagnostics = Vec::new();
        for line in messages.split_terminator('\n') {
            diagnostics.extend(diagnostic(line)?);
        }
        // A build that reports nothing at all didn't build the fixtures, whatever its status.
        if diagnostics.is_empty() {
            let shown = run.shown();
            return Err(format!("`{shown}` reported nothing. See {}.", log.display()));
        }
        Ok(diagnostics)
    }
}

/// Returns the name of `fixture`'s package in the scratch workspace.
fn package_name(fixture: &Fixture) -> String {
    format!("proof-{}", fixture.crate_directory())
}

/// Returns the manifest of `fixture`'s crate in the scratch workspace. A fixture of one file is
/// the crate's library where it is. One of several files is linked in as the crate's `src`, which
/// is where Cargo looks for a library unless it's told otherwise.
fn crate_manifest(fixture: &Fixture) -> Result<String, String> {
    let library = if fixture.directory.is_some() {
        String::new()
    } else {
        let source = fixture.source.to_str().filter(|path| !path.contains(['"', '\\']));
        let source = source.ok_or_else(|| {
            format!("{} can't be written into a manifest.", fixture.source.display())
        })?;
        format!("[lib]\npath = \"{source}\"\n\n")
    };
    let extra = fixture.manifest_extra.as_deref().unwrap_or_default();
    let package = package_name(fixture);
    let is_name = package.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte));
    assert!(is_name, "a fixture's name was checked when it was found, so `{package}` is a name");
    Ok(format!(
        "[package]\nname = \"{package}\"\n{PACKAGE}\n{library}[lints]\nworkspace = true\n\n{extra}"
    ))
}

/// Returns `manifest`, the root's, as the scratch workspace's: the same in every line but its
/// members, which are the fixtures' crates. So the fixtures are held to the very lint table the
/// workspace is.
fn scratch_manifest(manifest: &str) -> Result<String, String> {
    let unread = "the root manifest's `members` can't be read.";
    let (before, from_members) = manifest.split_once("members = [").ok_or(unread)?;
    let (_, after) = from_members.split_once(']').ok_or(unread)?;
    Ok(format!("{before}members = [\"crates/*\"]{after}"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{crate_manifest, failures, scratch_manifest};
    use crate::diagnostics::Diagnostic;
    use crate::fixtures::{Fixture, Kind};
    use crate::table::Tool;

    /// Returns a fixture of `kind` named `name`, of one file.
    fn fixture(kind: Kind, name: &str) -> Fixture {
        let source = PathBuf::from(format!("/workspace/xtask/fixtures/{name}.rs"));
        Fixture { kind, name: String::from(name), source, manifest_extra: None, directory: None }
    }

    /// Returns a diagnostic about the crate in `directory`, made by the lint with `code`.
    fn reported(directory: &str, code: Option<&str>) -> Diagnostic {
        Diagnostic { crate_directory: String::from(directory), code: code.map(String::from) }
    }

    #[test]
    fn a_fixture_fails_unless_it_did_what_its_there_for() {
        // One fixture that must be reported as a lint, and one that must pass. Each is tried
        // with what it should have been reported as, and with every way that can go wrong.
        let lint = [fixture(Kind::Lint(Tool::Clippy), "panic")];
        let clean = [fixture(Kind::Clean, "library")];
        let own = reported("lints-clippy-panic", Some("clippy::panic"));
        let other = reported("lints-clippy-panic", Some("clippy::todo"));
        let elsewhere = reported("lints-clippy-todo", Some("clippy::panic"));
        let uncoded = reported("lints-clippy-panic", None);
        // Its own lint is enough, whatever else is said of it.
        assert_eq!(failures(&lint, std::slice::from_ref(&own)), [""; 0]);
        assert_eq!(failures(&lint, &[other.clone(), own, uncoded.clone()]), [""; 0]);
        // Nothing, another lint, a message no lint made, or its lint in another crate, isn't.
        let not_nothing =
            "`lints-clippy-panic` should be reported as `clippy::panic`, not nothing.";
        assert_eq!(failures(&lint, &[]), [not_nothing]);
        assert_eq!(failures(&lint, &[elsewhere]), [not_nothing]);
        assert_eq!(failures(&lint, &[uncoded]), [not_nothing]);
        assert_eq!(
            failures(&lint, &[other]),
            ["`lints-clippy-panic` should be reported as `clippy::panic`, not clippy::todo."]
        );
        // A fixture that must pass fails on anything said of it, by a lint or not.
        assert_eq!(failures(&clean, &[]), [""; 0]);
        assert_eq!(
            failures(&clean, &[reported("clean-library", Some("unused_variables"))]),
            ["`clean-library` should pass, and was reported as unused_variables."]
        );
        assert_eq!(
            failures(&clean, &[reported("clean-library", None)]),
            ["`clean-library` should pass, and was reported as an error."]
        );
    }

    #[test]
    fn the_scratch_workspace_differs_from_the_real_one_only_in_its_members() {
        // Everything but the members is kept, to the byte, however the members are written.
        let split = "[workspace]\nmembers = [\n    \"a\",\n    \"b\",\n]\nresolver = \"3\"\n\n\
                     [workspace.lints.clippy]\npanic = \"deny\"\n";
        let scratch = "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n\n\
                       [workspace.lints.clippy]\npanic = \"deny\"\n";
        assert_eq!(scratch_manifest(split).as_deref(), Ok(scratch));
        let one_line = split.replace("[\n    \"a\",\n    \"b\",\n]", "[\"a\", \"b\"]");
        assert_eq!(scratch_manifest(&one_line).as_deref(), Ok(scratch));
        assert!(scratch_manifest("[package]\nname = \"x\"\n").is_err());
    }

    #[test]
    fn a_fixtures_crate_inherits_the_workspace_as_a_real_crate_does() {
        // A fixture of one file is its crate's library where it is. One of several files is
        // linked in as `src`, so its manifest names no library. Both inherit the lints.
        let one_file = fixture(Kind::Lint(Tool::Clippy), "panic");
        let manifest = crate_manifest(&one_file).expect("the path has nothing to escape");
        assert!(manifest.starts_with("[package]\nname = \"proof-lints-clippy-panic\"\n"));
        assert!(manifest.contains("\n[lib]\npath = \"/workspace/xtask/fixtures/panic.rs\"\n"));
        assert!(manifest.contains("\n[lints]\nworkspace = true\n"));
        assert!(manifest.contains("\nrust-version.workspace = true\n"));
        let directory = Some(PathBuf::from("/workspace/xtask/fixtures/panic"));
        let several = Fixture { directory, ..one_file };
        let manifest = crate_manifest(&several).expect("no path is written");
        assert!(!manifest.contains("[lib]"));
        assert!(manifest.contains("\n[lints]\nworkspace = true\n"));
        // A path that would break out of its string in the manifest is refused.
        let quoted = Fixture { source: PathBuf::from("/a\"b.rs"), ..fixture(Kind::Clean, "x") };
        assert!(crate_manifest(&quoted).is_err());
    }
}
