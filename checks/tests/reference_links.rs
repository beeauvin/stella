// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The reference links each rule to the tests that check it, as a link whose text is a test's
//! name and whose target is its file: [`lines`](../../linter/text/src/text_tests.rs). This
//! checks that every such link still leads to a file holding a `#[test]` of that name, so a
//! renamed or moved test can't leave the reference pointing at nothing.

use std::path::{Path, PathBuf};

/// This is the most files the walk will look at. It's far more than the repository holds, and
/// it keeps the walk bounded.
const FILES_MAX: usize = 10_000;

#[test]
fn every_test_link_in_the_reference_leads_to_that_test() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("checks/ is in the root");
    let reference = root.join("docs/reference");
    let mut links_count: u32 = 0;
    for page in files_ending_in(&reference, ".md") {
        let text = std::fs::read_to_string(&page).expect("read a reference page");
        for (name, target) in test_links(&text) {
            let file = page.parent().expect("a page is in a directory").join(target);
            let tests = std::fs::read_to_string(&file).unwrap_or_else(|error| {
                panic!(
                    "{}: `{name}` links to {target}, which can't be read: {error}",
                    page.display()
                )
            });
            assert!(
                test_names(&tests).contains(&name),
                "{}: `{name}` links to {target}, which has no test of that name",
                page.display(),
            );
            links_count += 1;
        }
    }
    // A scanner that silently found nothing would pass, so make sure it found the links.
    assert!(links_count > 0, "found no test links in {}", reference.display());
}

/// Returns every file under `directory` whose name ends in `suffix`. It walks with a stack rather
/// than recursion, and it stops with a panic past `FILES_MAX`.
fn files_ending_in(directory: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut directories = vec![directory.to_path_buf()];
    let mut seen_count: usize = 0;
    while let Some(next) = directories.pop() {
        for entry in std::fs::read_dir(&next).expect("read a directory") {
            let path = entry.expect("read a directory entry").path();
            seen_count += 1;
            assert!(seen_count <= FILES_MAX, "more than {FILES_MAX} files under {directory:?}");
            if path.is_dir() {
                directories.push(path);
            } else if path.to_string_lossy().ends_with(suffix) {
                found.push(path);
            }
        }
    }
    found
}

/// Returns every link in `text` that names a test: [`name`](target), where the target is a
/// `.rs` file.
fn test_links(text: &str) -> Vec<(&str, &str)> {
    let mut links = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[`") {
        rest = &rest[start + "[`".len()..];
        let Some(name_end) = rest.find("`](") else {
            continue;
        };
        let name = &rest[..name_end];
        let after_name = &rest[name_end + "`](".len()..];
        let Some(target_end) = after_name.find(')') else {
            continue;
        };
        let target = &after_name[..target_end];
        let is_a_name =
            name.chars().all(|character| character.is_ascii_alphanumeric() || character == '_');
        if is_a_name && target.ends_with(".rs") {
            links.push((name, target));
        }
    }
    links
}

/// Returns the name of every function marked `#[test]` in `source`, skipping any other
/// attributes between the mark and the `fn`.
fn test_names(source: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut lines = source.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if line != "#[test]" {
            continue;
        }
        let function = lines.by_ref().find(|line| !line.starts_with("#["));
        let name = function
            .and_then(|line| line.strip_prefix("fn "))
            .and_then(|rest| rest.split('(').next());
        if let Some(name) = name {
            names.push(name);
        }
    }
    names
}
