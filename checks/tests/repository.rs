// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks of what the repository says in more than one place.
//!
//! The Rust it's built with is pinned twice: for mise, which installs it, and for Cargo, which
//! tells Clippy which lints apply. The command that runs every check is named three times: in
//! the readme, in the mise task, and in the workflow. And the ignore file names every directory
//! Git is to keep, so a new crate that isn't named there is never committed, and nothing says
//! so. These checks hold each of those together.

use stella_checks::{members, root, text_of};

/// This is the command that runs every check, as a person types it.
const CI_COMMAND: &str = "cargo xtask ci";

/// This is the step CI runs, which is the mise task that runs `CI_COMMAND`.
const CI_STEP: &str = "mise run ci";

/// This is the workflow CI runs, from the repository's root.
const WORKFLOW: &str = ".github/workflows/ci.yml";

/// A key is the start of a line that holds a version in quotes.
#[derive(Clone, Copy)]
struct Key(&'static str);

/// Returns the version in the first line of `text` that starts with `key`, once the line is
/// trimmed: what's between the quotes that follow it. Returns `None` when no line starts with
/// it, or the quotes aren't there.
fn version_after(text: &str, key: Key) -> Option<&str> {
    let after = text.split('\n').find_map(|line| line.trim_start().strip_prefix(key.0))?;
    let (_, quoted) = after.split_once('"')?;
    let (version, _) = quoted.split_once('"')?;
    Some(version)
}

/// Returns the commands `workflow`, a workflow's text, runs: what follows `- run: ` on each line
/// that starts with it, once the line is trimmed.
fn run_steps(workflow: &str) -> Vec<&str> {
    workflow.split('\n').filter_map(|line| line.trim_start().strip_prefix("- run: ")).collect()
}

/// Returns the lines of `ignore`, an ignore file, that say something: not blank, and not a
/// comment.
fn rules(ignore: &str) -> Vec<&str> {
    ignore.split('\n').filter(|line| !line.is_empty() && !line.starts_with('#')).collect()
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
fn one_rust_is_pinned_for_mise_and_for_cargo() {
    // mise installs the toolchain, here and in CI. Cargo refuses an older one, and tells Clippy
    // which lints apply. A release bumped in one and not the other would have Clippy turn off
    // the lints newer than the Rust it was told.
    let mise = text_of(&root().join("mise.toml"));
    let cargo = text_of(&root().join("Cargo.toml"));
    let for_mise = version_after(&mise, Key("rust = { version ="));
    let for_cargo = version_after(&cargo, Key("rust-version ="));
    assert!(for_mise.is_some(), "mise.toml pins no Rust");
    assert_eq!(for_cargo, for_mise, "Cargo.toml and mise.toml pin different Rusts");
}

#[test]
fn ci_runs_the_command_the_readme_names() {
    // The readme tells a person the one command to run. The mise task runs that command, and
    // the workflow runs the mise task and nothing else, so CI can't check more or less than a
    // person does.
    let readme = text_of(&root().join("readme.md"));
    let mise = text_of(&root().join("mise.toml"));
    let workflow = text_of(&root().join(WORKFLOW));
    let in_readme = readme.split('\n').any(|line| line == CI_COMMAND);
    assert!(in_readme, "the readme doesn't name `{CI_COMMAND}`");
    let mut lines = mise.split('\n').skip_while(|&line| line != "[tasks.ci]");
    assert_eq!(lines.next(), Some("[tasks.ci]"), "mise.toml has no `ci` task");
    let wanted = format!("run = \"{CI_COMMAND}\"");
    let runs_it = lines.take_while(|line| !line.starts_with('[')).any(|line| line == wanted);
    assert!(runs_it, "mise.toml's `ci` task doesn't run `{CI_COMMAND}`");
    assert_eq!(run_steps(&workflow), [CI_STEP], "{WORKFLOW} runs something else");
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
    for directory in [".cargo", ".github", "docs"] {
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
    assert_eq!(version_after(pinned, Key("rust = { version =")), Some("1.2.3"));
    let package = "[workspace.package]\n  rust-version = \"1.2.3\"\n";
    assert_eq!(version_after(package, Key("rust-version =")), Some("1.2.3"));
    assert_eq!(version_after("rust-version = 1.2.3\n", Key("rust-version =")), None);
    assert_eq!(version_after("[tools]\nnode = \"24\"\n", Key("rust = { version =")), None);
    // A longer key that only starts the same way isn't the key.
    assert_eq!(version_after("rust-version = \"1.2.3\"\n", Key("rust =")), None);

    let workflow =
        "steps:\n  - uses: a/b@c\n  - run: one\n  - name: Two\n    run: two\n  - run: three\n";
    // A step that's named isn't found, so the workflow's steps are written the one way.
    assert_eq!(run_steps(workflow), ["one", "three"]);
    assert_eq!(run_steps("steps:\n  - uses: a/b@c\n"), [""; 0]);

    let ignore = "# Everything.\n*\n\n!/cli/\n!/cli/**\n!/docs/\n!/lexer/**\n.DS_Store\n";
    let rules = rules(ignore);
    assert_eq!(rules, ["*", "!/cli/", "!/cli/**", "!/docs/", "!/lexer/**", ".DS_Store"]);
    assert!(lets_through(&rules, "cli"));
    // The directory without its contents, and the contents without the directory, aren't enough.
    assert!(!lets_through(&rules, "docs"));
    assert!(!lets_through(&rules, "lexer"));
    assert!(!lets_through(&rules, "blocks"));
}
