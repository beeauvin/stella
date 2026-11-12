// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The reference links each rule to the tests that check it, as a link whose text is a test's
//! name and whose target is its file: [`lines`](../../linter/text/tests/text.rs). This
//! checks that every such link still leads to a file holding a `#[test]` of that name, so a
//! renamed or moved test can't leave the reference pointing at nothing. A Rust file may be named
//! only in such a link, so a link this check can't read can't slip past it.
//!
//! It also checks the links between the reference's own pages: each leads to a page that's
//! there, and the front page links every other page.

#![forbid(unsafe_code)]

use std::ffi::OsStr;
use std::path::PathBuf;

use stella_repository::{Filter, files_under, root, text_of};
use stella_test_support::slice;

/// Returns every page of the reference, each with its text.
fn pages() -> Vec<(PathBuf, String)> {
    let reference = root().join("docs/reference");
    let pages: Vec<(PathBuf, String)> = files_under(&reference, Filter::WithExtension("md"))
        .into_iter()
        .map(|page| {
            let text = text_of(&page);
            (page, text)
        })
        .collect();
    // A walk that silently found nothing would let every test here pass.
    assert!(!pages.is_empty(), "found no pages in {}", reference.display());
    pages
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
    let mut rest = text;
    while let Some((_, after_open)) = rest.split_once("[`") {
        rest = after_open;
        let Some((name, after_name)) = after_open.split_once('`') else {
            continue;
        };
        let Some(after_paren) = after_name.strip_prefix("](") else {
            continue;
        };
        let Some((target, _)) = after_paren.split_once(')') else {
            continue;
        };
        // A link to a line, or with a title, doesn't end in `.rs`, so it's no test link.
        #[expect(
            clippy::case_sensitive_file_extension_comparisons,
            reason = "TS-STRICT-LINTS: a link's target names a file exactly, as the forge follows \
                      it, so `text.RS` isn't a link to `text.rs`"
        )]
        let names_a_rust_file = target.ends_with(".rs");
        if is_a_test_name(name) && names_a_rust_file {
            let target_start = text.len() - after_paren.len();
            targets.push((target_start, target_start + target.len()));
            links.push((name, target));
        }
    }
    for (position, _) in text.match_indices(".rs") {
        if !ends_a_rust_file(slice(text, position + ".rs".len()..text.len())) {
            continue;
        }
        let inside = targets.iter().any(|&(start, end)| start <= position && position < end);
        if !inside {
            let before = slice(text, 0..position);
            let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
            let after = slice(text, position..text.len());
            let line_end = after.find('\n').map_or(text.len(), |newline| position + newline);
            return Err(slice(text, line_start..line_end));
        }
    }
    Ok(links)
}

/// Returns the target of every link in `text` that leads to a page: one that ends in `.md`.
fn page_links(text: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    // Each turn moves past one `](`, so the loop ends.
    let mut rest = text;
    while let Some((_, after_paren)) = rest.split_once("](") {
        rest = after_paren;
        let Some((target, _)) = after_paren.split_once(')') else {
            continue;
        };
        #[expect(
            clippy::case_sensitive_file_extension_comparisons,
            reason = "TS-STRICT-LINTS: a link's target names a file exactly, as the forge follows \
                      it, so `page.MD` isn't a link to `page.md`"
        )]
        let names_a_page = target.ends_with(".md");
        if names_a_page {
            targets.push(target);
        }
    }
    targets
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

/// Returns the name of every function marked `#[test]` in `source`, skipping any other
/// attributes between the mark and the `fn`.
fn test_names(source: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut lines = source.split('\n').map(str::trim);
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

#[test]
fn every_test_link_in_the_reference_leads_to_that_test() {
    // Every page's test links are read, and each must lead to a file with a `#[test]` of the
    // name the link shows. A Rust file named any other way fails here too.
    let mut links_count: u32 = 0;
    for (page, text) in pages() {
        let page_name = page.display();
        let links = test_links(&text);
        let stray = links.as_ref().err();
        assert!(
            stray.is_none(),
            "{page_name}: a Rust file is named outside a test link, [`test_name`](file.rs): \
             {stray:?}"
        );
        for (name, target) in links.expect("no Rust file is named outside a test link") {
            let file = page.parent().expect("a page is in a directory").join(target);
            // The read below says so too, but not which page's link led there.
            assert!(file.is_file(), "{page_name}: `{name}` links to {target}, which isn't there");
            let tests = text_of(&file);
            assert!(
                test_names(&tests).contains(&name),
                "{page_name}: `{name}` links to {target}, which has no test of that name",
            );
            links_count += 1;
        }
    }
    // A scanner that silently found nothing would pass, so make sure it found the links.
    assert!(links_count > 0, "found no test links in the reference");
}

#[test]
fn every_link_between_pages_leads_to_a_page() {
    // Every link whose target ends in `.md` is followed from the page it's on, and must lead to
    // a file that's there.
    let mut links_count: u32 = 0;
    for (page, text) in pages() {
        for target in page_links(&text) {
            let linked = page.parent().expect("a page is in a directory").join(target);
            assert!(
                linked.is_file(),
                "{}: a link leads to {target}, which isn't there",
                page.display()
            );
            links_count += 1;
        }
    }
    assert!(links_count > 0, "found no links between pages in the reference");
}

#[test]
fn the_front_page_links_every_page() {
    // The reference's front page is its table of contents, so a page it doesn't link can't be
    // found from it. Every other page in its directory must be one of its links.
    let front = root().join("docs/reference/readme.md");
    let text = text_of(&front);
    let linked = page_links(&text);
    for (page, _) in pages() {
        if page == front {
            continue;
        }
        let name = page.file_name().and_then(OsStr::to_str).expect("a page's name is UTF-8");
        assert!(linked.contains(&name), "the front page doesn't link {name}");
    }
}

#[test]
fn every_link_to_a_rust_file_names_a_test() {
    // The scanner itself, on text written here: it reads the one shape of test link, takes
    // nothing else for one, and refuses every other way to name a Rust file.
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
            assert!(text.split('\n').any(|line| line == error), "for {text:?}");
            assert!(error.contains("text.rs"), "for {text:?}");
        }
    }
}

#[test]
fn page_links_are_the_targets_that_end_in_md() {
    // The scanner for links between pages, on text written here: any link to a `.md` target is
    // one, whatever its text, and links to anything else aren't.
    let text = "See [comments](comments.md), [`lines`](text.rs), [a site](https://example.com) \
                and [the front page](../readme.md).";
    assert_eq!(page_links(text), ["comments.md", "../readme.md"]);
    assert_eq!(page_links("No links here, only (parentheses) and [brackets]."), [""; 0]);
}

#[test]
fn only_functions_marked_as_tests_are_named() {
    // The scanner for tests, on source written here: a function is a test when `#[test]` marks
    // it, with or without other attributes between, and no other function is.
    let source = "#[test]\nfn one() {\n}\n\nfn helper() {\n}\n\n#[test]\n\
                  #[should_panic(expected = \"x\")]\nfn two() {\n}\n    #[test]\n    \
                  fn three() {}\n";
    assert_eq!(test_names(source), ["one", "two", "three"]);
    assert_eq!(test_names("fn helper() {\n}\n"), [""; 0]);
}
