// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The fixtures the proof compiles, found under `xtask/fixtures/`.
//!
//! A fixture is a small library that breaks one rule on purpose, or keeps every rule. Where it
//! is says what it's there for:
//!
//! - `lints/<origin>/<lint>` is reported as that lint, which the table refuses by name. The
//!   origin is `rust`, `rustdoc` or `clippy`.
//! - `groups/<group>/<lint>` is reported as that lint, which is in a group the table refuses.
//! - `settings/<lint>` is reported as that lint, because of a value `clippy.toml` sets.
//! - `warnings/<lint>` is reported as that compiler lint, which only `-D warnings` refuses.
//! - `allowed/<lint>` does what a lint the table leaves off would report, and passes.
//! - `clean/<name>` breaks nothing, and passes.
//!
//! A fixture is one file, `<name>.rs`, or a directory, `<name>/`, with its `lib.rs`, any modules
//! it needs, and a `manifest.toml` if it needs lines added to its manifest. A second fixture for
//! one lint is told apart by a word after a dot, as in `disallowed_macros.include_str.rs`.

use std::path::{Path, PathBuf};

use crate::files::{entries_in, read_text_if_there};
use crate::table::{Claim, Level, Origin, is_lint_name};

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
    /// A lint the table refuses by name, from this origin.
    Lint(Origin),
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
            Kind::Lint(origin) => format!("lints-{}", origin.name()),
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

    /// Returns where the fixture is: its directory, or its one file.
    pub(crate) fn place(&self) -> &Path {
        self.directory.as_deref().unwrap_or(&self.source)
    }

    /// Returns what the proof expects of the fixture.
    pub(crate) fn expected(&self) -> Expected {
        match &self.kind {
            Kind::Lint(origin) => Expected::Reported(origin.code(self.lint())),
            Kind::Group(_) | Kind::Setting => Expected::Reported(Origin::Clippy.code(self.lint())),
            Kind::Warning => Expected::Reported(Origin::Rust.code(self.lint())),
            Kind::Allowed | Kind::Clean => Expected::Silent,
        }
    }
}

/// Returns every fixture under `directory`, which is `xtask/fixtures`, sorted by where it is.
///
/// # Errors
///
/// Returns what's wrong when a directory can't be read, holds something that isn't a fixture or
/// two fixtures of one name, or there are more than `FIXTURES_MAX`.
pub(crate) fn fixtures(directory: &Path) -> Result<Vec<Fixture>, String> {
    let mut found = Vec::new();
    for origin in [Origin::Rust, Origin::Rustdoc, Origin::Clippy] {
        add(&mut found, &directory.join("lints").join(origin.name()), &Kind::Lint(origin))?;
    }
    let groups = directory.join("groups");
    for group in entries_in(&groups, FIXTURES_MAX)? {
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
    assert!(found.len() <= FIXTURES_MAX, "the search stopped at the bound");
    refuse_one_name_twice(&found)?;
    Ok(found)
}

/// Refuses two of `fixtures` that would be built as one crate, which a file `x.rs` beside a
/// directory `x` would be, each answering for the other.
fn refuse_one_name_twice(fixtures: &[Fixture]) -> Result<(), String> {
    let mut crates: Vec<(String, &Path)> =
        fixtures.iter().map(|fixture| (fixture.crate_directory(), fixture.place())).collect();
    crates.sort();
    for pair in crates.windows(2) {
        let [(one, one_place), (other, other_place)] = pair else {
            unreachable!("a window of two holds two");
        };
        if one == other {
            return Err(format!(
                "{} and {} are two fixtures of one name.",
                one_place.display(),
                other_place.display()
            ));
        }
    }
    Ok(())
}

/// Adds every fixture in `directory`, each of `kind`, to `found`, sorted by name.
fn add(found: &mut Vec<Fixture>, directory: &Path, kind: &Kind) -> Result<(), String> {
    for entry in entries_in(directory, FIXTURES_MAX)? {
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
        if found.len() == FIXTURES_MAX {
            return Err(format!("there are more than {FIXTURES_MAX} fixtures."));
        }
        found.push(fixture);
    }
    Ok(())
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
        (Kind::Lint(origin), Level::Refused) => {
            *origin == claim.origin && fixture.lint() == claim.name
        }
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

    use stella_test_support::{Scratch, write};

    use super::{Expected, Fixture, Kind, fixtures, uncovered};
    use crate::table::{Claim, Level, Origin};

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
        Claim { origin: Origin::Clippy, name: String::from(name), level }
    }

    #[test]
    fn a_fixture_expects_what_its_place_says() {
        // Each kind of fixture, with what the proof expects of it and the directory its crate is
        // built in. A word after a dot tells two fixtures for one lint apart, and isn't the lint.
        for (kind, name, directory, expected) in [
            (
                Kind::Lint(Origin::Rust),
                "unsafe_code",
                "lints-rust-unsafe_code",
                Some("unsafe_code"),
            ),
            (
                Kind::Lint(Origin::Rust),
                "unsafe_op_in_unsafe_fn",
                "lints-rust-unsafe_op_in_unsafe_fn",
                Some("E0133"),
            ),
            (
                Kind::Lint(Origin::Rustdoc),
                "broken_intra_doc_links",
                "lints-rustdoc-broken_intra_doc_links",
                Some("rustdoc::broken_intra_doc_links"),
            ),
            (Kind::Lint(Origin::Clippy), "panic", "lints-clippy-panic", Some("clippy::panic")),
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
    fn fixtures_are_found_by_where_they_are() {
        // One fixture in each place, as a file or as a directory, with a second for one lint and
        // one with lines for its manifest. Each is found with the kind its place gives it, in
        // the order of the places and then of the names.
        let directory = Scratch::of("xtask-fixtures").fresh("found");
        for file in [
            "lints/rust/unsafe_code.rs",
            "lints/rustdoc/broken_intra_doc_links.rs",
            "lints/clippy/panic.rs",
            "lints/clippy/self_named_module_files/lib.rs",
            "lints/clippy/self_named_module_files/inner.rs",
            "groups/pedantic/must_use_candidate.rs",
            "settings/disallowed_macros.include_str.rs",
            "settings/disallowed_macros.option_env.rs",
            "warnings/unused_variables.rs",
            "allowed/option_if_let_else.rs",
            "clean/library/lib.rs",
        ] {
            write(&directory, file, b"");
        }
        write(&directory, "clean/library/manifest.toml", b"[features]\n");
        let found = fixtures(&directory).expect("every entry is a fixture");
        let crates: Vec<String> = found.iter().map(Fixture::crate_directory).collect();
        assert_eq!(
            crates,
            [
                "lints-rust-unsafe_code",
                "lints-rustdoc-broken_intra_doc_links",
                "lints-clippy-panic",
                "lints-clippy-self_named_module_files",
                "groups-pedantic-must_use_candidate",
                "settings-disallowed_macros-include_str",
                "settings-disallowed_macros-option_env",
                "warnings-unused_variables",
                "allowed-option_if_let_else",
                "clean-library",
            ]
        );
        let panic = Fixture {
            source: directory.join("lints/clippy/panic.rs"),
            ..fixture(Kind::Lint(Origin::Clippy), "panic")
        };
        let library = Fixture {
            source: directory.join("clean/library/lib.rs"),
            directory: Some(directory.join("clean/library")),
            manifest_extra: Some(String::from("[features]\n")),
            ..fixture(Kind::Clean, "library")
        };
        assert_eq!(found.get(2), Some(&panic));
        assert_eq!(found.last(), Some(&library));
        // Nothing there at all is no fixtures, which the cover check then refuses.
        assert_eq!(fixtures(&directory.join("absent")), Ok(Vec::new()));
    }

    #[test]
    fn what_isnt_a_fixture_is_refused() {
        // Each is one entry that isn't a fixture, in a directory of fixtures that's otherwise
        // empty, with the sentence that refuses it.
        for (name, file, refusal) in [
            ("not-rust", "lints/clippy/notes.txt", "lints/clippy/notes.txt isn't a fixture."),
            ("capital", "lints/clippy/Panic.rs", "lints/clippy/Panic.rs isn't named for a lint."),
            ("dash", "settings/too-many.rs", "settings/too-many.rs isn't named for a lint."),
            ("empty-word", "clean/a..b.rs", "clean/a..b.rs isn't named for a lint."),
            ("no-name", "clean/.rs", "clean/.rs isn't named for a lint."),
            ("hidden", "allowed/.hidden/lib.rs", "allowed/.hidden isn't named for a lint."),
            ("group-file", "groups/pedantic.rs", "groups/pedantic.rs isn't a group's directory."),
            ("group-name", "groups/Pedantic/x.rs", "groups/Pedantic isn't a group's directory."),
        ] {
            let directory = Scratch::of("xtask-fixtures").fresh(name);
            write(&directory, file, b"");
            let refused = format!("{}/{refusal}", directory.display());
            assert_eq!(fixtures(&directory), Err(refused), "for {file}");
        }
    }

    #[test]
    fn a_file_and_a_directory_of_one_name_are_refused() {
        // `panic.rs` and `panic/` would both be built as the crate `lints-clippy-panic`. A
        // fixture between them in the listing doesn't hide the pair.
        let directory = Scratch::of("xtask-fixtures").fresh("twice");
        write(&directory, "lints/clippy/panic.rs", b"");
        write(&directory, "lints/clippy/panic.again.rs", b"");
        write(&directory, "lints/clippy/panic/lib.rs", b"");
        let places = directory.join("lints/clippy");
        let refused = format!(
            "{} and {} are two fixtures of one name.",
            places.join("panic").display(),
            places.join("panic.rs").display()
        );
        assert_eq!(fixtures(&directory), Err(refused));
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
            fixture(Kind::Lint(Origin::Clippy), "panic"),
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
        // name from another origin, or of another kind, doesn't prove the claim either.
        assert_eq!(
            uncovered(&claims[1..], &fixtures),
            ["`lints-clippy-panic` is a fixture for nothing in the lint table."]
        );
        let other_origin =
            [fixture(Kind::Lint(Origin::Rust), "panic"), fixture(Kind::Clean, "library")];
        assert_eq!(
            uncovered(&claims[..1], &other_origin),
            [
                "`panic` in the lint table has no fixture.",
                "`lints-rust-panic` is a fixture for nothing in the lint table.",
            ]
        );
    }
}
