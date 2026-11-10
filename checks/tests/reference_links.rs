// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The reference links each rule to the tests that check it, as a link whose text is a test's
//! name and whose target is its file: [`lines`](../../linter/text/tests/text.rs). This
//! checks that every such link still leads to a file holding a `#[test]` of that name, so a
//! renamed or moved test can't leave the reference pointing at nothing. A Rust file may be named
//! only in such a link, so a link this check can't read can't slip past it.

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
        let links = test_links(&text).unwrap_or_else(|line| {
            panic!(
                "{}: a Rust file is named outside a test link, [`test_name`](file.rs): {line}",
                page.display()
            )
        });
        for (name, target) in links {
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

/// Returns every test link in `text`, as the test it names and its file: [`name`](file.rs). Any
/// other mention of a Rust file is an error, holding the line it's on, so a link this check can't
/// read can't slip past it.
fn test_links(text: &str) -> Result<Vec<(&str, &str)>, &str> {
    let mut links = Vec::new();
    // Where each test link's target sits in `text`, so every mention of a Rust file can be
    // checked against them.
    let mut targets: Vec<(usize, usize)> = Vec::new();
    // Each turn moves past one `[` and a backtick, so the loop ends.
    let mut searched = 0;
    while let Some(found) = text[searched..].find("[`") {
        let name_start = searched + found + "[`".len();
        searched = name_start;
        let Some(name_length) = text[name_start..].find('`') else {
            continue;
        };
        let name = &text[name_start..name_start + name_length];
        let Some(after_name) = text[name_start + name_length..].strip_prefix("`](") else {
            continue;
        };
        let Some(target_length) = after_name.find(')') else {
            continue;
        };
        let target = &after_name[..target_length];
        if !is_a_test_name(name) {
            continue;
        }
        // A link to a line, or with a title, doesn't end in `.rs`, so it's no test link.
        if !target.ends_with(".rs") {
            continue;
        }
        let target_start = text.len() - after_name.len();
        targets.push((target_start, target_start + target.len()));
        links.push((name, target));
    }
    for (position, _) in text.match_indices(".rs") {
        if !ends_a_rust_file(&text[position + ".rs".len()..]) {
            continue;
        }
        let inside = targets.iter().any(|&(start, end)| start <= position && position < end);
        if !inside {
            let line_start = text[..position].rfind('\n').map_or(0, |newline| newline + 1);
            let line_end =
                text[position..].find('\n').map_or(text.len(), |newline| position + newline);
            return Err(&text[line_start..line_end]);
        }
    }
    Ok(links)
}

/// Says whether `name` could be a test's name: letters, digits and underscores, at least one.
fn is_a_test_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    name.chars().all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Says whether `after` can follow `.rs` at the end of a Rust file's name: the end of the text,
/// or anything that can't go on a name. A `.` goes on one only when more of the name follows it,
/// as in `notes.rs.md`, not when it ends a sentence.
fn ends_a_rust_file(after: &str) -> bool {
    let mut characters = after.chars();
    match characters.next() {
        None => true,
        Some('.') => !characters.next().is_some_and(|next| next.is_ascii_alphanumeric()),
        Some(next) => !(next.is_ascii_alphanumeric() || next == '_' || next == '-'),
    }
}

#[test]
fn every_link_to_a_rust_file_names_a_test() {
    let found = test_links("See [`lines`](../text.rs), [`ascii`](text.rs) and [`a_1`](a.rs).");
    assert_eq!(found, Ok(vec![("lines", "../text.rs"), ("ascii", "text.rs"), ("a_1", "a.rs")]));
    // Links to anything else aren't test links, whatever their text, and nor are names that only
    // start like a Rust file's.
    let found = test_links("See [comments](comments.md), [`lines`](text.md) and [`x`](a.rs.md).");
    assert_eq!(found, Ok(vec![]));
    assert_eq!(test_links("A `.rsx` file, and `parse.rs_old`."), Ok(vec![]));
    for mention in [
        "[`lines`](text.rs#L1)",
        "[lines](text.rs)",
        "[`lines()`](text.rs)",
        "[`lines` and `ascii`](text.rs)",
        "[``](text.rs)",
        "[`a b`](text.rs)",
        "[the tests](../../linter/text/tests/text.rs)",
        "[`lines`](text.rs \"the tests\")",
        "[`lines`](<text.rs>)",
        "[`lines`](text.rs )",
        "[the tests][t]\n\n[t]: text.rs",
        "`text.rs`",
        "text.rs.",
    ] {
        for text in [
            format!("See {mention}, then [`ascii`](text.rs)."),
            format!("See [`ascii`](text.rs), then {mention}"),
        ] {
            // The error is the line that holds the mention.
            let error = test_links(&text).expect_err(&text);
            assert!(text.lines().any(|line| line == error), "for {text:?}");
            assert!(error.contains("text.rs"), "for {text:?}");
        }
    }
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
