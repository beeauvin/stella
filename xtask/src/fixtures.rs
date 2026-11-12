// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The fixtures the proof compiles, found under `xtask/fixtures/`.
//!
//! A fixture is a small library that breaks one rule on purpose, or keeps every rule. Where it
//! is says what it's there for:
//!
//! - `lints/<tool>/<lint>` is reported as that lint, which the table refuses by name.
//! - `groups/<group>/<lint>` is reported as that lint, which is in a group the table refuses.
//! - `settings/<lint>` is reported as that lint, because of a value `clippy.toml` sets.
//! - `warnings/<lint>` is reported as that compiler lint, which only `-D warnings` refuses.
//! - `allowed/<lint>` does what a lint the table leaves off would report, and passes.
//! - `clean/<name>` breaks nothing, and passes.
//!
//! A fixture is one file, `<name>.rs`, or a directory, `<name>/`, with its `lib.rs`, any modules
//! it needs, and a `manifest.toml` if it needs lines added to its manifest. A second fixture for
//! one lint is told apart by a word after a dot, as in `disallowed_macros.include_str.rs`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::files::read_text_if_there;
use crate::table::{Claim, Level, Tool};

/// This is the most fixtures the proof looks for. It's several times what the lint table can
/// need, and it keeps the search bounded.
pub(crate) const FIXTURES_MAX: usize = 1_000;

/// A fixture is one small library the proof compiles, and what it's there for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fixture {
    /// What the fixture is there to show.
    pub kind: Kind,
    /// The fixture's name: its file's, without `.rs`, or its directory's.
    pub name: String,
    /// The fixture's `lib.rs`.
    pub source: PathBuf,
    /// The fixture's directory, when it's a directory and not one file.
    pub directory: Option<PathBuf>,
    /// The lines to add to the fixture's manifest, when it has any.
    pub manifest_extra: Option<String>,
}

/// A kind is what a fixture is there to show, which is where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A lint the table refuses by name, from this tool.
    Lint(Tool),
    /// A lint in this group, which the table refuses whole.
    Group(String),
    /// A lint that fires because of a value `clippy.toml` sets.
    Setting,
    /// A compiler lint that only `-D warnings` refuses.
    Warning,
    /// A lint the table leaves off.
    Allowed,
    /// Nothing: the fixture keeps every rule.
    Clean,
}

/// What the proof expects of a fixture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Expected {
    /// A diagnostic with this code.
    Reported(String),
    /// No diagnostic at all.
    Silent,
}

impl Fixture {
    /// Returns the lint the fixture is named for: its name, up to any dot.
    pub(crate) fn lint(&self) -> &str {
        self.name.split_once('.').map_or(self.name.as_str(), |(lint, _)| lint)
    }

    /// Returns the name of the directory the proof builds the fixture's crate in, which is also
    /// what a failure calls it.
    pub(crate) fn crate_directory(&self) -> String {
        let place = match &self.kind {
            Kind::Lint(tool) => format!("lints-{}", tool.name()),
            Kind::Group(group) => format!("groups-{group}"),
            Kind::Setting => String::from("settings"),
            Kind::Warning => String::from("warnings"),
            Kind::Allowed => String::from("allowed"),
            Kind::Clean => String::from("clean"),
        };
        let directory = format!("{place}-{}", self.name.replace('.', "-"));
        let is_name =
            directory.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte));
        assert!(is_name, "a fixture is found only by a name that can name a crate");
        directory
    }

    /// Returns what the proof expects of the fixture.
    pub(crate) fn expected(&self) -> Expected {
        match &self.kind {
            Kind::Lint(tool) => Expected::Reported(tool.code(self.lint())),
            Kind::Group(_) | Kind::Setting => Expected::Reported(Tool::Clippy.code(self.lint())),
            Kind::Warning => Expected::Reported(Tool::Rust.code(self.lint())),
            Kind::Allowed | Kind::Clean => Expected::Silent,
        }
    }
}

/// Returns every fixture under `directory`, which is `xtask/fixtures`, sorted by where it is.
///
/// # Errors
///
/// Returns what's wrong when a directory can't be read, holds something that isn't a fixture,
/// or there are more than `FIXTURES_MAX`.
pub(crate) fn fixtures(directory: &Path) -> Result<Vec<Fixture>, String> {
    let mut found = Vec::new();
    for tool in [Tool::Rust, Tool::Rustdoc, Tool::Clippy] {
        add(&mut found, &directory.join("lints").join(tool.name()), &Kind::Lint(tool))?;
    }
    let groups = directory.join("groups");
    for group in entries_in(&groups)? {
        if !group.is_directory || !is_lint_name(&group.name) {
            let path = groups.join(group.name);
            return Err(format!("{} isn't a group's directory.", path.display()));
        }
        add(&mut found, &groups.join(&group.name), &Kind::Group(group.name))?;
    }
    add(&mut found, &directory.join("settings"), &Kind::Setting)?;
    add(&mut found, &directory.join("warnings"), &Kind::Warning)?;
    add(&mut found, &directory.join("allowed"), &Kind::Allowed)?;
    add(&mut found, &directory.join("clean"), &Kind::Clean)?;
    if found.len() > FIXTURES_MAX {
        return Err(format!("there are more than {FIXTURES_MAX} fixtures."));
    }
    Ok(found)
}

/// Adds every fixture in `directory`, each of `kind`, to `found`, sorted by name.
fn add(found: &mut Vec<Fixture>, directory: &Path, kind: &Kind) -> Result<(), String> {
    for entry in entries_in(directory)? {
        let path = directory.join(&entry.name);
        let kind = kind.clone();
        let fixture = if entry.is_directory {
            // The fixture's manifest lines are read now, with the fixture, so what was found is
            // what's built.
            let manifest_extra = read_text_if_there(&path.join("manifest.toml"))?;
            let source = path.join("lib.rs");
            Fixture {
                kind,
                name: entry.name,
                source,
                directory: Some(path.clone()),
                manifest_extra,
            }
        } else {
            let name = entry.name.strip_suffix(".rs").map(String::from);
            let name = name.ok_or_else(|| format!("{} isn't a fixture.", path.display()))?;
            Fixture { kind, name, source: path.clone(), directory: None, manifest_extra: None }
        };
        let is_name = fixture.name.split('.').all(is_lint_name);
        if !is_name {
            return Err(format!("{} isn't named for a lint.", path.display()));
        }
        found.push(fixture);
    }
    Ok(())
}

/// Says whether `name` could be a lint's: lowercase letters, digits and `_`, and at least one.
fn is_lint_name(name: &str) -> bool {
    let is_spelled =
        name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    is_spelled && !name.is_empty()
}

/// An entry is one thing in a directory of fixtures.
struct Entry {
    /// The entry's name.
    name: String,
    /// Whether the entry is a directory, as the listing says, without following a symlink.
    is_directory: bool,
}

/// Returns what's in `directory`, sorted by name. A directory that isn't there holds nothing.
fn entries_in(directory: &Path) -> Result<Vec<Entry>, String> {
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
        entries.push(Entry { name, is_directory });
        if entries.len() > FIXTURES_MAX {
            return Err(format!("{} holds more than {FIXTURES_MAX} entries.", directory.display()));
        }
    }
    entries.sort_by(|one, other| one.name.cmp(&other.name));
    Ok(entries)
}

/// Returns every way `fixtures` and `claims` don't cover each other: a claim with no fixture to
/// prove it, a fixture for a claim the table doesn't make, or no clean fixture to show that the
/// proof can pass. So a lint can't be added without its proof, or its proof outlive it.
pub(crate) fn uncovered(claims: &[Claim], fixtures: &[Fixture]) -> Vec<String> {
    let mut problems = Vec::new();
    for claim in claims {
        if !fixtures.iter().any(|fixture| proves(fixture, claim)) {
            problems.push(format!("`{}` in the lint table has no fixture.", claim.name));
        }
    }
    for fixture in fixtures {
        let needs_claim = match fixture.kind {
            Kind::Lint(_) | Kind::Group(_) | Kind::Allowed => true,
            Kind::Setting | Kind::Warning | Kind::Clean => false,
        };
        if needs_claim && !claims.iter().any(|claim| proves(fixture, claim)) {
            let directory = fixture.crate_directory();
            problems.push(format!("`{directory}` is a fixture for nothing in the lint table."));
        }
    }
    if !fixtures.iter().any(|fixture| fixture.kind == Kind::Clean) {
        problems.push(String::from("there's no clean fixture to show the proof can pass."));
    }
    problems
}

/// Says whether `fixture` is there to prove `claim`.
fn proves(fixture: &Fixture, claim: &Claim) -> bool {
    match (&fixture.kind, claim.level) {
        (Kind::Lint(tool), Level::Refused) => *tool == claim.tool && fixture.lint() == claim.name,
        (Kind::Group(group), Level::GroupRefused) => *group == claim.name,
        (Kind::Allowed, Level::Allowed) => fixture.lint() == claim.name,
        (Kind::Lint(_), Level::GroupRefused | Level::Allowed)
        | (Kind::Group(_), Level::Refused | Level::Allowed)
        | (Kind::Allowed, Level::Refused | Level::GroupRefused)
        | (
            Kind::Setting | Kind::Warning | Kind::Clean,
            Level::Refused | Level::GroupRefused | Level::Allowed,
        ) => false,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Expected, Fixture, Kind, uncovered};
    use crate::table::{Claim, Level, Tool};

    /// Returns a fixture of `kind` named `name`.
    fn fixture(kind: Kind, name: &str) -> Fixture {
        Fixture {
            kind,
            name: String::from(name),
            source: PathBuf::new(),
            directory: None,
            manifest_extra: None,
        }
    }

    /// Returns the claim that Clippy's `name` is at `level`.
    fn claim(name: &str, level: Level) -> Claim {
        Claim { tool: Tool::Clippy, name: String::from(name), level }
    }

    #[test]
    fn a_fixture_expects_what_its_place_says() {
        // Each kind of fixture, with what the proof expects of it and the directory its crate is
        // built in. A word after a dot tells two fixtures for one lint apart, and isn't the lint.
        for (kind, name, directory, expected) in [
            (Kind::Lint(Tool::Rust), "unsafe_code", "lints-rust-unsafe_code", Some("unsafe_code")),
            (
                Kind::Lint(Tool::Rust),
                "unsafe_op_in_unsafe_fn",
                "lints-rust-unsafe_op_in_unsafe_fn",
                Some("E0133"),
            ),
            (
                Kind::Lint(Tool::Rustdoc),
                "broken_intra_doc_links",
                "lints-rustdoc-broken_intra_doc_links",
                Some("rustdoc::broken_intra_doc_links"),
            ),
            (Kind::Lint(Tool::Clippy), "panic", "lints-clippy-panic", Some("clippy::panic")),
            (
                Kind::Group(String::from("pedantic")),
                "must_use_candidate",
                "groups-pedantic-must_use_candidate",
                Some("clippy::must_use_candidate"),
            ),
            (
                Kind::Setting,
                "disallowed_macros.include_str",
                "settings-disallowed_macros-include_str",
                Some("clippy::disallowed_macros"),
            ),
            (
                Kind::Warning,
                "unused_variables",
                "warnings-unused_variables",
                Some("unused_variables"),
            ),
            (Kind::Allowed, "option_if_let_else", "allowed-option_if_let_else", None),
            (Kind::Clean, "library", "clean-library", None),
        ] {
            let fixture = fixture(kind, name);
            let expected =
                expected.map_or(Expected::Silent, |code| Expected::Reported(code.into()));
            assert_eq!(fixture.expected(), expected, "for {name}");
            assert_eq!(fixture.crate_directory(), directory, "for {name}");
        }
    }

    #[test]
    fn every_claim_needs_its_fixture_and_every_fixture_its_claim() {
        // A table of one lint refused, one group refused and one lint left off, and a fixture
        // for each, with a clean one. Nothing is uncovered.
        let claims = [
            claim("panic", Level::Refused),
            claim("pedantic", Level::GroupRefused),
            claim("option_if_let_else", Level::Allowed),
        ];
        let fixtures = [
            fixture(Kind::Lint(Tool::Clippy), "panic"),
            fixture(Kind::Group(String::from("pedantic")), "must_use_candidate"),
            fixture(Kind::Allowed, "option_if_let_else"),
            fixture(Kind::Setting, "too_many_lines"),
            fixture(Kind::Clean, "library"),
        ];
        assert_eq!(uncovered(&claims, &fixtures), [""; 0]);
        // Take any one fixture away, and what it proved is named. A setting's fixture proves
        // nothing in the table, so nothing is said of it.
        let without = |index: usize| {
            let mut fewer = fixtures.to_vec();
            fewer.remove(index);
            uncovered(&claims, &fewer)
        };
        assert_eq!(without(0), ["`panic` in the lint table has no fixture."]);
        assert_eq!(without(1), ["`pedantic` in the lint table has no fixture."]);
        assert_eq!(without(2), ["`option_if_let_else` in the lint table has no fixture."]);
        assert_eq!(without(3), [""; 0]);
        assert_eq!(without(4), ["there's no clean fixture to show the proof can pass."]);
        // Take a claim away, and its fixture is a fixture for nothing. A fixture for the same
        // name from another tool, or of another kind, doesn't prove the claim either.
        assert_eq!(
            uncovered(&claims[1..], &fixtures),
            ["`lints-clippy-panic` is a fixture for nothing in the lint table."]
        );
        let other_tool =
            [fixture(Kind::Lint(Tool::Rust), "panic"), fixture(Kind::Clean, "library")];
        assert_eq!(
            uncovered(&claims[..1], &other_tool),
            [
                "`panic` in the lint table has no fixture.",
                "`lints-rust-panic` is a fixture for nothing in the lint table.",
            ]
        );
    }
}
