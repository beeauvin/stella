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
    /// The tool the lint is from.
    pub tool: Tool,
    /// The lint's or the group's name, as the table spells it.
    pub name: String,
    /// What the table says of it.
    pub level: Level,
}

/// A tool is where a lint comes from, which is also the table it's in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    /// The compiler, in `[workspace.lints.rust]`.
    Rust,
    /// Rustdoc, in `[workspace.lints.rustdoc]`.
    Rustdoc,
    /// Clippy, in `[workspace.lints.clippy]`.
    Clippy,
}

impl Tool {
    /// Returns the tool's name, which is its table's and its fixtures' directory's.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Rustdoc => "rustdoc",
            Self::Clippy => "clippy",
        }
    }

    /// Returns the code a diagnostic gives the lint `name` from this tool.
    pub(crate) fn code(self, name: &str) -> String {
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
    let mut tool: Option<Tool> = None;
    for line in manifest.split('\n') {
        let line = line.trim();
        if line.starts_with('[') {
            tool = match line {
                "[workspace.lints.rust]" => Some(Tool::Rust),
                "[workspace.lints.rustdoc]" => Some(Tool::Rustdoc),
                "[workspace.lints.clippy]" => Some(Tool::Clippy),
                _ => None,
            };
            continue;
        }
        let Some(tool) = tool else {
            continue;
        };
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, level) = claim_of(line).ok_or_else(|| format!("can't read `{line}`."))?;
        claims.push(Claim { tool, name: String::from(name), level });
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
    let is_name = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_' || byte.is_ascii_digit());
    is_name.then_some((name, level))
}

#[cfg(test)]
mod tests {
    use super::{Claim, Level, Tool, claims};

    #[test]
    fn claims_are_the_lines_of_the_three_tables() {
        // A manifest written here, with each way the real table writes a line: a single lint
        // refused, one forbidden, a group, one left off, a comment, and tables that aren't lints.
        let manifest = "[workspace]\nmembers = [\"a\"]\n\n[workspace.lints.rust]\n\
                        unsafe_code = \"deny\"                      # TS-NO-UNSAFE\n\
                        unsafe_op_in_unsafe_fn = \"forbid\"\n\n[workspace.lints.rustdoc]\n\
                        broken_intra_doc_links = \"deny\"\n\n[workspace.lints.clippy]\n\
                        # A comment.\n\
                        pedantic = { level = \"deny\", priority = -1 } # TS-STRICT-LINTS\n\
                        option_if_let_else = \"allow\"              # off\n\
                        as_conversions = \"deny\"\n\n[profile.dev]\npanic = \"abort\"\n";
        let claim = |tool, name: &str, level| Claim { tool, name: String::from(name), level };
        let expected = [
            claim(Tool::Rust, "unsafe_code", Level::Refused),
            claim(Tool::Rust, "unsafe_op_in_unsafe_fn", Level::Refused),
            claim(Tool::Rustdoc, "broken_intra_doc_links", Level::Refused),
            claim(Tool::Clippy, "pedantic", Level::GroupRefused),
            claim(Tool::Clippy, "option_if_let_else", Level::Allowed),
            claim(Tool::Clippy, "as_conversions", Level::Refused),
        ];
        assert_eq!(claims(manifest), Ok(expected.to_vec()));
        assert_eq!(claims("[package]\nname = \"x\"\n"), Ok(Vec::new()));
    }

    #[test]
    fn a_line_written_another_way_is_an_error() {
        // A level the tool doesn't know, and a line with no value, are each named in the error
        // rather than passed over, so the table can't hold a claim nothing checks.
        for line in ["as_conversions = \"warn\"", "as_conversions", "As-Conversions = \"deny\""] {
            let manifest = format!("[workspace.lints.clippy]\n{line}\n");
            assert_eq!(claims(&manifest), Err(format!("can't read `{line}`.")), "for {line}");
        }
    }
}
