// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks of the workspace that no tool makes.
//!
//! The workspace declares one set of lints, but nothing makes a crate inherit it: a crate whose
//! manifest lacks `[lints] workspace = true` is built with no lints at all, and passes. Nothing
//! makes a crate root forbid `unsafe` either, or stops an inner `#![allow(..)]` from switching a
//! lint off for a whole crate. And a crate that isn't a member of the workspace isn't built or
//! tested with it. These checks hold every crate to all four, and every source file to carrying
//! the license.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use stella_checks::{members, members_in, root, text_of};

/// This is the most files and directories a walk here will look at. It's far more than the
/// repository holds, and it keeps the walk bounded.
const ENTRIES_MAX: usize = 10_000;

/// These are the lines every source file starts with.
const LICENSE: &str = "\
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
";

/// These are what every crate takes from the workspace, each as the line its manifest holds.
const INHERITED: [&str; 6] = [
    "version.workspace = true",
    "edition.workspace = true",
    "rust-version.workspace = true",
    "license.workspace = true",
    "repository.workspace = true",
    "publish.workspace = true",
];

/// Returns every file under `directory` that's `wanted`, sorted by path. It doesn't go into
/// `target` or into anything whose name starts with `.`, where no crate lives. It walks with a
/// stack rather than recursion, and it stops with a panic past `ENTRIES_MAX`.
fn files_under(directory: &Path, wanted: Wanted) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut directories = vec![directory.to_path_buf()];
    let mut seen_count: usize = 0;
    while let Some(next) = directories.pop() {
        for entry in fs::read_dir(&next).expect("a directory of the repository can be read") {
            let path = entry.expect("an entry of the repository can be read").path();
            seen_count += 1;
            assert!(seen_count <= ENTRIES_MAX, "more than {ENTRIES_MAX} entries in the repository");
            let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
            if name.starts_with('.') || name == "target" {
                continue;
            }
            let is_wanted = match wanted {
                Wanted::Named(wanted_name) => name == wanted_name,
                Wanted::WithExtension(extension) => path.extension() == Some(OsStr::new(extension)),
            };
            if path.is_dir() {
                directories.push(path);
            } else if is_wanted {
                found.push(path);
            } else {
                // Any other file isn't one that was asked for.
            }
        }
    }
    found.sort();
    found
}

/// Which files a walk is looking for.
#[derive(Clone, Copy)]
enum Wanted {
    /// Files with exactly this name.
    Named(&'static str),
    /// Files with this extension.
    WithExtension(&'static str),
}

/// Says whether `manifest`, a crate's manifest, inherits the workspace's lints: whether it has a
/// `[lints]` table whose first line is `workspace = true`.
fn inherits_lints(manifest: &str) -> bool {
    let mut lines = manifest.lines().map(str::trim);
    lines.by_ref().any(|line| line == "[lints]") && lines.next() == Some("workspace = true")
}

/// Returns the first line of `source` that switches a lint off, or expects one, for a whole crate
/// or module: an inner `#![allow(..)]` or `#![expect(..)]`.
fn blanket_lint(source: &str) -> Option<&str> {
    let is_blanket = |line: &&str| {
        let line = line.trim_start();
        line.starts_with("#![allow(")
            || line.starts_with("#![expect(")
            || line.starts_with("#![cfg_attr(")
                && (line.contains("allow(") || line.contains("expect("))
    };
    source.lines().find(is_blanket)
}

#[test]
fn every_crate_is_a_member_of_the_workspace() {
    // Every manifest in the repository, but the root's, is a crate's. Each must be listed as a
    // member, or it's never built, linted or tested with the rest. And every member listed must
    // have its manifest.
    let members = members();
    let manifests = files_under(root(), Wanted::Named("Cargo.toml"));
    let mut crates_count: u32 = 0;
    for manifest in &manifests {
        let directory = manifest.parent().expect("a manifest is in a directory");
        if directory == root() {
            continue;
        }
        let relative = directory.strip_prefix(root()).expect("the crate is under the root");
        let relative = relative.to_str().expect("a crate's path is UTF-8");
        let listed = members.iter().any(|member| member == relative);
        assert!(listed, "{relative} has a manifest, but isn't a member of the workspace");
        crates_count += 1;
    }
    assert!(crates_count > 0, "found no crates in the repository");
    for member in &members {
        let manifest = root().join(member).join("Cargo.toml");
        assert!(manifest.is_file(), "{member} is a member of the workspace with no manifest");
    }
}

#[test]
fn every_crate_inherits_the_workspace() {
    // Each member's manifest must take the lints from the workspace, or it's built with none,
    // and take its version, edition, Rust version, license, repository and `publish` from it too,
    // so there's one of each.
    for member in members() {
        let manifest = text_of(&root().join(&member).join("Cargo.toml"));
        assert!(
            inherits_lints(&manifest),
            "{member}'s manifest must hold `[lints]` and then `workspace = true`"
        );
        for line in INHERITED {
            let inherits = manifest.lines().any(|found| found.trim() == line);
            assert!(inherits, "{member}'s manifest must hold `{line}`");
        }
    }
}

#[test]
fn every_crate_root_forbids_unsafe() {
    // A crate's roots are its `src/lib.rs` and its `src/main.rs`. Each one a member has must
    // forbid `unsafe`, and every member has at least one.
    for member in members() {
        let source = root().join(&member).join("src");
        let roots: Vec<PathBuf> = ["lib.rs", "main.rs"]
            .iter()
            .map(|name| source.join(name))
            .filter(|path| path.is_file())
            .collect();
        assert!(!roots.is_empty(), "{member} has neither a `src/lib.rs` nor a `src/main.rs`");
        for crate_root in roots {
            let forbids =
                text_of(&crate_root).lines().any(|line| line == "#![forbid(unsafe_code)]");
            assert!(forbids, "{} must hold `#![forbid(unsafe_code)]`", crate_root.display());
        }
    }
}

#[test]
fn no_file_switches_a_lint_off_for_a_whole_crate_or_module() {
    // An inner `#![allow(..)]` isn't seen by the lint that refuses `#[allow]`, and an inner
    // `#![expect(..)]` covers far more than the one item an exception is for. No source file may
    // hold either.
    let sources = files_under(root(), Wanted::WithExtension("rs"));
    assert!(!sources.is_empty(), "found no source files in the repository");
    for source in sources {
        let text = text_of(&source);
        let blanket = blanket_lint(&text);
        assert!(blanket.is_none(), "{} holds {blanket:?}", source.display());
    }
}

#[test]
fn every_source_file_carries_the_license() {
    // Each source file starts with the three lines that say it's under the MPL, and then a blank
    // line.
    let sources = files_under(root(), Wanted::WithExtension("rs"));
    assert!(!sources.is_empty(), "found no source files in the repository");
    for source in sources {
        let text = text_of(&source);
        let after = text.strip_prefix(LICENSE);
        assert!(after.is_some(), "{} must start with the license's three lines", source.display());
        let blank = after.is_some_and(|after| after.starts_with('\n'));
        assert!(blank, "{} must have a blank line after the license", source.display());
    }
}

#[test]
fn the_scanners_find_what_theyre_for() {
    // The checks above lean on these scanners, so each is tried on text written here: one case
    // it must find, and one it must not.
    let manifest = "[workspace]\nmembers = [\"blocks\", \"linter/text\"]\nresolver = \"3\"\n";
    assert_eq!(members_in(manifest), ["blocks", "linter/text"]);
    let split = "[workspace]\nmembers = [\n    \"blocks\",\n    \"cli\",\n]\n";
    assert_eq!(members_in(split), ["blocks", "cli"]);
    assert_eq!(members_in("[package]\nname = \"x\"\n"), [""; 0]);

    assert!(inherits_lints("[package]\nname = \"x\"\n\n[lints]\nworkspace = true\n"));
    assert!(!inherits_lints("[package]\nname = \"x\"\n"));
    assert!(!inherits_lints("[lints]\n\n[lints.clippy]\npedantic = \"deny\"\n"));
    assert!(!inherits_lints("[lints.rust]\nworkspace = true\n"));

    for blanket in [
        "#![allow(clippy::pedantic)]",
        "#![allow(clippy::pedantic, reason = \"it was quicker\")]",
        "    #![expect(clippy::as_conversions, reason = \"all of it\")]",
        "#![cfg_attr(test, allow(clippy::unwrap_used))]",
    ] {
        let source = format!("//! A crate.\n\n{blanket}\n\nfn main() {{}}\n");
        assert_eq!(blanket_lint(&source), Some(blanket), "for {blanket}");
    }
    for fine in [
        "#![forbid(unsafe_code)]",
        "#[expect(clippy::as_conversions, reason = \"TS-SIZED-TYPES: one item\")]",
        "// A comment that mentions #![allow(clippy::pedantic)].",
        "#![cfg_attr(docsrs, feature(doc_cfg))]",
    ] {
        let source = format!("//! A crate.\n\n{fine}\n\nfn main() {{}}\n");
        assert_eq!(blanket_lint(&source), None, "for {fine}");
    }
}
