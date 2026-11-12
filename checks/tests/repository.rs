// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks of what the repository says in more than one place.
//!
//! The Rust it's built with is pinned three times: for mise, for Cargo, and for CI. The checks
//! that must pass are listed twice: in the readme, and in the workflow that runs them. And the
//! ignore file names every directory Git is to keep, so a new crate that isn't named there is
//! never committed, and nothing says so. These checks hold each of those together.

use stella_checks::{members, root, text_of};

/// These are the checks that must pass before anything merges, each as the command that runs it.
const CHECKS: [&str; 4] = [
    "cargo fmt --check",
    "cargo clippy --workspace --all-targets -- -D warnings",
    "cargo test --workspace",
    "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps",
];

/// This is the workflow CI runs, from the repository's root.
const WORKFLOW: &str = ".github/workflows/ci.yml";

/// Returns the version in the first line of `text` that starts with `before`, once the line is
/// trimmed: what's between the quotes that follow it. Returns `None` when no line starts with
/// it, or the quotes aren't there.
fn version_after<'text>(text: &'text str, before: &str) -> Option<&'text str> {
    let after = text.lines().find_map(|line| line.trim_start().strip_prefix(before))?;
    let (_, quoted) = after.split_once('"')?;
    let (version, _) = quoted.split_once('"')?;
    Some(version)
}

/// Returns the lines of `ignore`, an ignore file, that say something: not blank, and not a
/// comment.
fn rules(ignore: &str) -> Vec<&str> {
    ignore.lines().filter(|line| !line.is_empty() && !line.starts_with('#')).collect()
}

/// Says whether `rules`, an ignore file's rules, let `directory` through, with everything in
/// it: the directory first, then its contents, since Git won't look inside a directory it
/// ignores.
fn lets_through(rules: &[&str], directory: &str) -> bool {
    let directory_rule = format!("!/{directory}/");
    let contents_rule = format!("!/{directory}/**");
    rules.windows(2).any(|pair| pair == [directory_rule.as_str(), contents_rule.as_str()])
}

#[test]
fn one_rust_is_pinned_everywhere() {
    // mise installs the toolchain, Cargo refuses an older one and tells Clippy which lints
    // apply, and CI installs its own. A release bumped in one place and not the others would
    // have CI check with a different compiler than the one the code was written against.
    let mise = text_of(&root().join("mise.toml"));
    let cargo = text_of(&root().join("Cargo.toml"));
    let workflow = text_of(&root().join(WORKFLOW));
    let for_mise = version_after(&mise, "rust = { version =");
    let for_cargo = version_after(&cargo, "rust-version =");
    let for_workflow = version_after(&workflow, "RUST_VERSION:");
    assert!(for_mise.is_some(), "mise.toml pins no Rust");
    assert_eq!(for_cargo, for_mise, "Cargo.toml and mise.toml pin different Rusts");
    assert_eq!(for_workflow, for_mise, "{WORKFLOW} and mise.toml pin different Rusts");
}

#[test]
fn the_readme_and_the_workflow_run_the_same_checks() {
    // The readme tells a person which checks to run, and the workflow runs them. Each command
    // must be in both, word for word, and the workflow must run it as a step of its own.
    let readme = text_of(&root().join("readme.md"));
    let workflow = text_of(&root().join(WORKFLOW));
    for check in CHECKS {
        let in_readme = readme.lines().any(|line| line == check);
        assert!(in_readme, "the readme doesn't list `{check}`");
        let step = format!("- run: {check}");
        let in_workflow = workflow.lines().any(|line| line.trim_start() == step);
        assert!(in_workflow, "{WORKFLOW} doesn't run `{check}`");
    }
    let steps_count =
        workflow.lines().filter(|line| line.trim_start().starts_with("- run: ")).count();
    assert_eq!(steps_count, CHECKS.len(), "{WORKFLOW} runs a check the readme doesn't list");
}

#[test]
fn the_ignore_file_lets_every_crate_through() {
    // The ignore file ignores everything, then names what Git is to keep. A crate in a directory
    // it doesn't name would never be committed, and `git status` wouldn't mention it.
    let ignore = text_of(&root().join(".gitignore"));
    let rules = rules(&ignore);
    assert_eq!(rules.first(), Some(&"*"), "the ignore file must start by ignoring everything");
    for member in members() {
        let directory = member.split('/').next().expect("a split has a first piece");
        assert!(
            lets_through(&rules, directory),
            "the ignore file doesn't let {directory}/ through"
        );
    }
    for directory in [".github", "docs"] {
        assert!(
            lets_through(&rules, directory),
            "the ignore file doesn't let {directory}/ through"
        );
    }
}

#[test]
fn the_scanners_find_what_theyre_for() {
    // The checks above lean on these scanners, so each is tried on text written here: cases it
    // must find, and cases it must not.
    let pinned = "[tools]\nrust = { version = \"1.2.3\", components = \"clippy\" }\n";
    assert_eq!(version_after(pinned, "rust = { version ="), Some("1.2.3"));
    assert_eq!(version_after("env:\n  RUST_VERSION: \"1.2.3\"\n", "RUST_VERSION:"), Some("1.2.3"));
    assert_eq!(version_after("rust-version = 1.2.3\n", "rust-version ="), None);
    assert_eq!(version_after("[tools]\nnode = \"24\"\n", "rust = { version ="), None);
    // A longer key that only starts the same way isn't the key.
    assert_eq!(version_after("rust-version = \"1.2.3\"\n", "rust ="), None);

    let ignore = "# Everything.\n*\n\n!/cli/\n!/cli/**\n!/docs/\n!/lexer/**\n.DS_Store\n";
    let rules = rules(ignore);
    assert_eq!(rules, ["*", "!/cli/", "!/cli/**", "!/docs/", "!/lexer/**", ".DS_Store"]);
    assert!(lets_through(&rules, "cli"));
    // The directory without its contents, and the contents without the directory, aren't enough.
    assert!(!lets_through(&rules, "docs"));
    assert!(!lets_through(&rules, "lexer"));
    assert!(!lets_through(&rules, "blocks"));
}
