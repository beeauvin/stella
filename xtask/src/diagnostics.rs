// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! What Cargo reports while it builds, read from its JSON messages.
//!
//! With `--message-format=json`, Cargo writes one JSON object on each line of its stdout. A
//! diagnostic is one whose reason is `compiler-message`. This reads the two things the proof
//! needs from it, the crate it's about and the lint that made it, and nothing else.
//!
//! It isn't a JSON parser. It leans on one rule of JSON: a `"` inside a string is always written
//! `\"`, so a key with its quotes and colon can only be found where it's really a key.

/// A diagnostic is one thing the compiler, Clippy or rustdoc said about a crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Diagnostic {
    /// The name of the directory the crate's manifest is in.
    pub crate_directory: String,
    /// The code of the lint that made it, like `clippy::as_conversions`. A message that no lint
    /// made, like the note that says where an error code is explained, has none.
    pub code: Option<String>,
}

/// Returns the diagnostic on `line`, one line of Cargo's JSON, or `None` when the line is some
/// other message, such as an artifact that was built.
///
/// # Errors
///
/// Returns what's wrong when the line is a diagnostic this can't read, so a change in what Cargo
/// writes is met with a reason instead of a proof that quietly sees nothing.
pub(crate) fn diagnostic(line: &str) -> Result<Option<Diagnostic>, String> {
    if !line.contains("\"reason\":\"compiler-message\"") {
        return Ok(None);
    }
    let manifest_path =
        string_after(line, "\"manifest_path\":").ok_or("a diagnostic names no manifest.")?;
    let crate_directory = manifest_path
        .strip_suffix("/Cargo.toml")
        .and_then(|directory| directory.rsplit('/').next())
        .ok_or_else(|| format!("`{manifest_path}` isn't a manifest's path."))?;
    // The diagnostic itself is the object after `"message":`. Cargo writes its keys in the order
    // of the alphabet, so the notes under it, in `children`, come before its own `code`, and
    // each note has a `code` that's null. Nothing after its own has one. So the last `code` on
    // the line is the diagnostic's: either null, or the name inside the object that holds it.
    let (_, message) = line.split_once("\"message\":{").ok_or("a diagnostic holds no message.")?;
    let (_, code) =
        message.rsplit_once("\"code\":").ok_or("a diagnostic's message has no code.")?;
    let code = if code.starts_with("null") {
        None
    } else {
        let name = code.strip_prefix('"').and_then(|name| name.split_once('"'));
        let (name, _) = name.ok_or("a diagnostic's code has no name.")?;
        Some(String::from(name))
    };
    Ok(Some(Diagnostic { crate_directory: String::from(crate_directory), code }))
}

/// Returns the string that's the value right after the first `key` in `text`, without its
/// quotes, or `None` when the key isn't there, isn't followed by a string, or the string holds
/// an escape, which nothing read here should.
fn string_after<'text>(text: &'text str, key: &str) -> Option<&'text str> {
    let (_, after_key) = text.split_once(key)?;
    let after_quote = after_key.strip_prefix('"')?;
    let (value, _) = after_quote.split_once('"')?;
    (!value.contains('\\')).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, diagnostic};

    /// Returns the diagnostic the proof expects of `line`, a line captured from Cargo.
    fn read(line: &str) -> Diagnostic {
        let read = diagnostic(line);
        assert!(matches!(read, Ok(Some(_))), "{line} should be a diagnostic: {read:?}");
        read.expect("it was read, as just asserted").expect("it's a diagnostic, as just asserted")
    }

    #[test]
    fn a_line_that_isnt_a_diagnostic_is_passed_over() {
        // Cargo also says which artifacts it built, and when the build finished.
        for line in [
            "{\"reason\":\"build-finished\",\"success\":false}",
            "{\"reason\":\"compiler-artifact\",\"manifest_path\":\"/w/crates/a/Cargo.toml\"}",
            "",
        ] {
            assert_eq!(diagnostic(line), Ok(None), "for {line}");
        }
    }

    #[test]
    fn a_diagnostic_that_cant_be_read_is_an_error() {
        // Each is a diagnostic with a part missing, so it's an error and not a line passed over.
        for line in [
            "{\"reason\":\"compiler-message\",\"message\":{\"code\":null}}",
            "{\"reason\":\"compiler-message\",\"manifest_path\":\"/w/crates/a/Cargo.toml\"}",
            "{\"reason\":\"compiler-message\",\"manifest_path\":\"/w/a.toml\",\"message\":{}}",
        ] {
            assert!(diagnostic(line).is_err(), "for {line}");
        }
    }

    #[test]
    fn the_crate_and_the_code_are_read_from_a_captured_line() {
        // These lines were captured from `cargo clippy --message-format=json` on the pinned
        // toolchain, and are kept whole in `captured/`, but for the workspace's path, which is
        // written `/workspace`. One is a lint. The other is the note the compiler adds after an
        // error that has a code of its own, which no lint made.
        let captured = std::fs::read_to_string(
            std::path::Path::new(
                &std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo says where the crate is"),
            )
            .join("captured/diagnostics.jsonl"),
        )
        .expect("the captured lines are in the repository");
        let read: Vec<Diagnostic> = captured.split_terminator('\n').map(read).collect();
        let of = |directory: &str, code: Option<&str>| Diagnostic {
            crate_directory: String::from(directory),
            code: code.map(String::from),
        };
        assert_eq!(
            read,
            [
                of("lints-clippy-as_conversions", Some("clippy::as_conversions")),
                of("lints-rust-unsafe_op_in_unsafe_fn", None),
            ]
        );
    }
}
