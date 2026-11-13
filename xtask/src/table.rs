// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The workspace's lint table, read from the root manifest.
//!
//! The table is the three `[workspace.lints.*]` tables. Each line names a lint or a group of
//! lints and gives it a level, so each line is something the configuration claims.

/// A claim is one line of the lint table: what it names, and what it says of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Claim {
    /// Where the lint is from.
    pub origin: Origin,
    /// The lint's or the group's name, as the table spells it.
    pub name: String,
    /// What the table says of it.
    pub level: Level,
}

/// An origin is where a lint comes from: the compiler, rustdoc or Clippy. Each has a table of
/// its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// The compiler, in `[workspace.lints.rust]`.
    Rust,
    /// Rustdoc, in `[workspace.lints.rustdoc]`.
    Rustdoc,
    /// Clippy, in `[workspace.lints.clippy]`.
    Clippy,
}

impl Origin {
    /// Returns the origin's name, which is its table's and its fixtures' directory's.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Rustdoc => "rustdoc",
            Self::Clippy => "clippy",
        }
    }

    /// Returns the code a diagnostic gives the lint `name` from this origin.
    pub(crate) fn code(self, name: &str) -> String {
        assert!(!name.is_empty(), "a lint has a name");
        assert!(!name.contains("::"), "a lint's name is given without where it's from");
        match (self, name) {
            // The compiler reports this one lint under the error code of the unsafe operation it
            // found, and names the lint only in a note.
            (Self::Rust, "unsafe_op_in_unsafe_fn") => String::from("E0133"),
            (Self::Rust, _) => String::from(name),
            (Self::Rustdoc, _) => format!("rustdoc::{name}"),
            (Self::Clippy, _) => format!("clippy::{name}"),
        }
    }
}

/// A level is what a line of the table says of what it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    /// One lint is refused, with `deny` or `forbid`.
    Refused,
    /// A whole group is refused, at a lower priority than the single lints.
    GroupRefused,
    /// One lint is left off, with `allow`, though its group is refused.
    Allowed,
}

/// Returns every claim `manifest`, the root manifest's text, makes, in the order it makes them.
///
/// # Errors
///
/// Returns the line that isn't a claim the tool can read, so a new way of writing one is met
/// with a reason instead of being passed over.
pub(crate) fn claims(manifest: &str) -> Result<Vec<Claim>, String> {
    let mut claims = Vec::new();
    // The lint table a line is in, when it's in one.
    let mut origin: Option<Origin> = None;
    for line in manifest.split('\n') {
        let line = line.trim();
        if line.starts_with('[') {
            origin = match line {
                "[workspace.lints.rust]" => Some(Origin::Rust),
                "[workspace.lints.rustdoc]" => Some(Origin::Rustdoc),
                "[workspace.lints.clippy]" => Some(Origin::Clippy),
                _ => None,
            };
            continue;
        }
        let Some(origin) = origin else {
            continue;
        };
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let unread = || format!("can't read `{line}`.");
        let (name, level) = claim_of(line).ok_or_else(unread)?;
        // Only Clippy's groups have fixtures of their own. A group of the compiler's would be a
        // claim nothing here knows how to show.
        if level == Level::GroupRefused && origin != Origin::Clippy {
            return Err(unread());
        }
        assert!(is_lint_name(name), "a claim is read only with a name that can be a lint's");
        claims.push(Claim { origin, name: String::from(name), level });
    }
    Ok(claims)
}

/// Returns the name and the level `line`, a line of a lint table, gives, or `None` when it's
/// written some way this doesn't know.
fn claim_of(line: &str) -> Option<(&str, Level)> {
    let (name, value) = line.split_once(" = ")?;
    // A comment after the value says which rule the line enforces.
    let value = value.split_once('#').map_or(value, |(value, _)| value).trim();
    let level = match value {
        "\"deny\"" | "\"forbid\"" => Level::Refused,
        "\"allow\"" => Level::Allowed,
        "{ level = \"deny\", priority = -1 }" => Level::GroupRefused,
        _ => return None,
    };
    is_lint_name(name).then_some((name, level))
}

/// Says whether `name` could be a lint's: lowercase letters, digits and `_`, and at least one.
pub(crate) fn is_lint_name(name: &str) -> bool {
    let is_spelled =
        name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    is_spelled && !name.is_empty()
}

#[cfg(test)]
mod tests {
    use super::{Claim, Level, Origin, claims};

    #[test]
    fn claims_are_the_lines_of_the_three_tables() {
        // A manifest written here, with each way the real table writes a line: a single lint
        // refused, one forbidden, a group, one left off, a comment, and tables that aren't lints.
        let manifest = "[workspace]\nmembers = [\"a\"]\n\n[workspace.lints.rust]\n\
                        unsafe_code = \"deny\"                      # KS-NO-UNSAFE\n\
                        unsafe_op_in_unsafe_fn = \"forbid\"\n\n[workspace.lints.rustdoc]\n\
                        broken_intra_doc_links = \"deny\"\n\n[workspace.lints.clippy]\n\
                        # A comment.\n\
                        pedantic = { level = \"deny\", priority = -1 } # KS-STRICT-LINTS\n\
                        option_if_let_else = \"allow\"              # off\n\
                        as_conversions = \"deny\"\n\n[profile.dev]\npanic = \"abort\"\n";
        let claim = |origin, name: &str, level| Claim { origin, name: String::from(name), level };
        let expected = [
            claim(Origin::Rust, "unsafe_code", Level::Refused),
            claim(Origin::Rust, "unsafe_op_in_unsafe_fn", Level::Refused),
            claim(Origin::Rustdoc, "broken_intra_doc_links", Level::Refused),
            claim(Origin::Clippy, "pedantic", Level::GroupRefused),
            claim(Origin::Clippy, "option_if_let_else", Level::Allowed),
            claim(Origin::Clippy, "as_conversions", Level::Refused),
        ];
        assert_eq!(claims(manifest), Ok(expected.to_vec()));
        assert_eq!(claims("[package]\nname = \"x\"\n"), Ok(Vec::new()));
    }

    #[test]
    fn a_line_written_another_way_is_an_error() {
        // A level the tool doesn't know, and a line with no value, are each named in the error
        // rather than passed over, so the table can't hold a claim nothing checks.
        for line in [
            "as_conversions = \"warn\"",
            "as_conversions",
            "As-Conversions = \"deny\"",
            " = \"deny\"",
        ] {
            let manifest = format!("[workspace.lints.clippy]\n{line}\n");
            let unread = format!("can't read `{}`.", line.trim());
            assert_eq!(claims(&manifest), Err(unread), "for {line}");
        }
        // A group is read in Clippy's table alone, since only Clippy's groups have fixtures.
        let group = "warnings = { level = \"deny\", priority = -1 }";
        let manifest = format!("[workspace.lints.rust]\n{group}\n");
        assert_eq!(claims(&manifest), Err(format!("can't read `{group}`.")));
    }
}
