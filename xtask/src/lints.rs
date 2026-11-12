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
//! Two kinds of fixture are built a second way as well, since each could pass the first for the
//! wrong reason. A fixture of a lint the table leaves off passes by saying nothing, as plain
//! clean code would, so it must also be reported once the table's `allow` lines are gone. And a
//! fixture of something `clippy.toml` sets must also pass under Clippy's own settings, or its
//! being reported wouldn't be the setting's doing.
//!
//! What a pass shows is that CI refuses each fixture. For most lints, that's the table's line at
//! work, since the lint is off without it. A few are warnings even without their line, such as
//! `unknown_lints`, and the `correctness`, `suspicious`, `style`, `complexity` and `perf` groups
//! are too. `-D warnings` would refuse those on its own, so for them a pass doesn't tell the two
//! apart. And `allowed-duplicate-crates` in `clippy.toml` has no fixture, since it takes two
//! versions of a dependency to show, and the workspace has no dependencies.

use std::io::{self, Write as _};
use std::path::Path;

use crate::cargo::{CLIPPY, REFUSE_WARNINGS, REPORT_ALL_AS_JSON, Report, Run};
use crate::diagnostics::{Diagnostic, diagnostic};
use crate::files::{Link, read_text, remove_directory, write_text};
use crate::fixtures::{Expectation, Fixture, Kind, fixtures, uncovered};
use crate::table::{Origin, claims};

/// These are what every fixture's manifest takes from the scratch workspace, as the crates of
/// the real one do.
const INHERITED_KEYS: &str = "version.workspace = true\nedition.workspace = true\n\
                              rust-version.workspace = true\nlicense.workspace = true\n\
                              repository.workspace = true\npublish.workspace = true\n";

/// These are the setups, in the order the proof builds them.
const SETUPS: [Setup; 4] = [
    Setup::AsConfigured,
    Setup::AsConfiguredForCargo,
    Setup::WithoutAllows,
    Setup::WithClippyDefaults,
];

/// A setup is one way the proof builds fixtures. Each has a scratch workspace of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Setup {
    /// The workspace's own lint table and `clippy.toml`, as CI has them.
    AsConfigured,
    /// The same, for the `cargo` group's fixtures alone. Clippy's `cargo` lints read every
    /// manifest in a workspace, and report what they find in any of them against each crate in
    /// it, so in with the rest they'd have every other fixture reported as well.
    AsConfiguredForCargo,
    /// The lint table without its `allow` lines, for the fixtures of the lints it leaves off.
    /// Each must be reported there, or its passing as configured would show nothing.
    WithoutAllows,
    /// Clippy's own settings in place of `clippy.toml`, for the fixtures of what it sets. Each
    /// must pass there, or its being reported as configured wouldn't be the setting's doing.
    WithClippyDefaults,
}

impl Setup {
    /// Returns where the setup's scratch workspace is, from the root.
    const fn directory(self) -> &'static str {
        match self {
            Self::AsConfigured => "target/lint-proof",
            Self::AsConfiguredForCargo => "target/lint-proof-cargo",
            Self::WithoutAllows => "target/lint-proof-unallowed",
            Self::WithClippyDefaults => "target/lint-proof-defaults",
        }
    }

    /// Says whether `fixture` is built in this setup.
    fn builds(self, fixture: &Fixture) -> bool {
        let is_cargo = fixture.kind == Kind::Group(String::from("cargo"));
        match self {
            Self::AsConfigured => !is_cargo,
            Self::AsConfiguredForCargo => is_cargo,
            Self::WithoutAllows => fixture.kind == Kind::Allowed,
            Self::WithClippyDefaults => fixture.kind == Kind::Setting,
        }
    }

    /// Returns what the proof expects of `fixture` in this setup, and the words a failure puts
    /// before the fixture to say which setup it failed in.
    fn expects(self, fixture: &Fixture) -> (Expectation, &'static str) {
        assert!(self.builds(fixture), "a setup is asked only about a fixture it builds");
        match self {
            Self::AsConfigured | Self::AsConfiguredForCargo => (fixture.expected(), ""),
            Self::WithoutAllows => (
                Expectation::Reported(Origin::Clippy.code(fixture.lint())),
                "without its `allow` line, ",
            ),
            Self::WithClippyDefaults => (Expectation::Silent, "under Clippy's own settings, "),
        }
    }
}

/// Proves that the lints of the workspace at `root` bite.
pub(crate) fn prove(root: &Path) -> Result<(), String> {
    // The configuration is read once, so the table the claims are read from is the very text
    // every scratch workspace is made with.
    let manifest = read_text(&root.join("Cargo.toml"))?;
    let settings = read_text(&root.join("clippy.toml"))?;
    let configuration = Configuration { manifest: &manifest, settings: &settings };
    let claims = claims(&manifest)?;
    let fixtures = fixtures(&root.join("xtask/fixtures"))?;
    let problems = uncovered(&claims, &fixtures);
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    for fixture in &fixtures {
        let is_built = SETUPS.iter().any(|setup| setup.builds(fixture));
        assert!(is_built, "every kind of fixture is built in some setup");
    }
    let mut failures = Vec::new();
    for setup in SETUPS {
        let members: Vec<Fixture> =
            fixtures.iter().filter(|fixture| setup.builds(fixture)).cloned().collect();
        if members.is_empty() {
            continue;
        }
        let scratch = ScratchWorkspace { root, setup, configuration };
        failures.extend(scratch.failures(&members)?);
    }
    if !failures.is_empty() {
        return Err(failures.join("\n"));
    }
    let said =
        format!("xtask: each of the {} lint fixtures did what it's there for.\n", fixtures.len());
    io::stderr().lock().write_all(said.as_bytes()).map_err(|error| format!("stderr: {error}."))
}

/// A trial is some fixtures built in one setup.
#[derive(Clone, Copy)]
struct Trial<'proof> {
    /// The setup the fixtures were built in.
    setup: Setup,
    /// The fixtures that were built.
    fixtures: &'proof [Fixture],
}

/// Returns every fixture of `trial` that didn't do what its setup expects of it, going by
/// `diagnostics`, which are what the trial's build reported. Each is a sentence. None means the
/// trial holds.
fn failures_in(trial: Trial<'_>, diagnostics: &[Diagnostic]) -> Vec<String> {
    let mut failures = Vec::new();
    for fixture in trial.fixtures {
        let directory = fixture.crate_directory();
        let own: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.crate_directory == directory)
            .collect();
        let mut codes: Vec<&str> = own.iter().filter_map(|own| own.code.as_deref()).collect();
        codes.sort_unstable();
        codes.dedup();
        let (expected, in_setup) = trial.setup.expects(fixture);
        match expected {
            Expectation::Reported(code) if codes.contains(&code.as_str()) => {}
            Expectation::Reported(code) => {
                let found =
                    if codes.is_empty() { String::from("nothing") } else { codes.join(", ") };
                failures.push(format!(
                    "{in_setup}`{directory}` should be reported as `{code}`, not {found}."
                ));
            }
            Expectation::Silent if own.is_empty() => {}
            Expectation::Silent => {
                let found =
                    if codes.is_empty() { String::from("an error") } else { codes.join(", ") };
                failures.push(format!(
                    "{in_setup}`{directory}` should pass, and was reported as {found}."
                ));
            }
        }
    }
    assert!(failures.len() <= trial.fixtures.len(), "a fixture fails once at most");
    failures
}

/// A configuration is what the real workspace holds its code to.
#[derive(Clone, Copy)]
struct Configuration<'proof> {
    /// The text of the root manifest, which holds the lint table.
    manifest: &'proof str,
    /// The text of `clippy.toml`.
    settings: &'proof str,
}

/// A scratch workspace is where the proof builds the fixtures of one setup.
#[derive(Clone, Copy)]
struct ScratchWorkspace<'proof> {
    /// The real workspace's root, which the scratch workspace is under.
    root: &'proof Path,
    /// The setup the workspace is for, which says where it is and what it's made with.
    setup: Setup,
    /// What the real workspace is configured with, which the setup starts from.
    configuration: Configuration<'proof>,
}

impl ScratchWorkspace<'_> {
    /// Builds `fixtures` in the scratch workspace, and returns every one that didn't do what
    /// the setup expects of it, as a sentence each.
    fn failures(self, fixtures: &[Fixture]) -> Result<Vec<String>, String> {
        self.write(fixtures)?;
        let clippy = [&CLIPPY[..], &REPORT_ALL_AS_JSON, &REFUSE_WARNINGS].concat();
        let mut diagnostics = self.diagnostics(&clippy)?;
        // Rustdoc's lints are only reported when the documentation is built, so those fixtures
        // are built again that way, all in one run.
        let is_rustdoc = |fixture: &&Fixture| fixture.kind == Kind::Lint(Origin::Rustdoc);
        let packages: Vec<String> = fixtures.iter().filter(is_rustdoc).map(package_name).collect();
        if !packages.is_empty() {
            let mut doc = [&["doc", "--no-deps"][..], &REPORT_ALL_AS_JSON].concat();
            for package in &packages {
                doc.extend(["--package", package.as_str()]);
            }
            diagnostics.extend(self.diagnostics(&doc)?);
        }
        Ok(failures_in(Trial { setup: self.setup, fixtures }, &diagnostics))
    }

    /// Makes the scratch workspace, anew: the root's manifest with the fixtures' crates as its
    /// members, its `clippy.toml`, each as the setup has them, and a crate for each of
    /// `fixtures`.
    fn write(self, fixtures: &[Fixture]) -> Result<(), String> {
        let directory = self.root.join(self.setup.directory());
        assert!(directory.starts_with(self.root.join("target")), "the proof builds under `target`");
        let crates = directory.join("crates");
        // The crates are made anew, so a fixture that's gone leaves no crate behind. What was
        // built is kept, since it's beside them.
        remove_directory(&crates)?;
        let manifest = scratch_manifest(self.configuration.manifest)?;
        let manifest = match self.setup {
            Setup::WithoutAllows => without_allows(&manifest),
            Setup::AsConfigured | Setup::AsConfiguredForCargo | Setup::WithClippyDefaults => {
                manifest
            }
        };
        write_text(&directory.join("Cargo.toml"), &manifest)?;
        // Clippy takes its settings from the nearest `clippy.toml`, so an empty one here is its
        // own settings, though the real one is in a directory above.
        let settings = match self.setup {
            Setup::WithClippyDefaults => "",
            Setup::AsConfigured | Setup::AsConfiguredForCargo | Setup::WithoutAllows => {
                self.configuration.settings
            }
        };
        write_text(&directory.join("clippy.toml"), settings)?;
        for fixture in fixtures {
            let crate_directory = crates.join(fixture.crate_directory());
            write_text(&crate_directory.join("Cargo.toml"), &crate_manifest(fixture)?)?;
            // A fixture of several files is linked in as its crate's `src`, so its files are
            // inside the workspace. Clippy's lints about where a module's files are only look
            // there.
            if let Some(files) = &fixture.directory {
                Link { at: &crate_directory.join("src"), to: files }.make()?;
            }
        }
        Ok(())
    }

    /// Runs Cargo with `arguments` in the scratch workspace and returns every diagnostic it
    /// reports.
    fn diagnostics(self, arguments: &[&str]) -> Result<Vec<Diagnostic>, String> {
        let directory = self.root.join(self.setup.directory());
        let run = Run { directory: &directory, arguments };
        let log = directory.join("cargo-stderr.log");
        let report = run.report(&log)?;
        diagnostics_in(&report)
            .map_err(|reason| format!("`{}`: {reason} See {}.", run.shown(), log.display()))
    }
}

/// Returns every diagnostic in `report`, which is what came of building some fixtures.
///
/// # Errors
///
/// Returns why when a diagnostic can't be read, or when Cargo failed without one. A build that
/// fails because a fixture broke a rule says so in a diagnostic, so a failure with none is some
/// other failure, such as a manifest Cargo can't read, and nothing was shown about any fixture.
/// That holds in every setup, the ones whose fixtures should pass among them.
fn diagnostics_in(report: &Report) -> Result<Vec<Diagnostic>, String> {
    let mut diagnostics = Vec::new();
    for line in report.messages.split_terminator('\n') {
        diagnostics.extend(diagnostic(line)?);
    }
    if !report.succeeded && diagnostics.is_empty() {
        return Err(String::from("Cargo failed without reporting a diagnostic."));
    }
    Ok(diagnostics)
}

/// Returns `manifest` without the lines of its lint table that leave a lint off.
fn without_allows(manifest: &str) -> String {
    let is_allow = |line: &&str| {
        let value = line.split_once(" = ").map(|(_, value)| value);
        value.is_some_and(|value| value.starts_with("\"allow\""))
    };
    let kept: Vec<&str> = manifest.split('\n').filter(|line| !is_allow(line)).collect();
    kept.join("\n")
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
        "[package]\nname = \"{package}\"\n{INHERITED_KEYS}\n{library}[lints]\nworkspace = true\n\n\
         {extra}"
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

    use super::{
        SETUPS, Setup, Trial, crate_manifest, diagnostics_in, failures_in, scratch_manifest,
        without_allows,
    };
    use crate::cargo::Report;
    use crate::diagnostics::Diagnostic;
    use crate::fixtures::{Fixture, Kind};
    use crate::table::Origin;

    /// Returns a fixture of `kind` named `name`, of one file.
    fn fixture(kind: Kind, name: &str) -> Fixture {
        let source = PathBuf::from(format!("/workspace/xtask/fixtures/{name}.rs"));
        Fixture { kind, name: String::from(name), source, manifest_extra: None, directory: None }
    }

    /// Returns the failures of `fixtures`, built as CI has them, going by `diagnostics`.
    fn failures(fixtures: &[Fixture], diagnostics: &[Diagnostic]) -> Vec<String> {
        failures_in(Trial { setup: Setup::AsConfigured, fixtures }, diagnostics)
    }

    /// Returns a diagnostic about the crate in `directory`, made by the lint with `code`.
    fn reported(directory: &str, code: Option<&str>) -> Diagnostic {
        Diagnostic { crate_directory: String::from(directory), code: code.map(String::from) }
    }

    #[test]
    fn a_fixture_fails_unless_it_did_what_its_there_for() {
        // One fixture that must be reported as a lint, and one that must pass. Each is tried
        // with what it should have been reported as, and with every way that can go wrong.
        let lint = [fixture(Kind::Lint(Origin::Clippy), "panic")];
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
        let one_file = fixture(Kind::Lint(Origin::Clippy), "panic");
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

    #[test]
    fn each_setup_builds_its_own_fixtures_and_expects_its_own_thing() {
        // One fixture of each kind. As configured, all but the `cargo` group's are built, which
        // is built apart. The two second ways each build one kind, and expect the opposite of
        // what that kind does as configured.
        let lint = fixture(Kind::Lint(Origin::Clippy), "panic");
        let cargo = fixture(Kind::Group(String::from("cargo")), "negative_feature_names");
        let allowed = fixture(Kind::Allowed, "option_if_let_else");
        let setting = fixture(Kind::Setting, "too_many_lines");
        let all = [&lint, &cargo, &allowed, &setting];
        let built = |setup: Setup| -> Vec<&str> {
            let built = all.iter().filter(|fixture| setup.builds(fixture));
            built.map(|fixture| fixture.name.as_str()).collect()
        };
        assert_eq!(built(Setup::AsConfigured), ["panic", "option_if_let_else", "too_many_lines"]);
        assert_eq!(built(Setup::AsConfiguredForCargo), ["negative_feature_names"]);
        assert_eq!(built(Setup::WithoutAllows), ["option_if_let_else"]);
        assert_eq!(built(Setup::WithClippyDefaults), ["too_many_lines"]);
        // A fixture of a lint that's left off passes as configured, and must be reported once
        // it isn't left off.
        let unallowed =
            Trial { setup: Setup::WithoutAllows, fixtures: std::slice::from_ref(&allowed) };
        let own = reported("allowed-option_if_let_else", Some("clippy::option_if_let_else"));
        assert_eq!(failures(std::slice::from_ref(&allowed), &[]), [""; 0]);
        assert_eq!(failures_in(unallowed, &[own]), [""; 0]);
        assert_eq!(
            failures_in(unallowed, &[]),
            ["without its `allow` line, `allowed-option_if_let_else` should be reported as \
             `clippy::option_if_let_else`, not nothing."]
        );
        // A fixture of a setting is reported as configured, and must pass without the setting.
        let defaults =
            Trial { setup: Setup::WithClippyDefaults, fixtures: std::slice::from_ref(&setting) };
        let own = reported("settings-too_many_lines", Some("clippy::too_many_lines"));
        assert_eq!(failures(std::slice::from_ref(&setting), std::slice::from_ref(&own)), [""; 0]);
        assert_eq!(failures_in(defaults, &[]), [""; 0]);
        assert_eq!(
            failures_in(defaults, &[own]),
            ["under Clippy's own settings, `settings-too_many_lines` should pass, and was \
                 reported as clippy::too_many_lines."]
        );
    }

    #[test]
    fn a_build_that_fails_without_a_diagnostic_is_an_error() {
        // What Cargo says of a build with nothing to report, written here since it's only the
        // lines that aren't diagnostics. With success, that's a build that passed. With failure,
        // nothing was shown about any fixture, so it's an error and not a pass.
        let messages = String::from("{\"reason\":\"build-finished\",\"success\":true}\n");
        let passed = Report { messages: messages.clone(), succeeded: true };
        assert_eq!(diagnostics_in(&passed), Ok(Vec::new()));
        let failed = Report { messages, succeeded: false };
        assert_eq!(
            diagnostics_in(&failed),
            Err(String::from("Cargo failed without reporting a diagnostic."))
        );
        let silent = Report { messages: String::new(), succeeded: false };
        assert!(diagnostics_in(&silent).is_err());
        // A diagnostic that can't be read is an error whatever the status.
        let unread = String::from("{\"reason\":\"compiler-message\",\"message\":{}}\n");
        assert!(diagnostics_in(&Report { messages: unread, succeeded: false }).is_err());
    }

    #[test]
    fn every_kind_of_fixture_is_built_in_some_setup() {
        // One fixture of each kind there is. Each is built as configured, in exactly one of the
        // two workspaces that are, and the two second ways build only what they're for.
        for kind in [
            Kind::Lint(Origin::Rust),
            Kind::Lint(Origin::Rustdoc),
            Kind::Lint(Origin::Clippy),
            Kind::Group(String::from("pedantic")),
            Kind::Group(String::from("cargo")),
            Kind::Setting,
            Kind::Warning,
            Kind::Allowed,
            Kind::Clean,
        ] {
            let fixture = fixture(kind, "name");
            let as_configured = [Setup::AsConfigured, Setup::AsConfiguredForCargo]
                .iter()
                .filter(|setup| setup.builds(&fixture))
                .count();
            assert_eq!(as_configured, 1, "for {:?}", fixture.kind);
            let built = SETUPS.iter().filter(|setup| setup.builds(&fixture)).count();
            let second = matches!(fixture.kind, Kind::Allowed | Kind::Setting);
            assert_eq!(built, if second { 2 } else { 1 }, "for {:?}", fixture.kind);
        }
    }

    #[test]
    fn the_allow_lines_are_the_only_ones_taken_out() {
        // A lint table with a lint refused, one left off, and a group. Only the one left off
        // goes, and a line elsewhere that merely holds the word stays.
        let manifest = "[workspace.lints.clippy]\npedantic = { level = \"deny\", priority = -1 }\n\
                        option_if_let_else = \"allow\"              # off\npanic = \"deny\"\n\n\
                        [workspace.package]\ndescription = \"what we allow\"\n";
        let kept = "[workspace.lints.clippy]\npedantic = { level = \"deny\", priority = -1 }\n\
                    panic = \"deny\"\n\n[workspace.package]\ndescription = \"what we allow\"\n";
        assert_eq!(without_allows(manifest), kept);
        assert_eq!(without_allows(kept), kept);
    }
}
