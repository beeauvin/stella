// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests that the rules page and the code name the same rules.
//!
//! The code cites a rule by its code: beside a lint, in the reason of an `#[expect]`, or in a
//! comment where it departs from one. `docs/rules.md` says what each code means. A code cited
//! and not stated there would send a reader to a page that doesn't answer, and one stated and
//! never cited is a rule nothing holds the code to.

#![forbid(unsafe_code)]

use std::path::Path;

use stella_repository::{Wanted, files_under, root, text_of};

/// This is the page that states each rule, from the repository's root.
const RULES_PAGE: &str = "docs/rules.md";

/// This is how every rule's code starts.
const PREFIX: &str = "TS-";

/// Returns every rule's code in `text`, in the order they come, with repeats. A code is the
/// prefix and then capitals joined by single hyphens, so a hyphen that only trails isn't part of
/// it, and the prefix alone isn't a code.
fn codes_in(text: &str) -> Vec<&str> {
    let mut codes = Vec::new();
    let is_in_code = |character: char| character.is_ascii_uppercase() || character == '-';
    for (start, _) in text.match_indices(PREFIX) {
        // The prefix can turn up inside a code, as the end of one word and the hyphen after it.
        let before = text.get(..start).expect("a match starts between characters");
        if before.ends_with(is_in_code) {
            continue;
        }
        let from_prefix = text.get(start..).expect("a match starts between characters");
        let end = from_prefix.find(|character| !is_in_code(character)).unwrap_or(from_prefix.len());
        let code = from_prefix.get(..end).expect("a code ends between characters");
        let code = code.trim_end_matches('-');
        if code.len() > PREFIX.len() {
            codes.push(code);
        }
    }
    codes
}

/// Returns the codes `page`, the rules page's text, states: each one that's a heading of its
/// own.
fn stated_in(page: &str) -> Vec<&str> {
    page.split('\n').filter_map(|line| line.strip_prefix("### ")).collect()
}

/// Returns every code the repository cites, each with the file that cites it, sorted and
/// without repeats. The rules page states the codes, and this file is about them, so neither
/// counts as citing one.
fn cited() -> Vec<(String, String)> {
    let root = root();
    let mut cited = Vec::new();
    for extension in ["rs", "toml", "md", "yml"] {
        for file in files_under(&root, Wanted::WithExtension(extension)) {
            let relative = file.strip_prefix(&root).expect("the file is under the root");
            if relative == Path::new(RULES_PAGE)
                || relative == Path::new("repository/tests/rules.rs")
            {
                continue;
            }
            let text = text_of(&file);
            let name = relative.display().to_string();
            cited
                .extend(codes_in(&text).into_iter().map(|code| (String::from(code), name.clone())));
        }
    }
    cited.sort();
    cited.dedup();
    // A walk that silently found nothing would let both tests here pass.
    assert!(!cited.is_empty(), "found no rule cited anywhere in the repository");
    cited
}

#[test]
fn every_rule_the_code_cites_is_stated() {
    // Each code cited in a source file, a manifest, a workflow or a page must be a heading of
    // the rules page, so a reader who meets one can look it up.
    let page = text_of(&root().join(RULES_PAGE));
    let stated = stated_in(&page);
    for (code, file) in cited() {
        assert!(
            stated.contains(&code.as_str()),
            "{file} cites `{code}`, which {RULES_PAGE} doesn't state"
        );
    }
}

#[test]
fn every_rule_stated_is_cited() {
    // Each heading of the rules page is a code, stated once, and something in the repository
    // cites it. So the page lists the rules the code is really held to, and no others.
    let page = text_of(&root().join(RULES_PAGE));
    let stated = stated_in(&page);
    assert!(!stated.is_empty(), "{RULES_PAGE} states no rules");
    let cited = cited();
    for (index, heading) in stated.iter().enumerate() {
        assert_eq!(codes_in(heading), [*heading], "`### {heading}` in {RULES_PAGE} isn't a code");
        let is_first = !stated[..index].contains(heading);
        assert!(is_first, "{RULES_PAGE} states `{heading}` twice");
        let is_cited = cited.iter().any(|(code, _)| code == heading);
        assert!(is_cited, "{RULES_PAGE} states `{heading}`, which nothing cites");
    }
}

#[test]
fn the_scanners_find_what_theyre_for() {
    // The tests above lean on these scanners, so each is tried on text written here: codes it
    // must find, and things near to a code that it must not.
    let text = "unsafe_code = \"deny\"  # TS-NO-UNSAFE (each crate root forbids)\n\
                reason = \"TS-SIZED-TYPES: why\"\n// TS-BOUNDED: the limit.\nTS-A, TS-, TS-- and \
                ts-lower, then TS-ASSERT-ARGS.\n";
    let found = ["TS-NO-UNSAFE", "TS-SIZED-TYPES", "TS-BOUNDED", "TS-A", "TS-ASSERT-ARGS"];
    assert_eq!(codes_in(text), found);
    assert_eq!(codes_in("no codes here, and TS alone isn't one"), [""; 0]);
    // A code at the very end of the text is found whole.
    assert_eq!(codes_in("see TS-MACROS"), ["TS-MACROS"]);
    // A word in a code that ends with the prefix's letters doesn't start a second code.
    assert_eq!(codes_in("TS-UNITS-LAST and TS-STRICT-LINTS"), ["TS-UNITS-LAST", "TS-STRICT-LINTS"]);

    let page = "# The rules\n\n## Safety\n\n### TS-BOUNDED\n\nEverything has a limit.\n\n\
                ### TS-MACROS\n\n#### Not a rule\n";
    assert_eq!(stated_in(page), ["TS-BOUNDED", "TS-MACROS"]);
}
