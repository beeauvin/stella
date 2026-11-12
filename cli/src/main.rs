// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `stella` command. For now it has one: `stella lint [path]`. It checks each program's path,
//! then its text with the text lint, then its tokens with the lexer, then their layout with the
//! tokens lint.
//!
//! Everything it says goes to stderr, each message as soon as it's made, so linting every file
//! holds one message at a time, however many there are. If stderr can't be written, there's
//! nowhere left to say so, and the command exits with 2. It writes through its own copy of
//! stderr, because std's own quietly treats a stderr that isn't open for writing as written. A
//! stderr that's closed can be written, though: Rust opens `/dev/null` in its place before `main`
//! runs, so what's written to it is lost, and the status stands.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirEntryExt, MetadataExt};
use std::path::Path;
use std::process::ExitCode;

use stella_cli::{Found, WALK_ENTRIES_MAX, message, shown, walk};
use stella_lexer::{TokenBuffer, lex};
use stella_linter_text::{Kind, Place, Problem, SOURCE_BYTES_MAX, lint, lint_path};
use stella_linter_tokens::lint_tokens;

/// Every program the command checked passed.
const PASSED: u8 = 0;
/// A program was refused.
const REFUSED: u8 = 1;
/// Something outside the program went wrong.
const FAILED: u8 = 2;

// When linting every file, the worst status wins, so they're in order of how bad they are.
const _: () = assert!(PASSED < REFUSED);
const _: () = assert!(REFUSED < FAILED);

/// This is the line every misuse ends with.
const USAGE: &str = "usage: stella lint [path]\n";

/// A source is read into one buffer, made once, with room for one byte more than the largest
/// source, so a source that's too big is seen to be.
const BUFFER_BYTES: usize = SOURCE_BYTES_MAX as usize + 1;

/// The buffers a program is checked in. They're made once, in `main`, and lent to each program in
/// turn, so checking every program in a tree takes the same room as checking one.
struct Buffers {
    /// The source is read into this.
    source: [u8; BUFFER_BYTES],
    /// The source's tokens go in this.
    tokens: TokenBuffer,
}

// The walk's sentences name its bound, so a new bound needs new sentences.
const _: () = assert!(WALK_ENTRIES_MAX == 10_000);

/// A command is what the arguments ask for.
enum Command<'arguments> {
    /// Lint the one program at this path.
    LintOne(&'arguments [u8]),
    /// Lint every program under the directory the command runs in.
    LintEvery,
}

fn main() -> ExitCode {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let mut buffers = Buffers { source: [0_u8; BUFFER_BYTES], tokens: TokenBuffer::new() };
    let Ok(stderr) = io::stderr().as_fd().try_clone_to_owned() else {
        return ExitCode::from(FAILED);
    };
    let mut stderr = File::from(stderr);
    let written = match command(&arguments) {
        Err(sentence) => write!(stderr, "stella: {sentence}\n{USAGE}").map(|()| FAILED),
        Ok(Command::LintEvery) => lint_every(&mut buffers, &mut stderr),
        Ok(Command::LintOne(path)) => {
            let (status, text) = lint_one(path, &mut buffers);
            stderr.write_all(text.as_bytes()).map(|()| status)
        }
    };
    match written {
        Ok(status) => {
            assert!(status <= FAILED);
            ExitCode::from(status)
        }
        Err(_) => ExitCode::from(FAILED),
    }
}

/// Works out what the arguments ask for. A misuse is the sentence that says what's wrong.
fn command(arguments: &[OsString]) -> Result<Command<'_>, String> {
    // There are no options, so any argument that looks like one is a misuse, wherever it is.
    if let Some(option) = arguments.iter().find(|argument| argument.as_bytes().starts_with(b"-")) {
        return Err(format!("`{}` isn't an option; there are none.", shown(option.as_bytes())));
    }
    // An empty command is no command at all.
    let Some((command, paths)) = arguments.split_first().filter(|(command, _)| !command.is_empty())
    else {
        return Err(String::from("a command is needed."));
    };
    if command.as_bytes() != b"lint" {
        return Err(format!("`{}` isn't a command.", shown(command.as_bytes())));
    }
    let path = match paths {
        [] => return Ok(Command::LintEvery),
        [path] => path.as_bytes(),
        _ => return Err(String::from("`lint` takes one path, or none to lint every file.")),
    };
    if path.is_empty() {
        return Err(String::from("the path is empty."));
    }
    assert!(!path.starts_with(b"-"));
    Ok(Command::LintOne(path))
}

/// Lints the program at `path`, checking the path before the file is read. Returns its status
/// and its message, which is empty when it passes.
fn lint_one(path: &[u8], buffers: &mut Buffers) -> (u8, String) {
    assert!(!path.is_empty());
    // A path that isn't UTF-8 can't be kebab, so it's refused like any other bad name.
    let typed = String::from_utf8_lossy(path);
    let source_path = match lint_path(&typed) {
        Ok(source_path) => source_path,
        Err(problem) => return (REFUSED, message(path, b"", problem.place, problem.kind)),
    };
    // On macOS, which ignores case, `hello.stella` can name `Hello.stella`, so the path is found
    // as it's stored before it's checked again.
    let as_stored = match stored(source_path.normalized()) {
        Ok(as_stored) => as_stored,
        Err(Unstored::Symlink) => {
            let problem = Problem::whole_file(Kind::PathSymlink);
            return (REFUSED, message(path, b"", problem.place, problem.kind));
        }
        Err(Unstored::Failed(sentence)) => {
            return (FAILED, message(path, b"", Place::File, sentence));
        }
    };
    lint_stored(path, &as_stored, buffers)
}

/// Lints the program stored at `stored`, which `path` names: its path, then its text, its tokens
/// and their layout. A problem with the path is shown at `stored`, and any other at `path`, as it
/// was given. Returns its status and its message, which is empty when it passes.
fn lint_stored(path: &[u8], stored: &[u8], buffers: &mut Buffers) -> (u8, String) {
    assert!(!stored.is_empty());
    let stored_text = String::from_utf8_lossy(stored);
    let stored_path = match lint_path(&stored_text) {
        Ok(stored_path) => stored_path,
        Err(problem) => return (REFUSED, message(stored, b"", problem.place, problem.kind)),
    };
    let source = match read(stored_path.normalized(), &mut buffers.source) {
        Ok(source) => source,
        Err(sentence) => return (FAILED, message(path, b"", Place::File, sentence)),
    };
    let (status, text) = match check(source, &mut buffers.tokens) {
        Ok(()) => (PASSED, String::new()),
        Err((place, sentence)) => (REFUSED, message(path, source, place, sentence)),
    };
    // A program that passes says nothing, and one that doesn't always says why.
    assert_eq!(status == PASSED, text.is_empty());
    (status, text)
}

/// Checks `source`'s text, then its tokens, lexed into `buffer`, then their layout, and stops at
/// the first check that fails. Returns where its problem is and the sentence that says what it
/// is. Each check takes what only the one before it makes, so they can't run in another order,
/// and an earlier check's problem always comes first.
fn check(source: &[u8], buffer: &mut TokenBuffer) -> Result<(), (Place, String)> {
    assert!(source.len() <= BUFFER_BYTES);
    let text = lint(source).map_err(|problem| (problem.place, problem.kind.to_string()))?;
    // Only text that passed is lexed, so the text lint's limits hold from here on.
    assert!(source.len() <= SOURCE_BYTES_MAX as usize);
    let lexed = lex(text, buffer);
    let tokens =
        lexed.map_err(|problem| (Place::At(problem.position), problem.kind.to_string()))?;
    let laid_out = lint_tokens(tokens);
    laid_out.map_err(|problem| (Place::At(problem.position), problem.kind.to_string()))?;
    Ok(())
}

/// Unstored says why a path couldn't be found as it's stored.
enum Unstored {
    /// A part of the path is a symlink.
    Symlink,
    /// The path couldn't be followed, for the reason this sentence gives.
    Failed(String),
}

/// Returns `path`, which the path lint passed, with each name in it as it's stored. Each name is
/// looked up in its directory, so no symlink is followed and nothing is resolved to an absolute
/// path, which can be too long to resolve. A symlink anywhere in it is refused, before any name
/// in it is checked.
fn stored(path: &str) -> Result<Vec<u8>, Unstored> {
    assert!(!path.is_empty());
    assert!(!path.starts_with('/'));
    let failed = |error: io::Error| Unstored::Failed(file_error(&error));
    let mut stored: Vec<u8> = Vec::new();
    // Each part's device is compared with its directory's, so a mount point can be told apart.
    let mut directory_device = fs::symlink_metadata(".").map_err(failed)?.dev();
    // This counts the entries looked at across every directory on the way, to keep it bounded.
    let mut entries_count: u32 = 0;
    for part in path.split('/') {
        assert!(!part.is_empty());
        let directory =
            if stored.is_empty() { OsStr::new(".") } else { OsStr::from_bytes(&stored) };
        let metadata = fs::symlink_metadata(Path::new(directory).join(part)).map_err(failed)?;
        if metadata.file_type().is_symlink() {
            return Err(Unstored::Symlink);
        }
        // A mount point is listed with the inode of what it covers, so it's found by its name.
        let by_name = Wanted::Name { device: metadata.dev(), inode: metadata.ino() };
        let wanted = if metadata.dev() == directory_device {
            Wanted::Inode(metadata.ino())
        } else {
            by_name
        };
        let entries_before = entries_count;
        let mut found = stored_name(directory, part, wanted, &mut entries_count)?;
        // Some entries aren't listed with their own inode though they're on the same device: a
        // bind mount within one filesystem, or any entry on a filesystem whose listing gives other
        // inodes than `lstat` does. So a part not found by its inode is looked for by its name.
        // The directory's entries were counted already, so they're counted again from where they
        // started, and the bound means what its sentence says.
        if found.is_none() && matches!(wanted, Wanted::Inode(_)) {
            let mut entries_again = entries_before;
            found = stored_name(directory, part, by_name, &mut entries_again)?;
            assert!(entries_again <= WALK_ENTRIES_MAX);
        }
        // It was there when it was looked at, so it's gone since.
        let Some(name) = found else {
            return Err(Unstored::Failed(String::from(MISSING)));
        };
        assert!(!name.is_empty());
        if !stored.is_empty() {
            stored.push(b'/');
        }
        stored.extend_from_slice(&name);
        directory_device = metadata.dev();
    }
    assert!(!stored.is_empty());
    assert!(entries_count <= WALK_ENTRIES_MAX);
    Ok(stored)
}

/// Wanted says how a part of a path is found among its directory's entries.
#[derive(Clone, Copy)]
enum Wanted {
    /// By its inode, which every name for the same file shares.
    Inode(u64),
    /// By its name, for an entry whose listed inode isn't its own, as a mount point's is of what
    /// it covers. A name that doesn't match it even in ASCII case is looked at itself, to see if
    /// it's this device and inode.
    Name {
        /// This is the device it's on, which for a mount point is the mounted one.
        device: u64,
        /// This is its own inode, which for a mount point is the mounted root's.
        inode: u64,
    },
}

/// Closeness says how closely an entry's name matches the part typed, from least to most.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Closeness {
    /// The name isn't ASCII, and it's for the same file, found by its inode alone. It may be the
    /// name that folds to the part typed beyond ASCII case, as the Kelvin sign folds to `k`, or
    /// another hard link. Either way it isn't kebab, so the path is refused, though of two such
    /// names it may show the other.
    Folded,
    /// The name differs from the part typed only in ASCII case.
    AsciiCase,
    /// The name is exactly the part typed.
    Exact,
}

/// Returns the name `part` is stored by in `directory`, found as `wanted` says, or nothing if no
/// entry is it. Of several names for the same file, as hard links make, the closest to `part` is
/// taken. An ASCII name that matches `part` in neither way is another link, never the name that
/// was typed, so it's never taken. Every entry is looked at, and counted in `entries_count`.
fn stored_name(
    directory: &OsStr,
    part: &str,
    wanted: Wanted,
    entries_count: &mut u32,
) -> Result<Option<Vec<u8>>, Unstored> {
    assert!(!part.is_empty());
    assert!(*entries_count <= WALK_ENTRIES_MAX);
    let unreadable = |error: io::Error| {
        let reason = reason(&error);
        Unstored::Failed(format!("a directory on the way to this file can't be read: {reason}."))
    };
    let mut found: Option<(Vec<u8>, Closeness)> = None;
    for entry in fs::read_dir(directory).map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        *entries_count += 1;
        if *entries_count > WALK_ENTRIES_MAX {
            let sentence = "the directories on the way to this file hold more than 10,000 entries.";
            return Err(Unstored::Failed(String::from(sentence)));
        }
        let name = entry.file_name();
        let name = name.as_bytes();
        let another_file = match wanted {
            Wanted::Inode(inode) => entry.ino() != inode,
            Wanted::Name { .. } => false,
        };
        if another_file {
            continue;
        }
        let closeness = if name == part.as_bytes() {
            Closeness::Exact
        } else if name.eq_ignore_ascii_case(part.as_bytes()) {
            Closeness::AsciiCase
        } else if name.is_ascii() {
            continue;
        } else if let Wanted::Name { device, inode } = wanted {
            // A mount point's listed inode can't say whether this is it, so the entry itself is
            // looked at. One that can't be looked at isn't it.
            let Ok(metadata) =
                fs::symlink_metadata(Path::new(directory).join(OsStr::from_bytes(name)))
            else {
                continue;
            };
            if metadata.dev() != device {
                continue;
            }
            if metadata.ino() != inode {
                continue;
            }
            Closeness::Folded
        } else {
            Closeness::Folded
        };
        if found.as_ref().is_none_or(|(_, best)| closeness > *best) {
            found = Some((name.to_vec(), closeness));
        }
    }
    let name = found.map(|(name, _)| name);
    assert!(name.as_ref().is_none_or(|name| !name.is_empty()));
    Ok(name)
}

/// Lints every program the walk finds, writing each message to `stderr` as it's made, with a
/// blank line between messages. Returns the worst status any of them gets.
fn lint_every(buffers: &mut Buffers, stderr: &mut impl Write) -> io::Result<u8> {
    let Ok(found) = walk() else {
        let sentence = "there are more than 10,000 files and directories under this directory.";
        writeln!(stderr, "stella: {sentence}")?;
        return Ok(FAILED);
    };
    if found.is_empty() {
        writeln!(stderr, "stella: there are no `.stella` files under this directory.")?;
        return Ok(FAILED);
    }
    let mut status = PASSED;
    let mut messages_count: u32 = 0;
    for find in &found {
        let (find_status, text) = match find {
            // The walk finds each name as it's stored, and it never follows a symlink.
            Found::Source(path) => lint_stored(path, path, buffers),
            Found::Unreadable { directory, error } => {
                let reason = reason(error);
                let sentence = format!("this directory can't be read: {reason}.");
                (FAILED, message(directory, b"", Place::File, sentence))
            }
        };
        assert_eq!(find_status == PASSED, text.is_empty());
        status = status.max(find_status);
        if !text.is_empty() {
            if messages_count > 0 {
                stderr.write_all(b"\n")?;
            }
            stderr.write_all(text.as_bytes())?;
            messages_count += 1;
        }
    }
    assert_eq!(status == PASSED, messages_count == 0);
    Ok(status)
}

/// Reads the file at `path` into `buffer`, as much of it as fits. Returns what was read, or the
/// sentence that says why it couldn't be.
fn read<'buffer>(
    path: &str,
    buffer: &'buffer mut [u8; BUFFER_BYTES],
) -> Result<&'buffer [u8], String> {
    // The file is checked before it's opened, since opening something that isn't a file, like a
    // FIFO, can wait forever. Only something swapped in between the two could still do that, or be
    // a symlink by the time it's opened. Closing that gap would take `unsafe` or a dependency, and
    // only someone who can already write where the program is could open it.
    let metadata = fs::metadata(path).map_err(|error| file_error(&error))?;
    if !metadata.is_file() {
        return Err(String::from("this isn't a file."));
    }
    let mut file = File::open(path)
        .map_err(|error| format!("this file can't be read: {}.", reason(&error)))?;
    let mut length = 0;
    // Every read either adds bytes, ends the file, or fails, so this stops by the time the
    // buffer's full. An interrupted read fails like any other.
    while length < BUFFER_BYTES {
        match file.read(&mut buffer[length..]) {
            Ok(0) => break,
            Ok(count) => length += count,
            Err(error) => return Err(format!("this file can't be read: {}.", reason(&error))),
        }
    }
    assert!(length <= BUFFER_BYTES);
    Ok(&buffer[..length])
}

/// This is the sentence for a file that isn't there, however it's found to be missing.
const MISSING: &str = "this file doesn't exist.";

/// Returns the sentence for `error`, which came from looking at a file: that it doesn't exist,
/// or that it can't be read, and why.
fn file_error(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => String::from(MISSING),
        _ => format!("this file can't be read: {}.", reason(error)),
    }
}

/// Returns why `error` happened, shown safely like anything else that reaches the terminal.
fn reason(error: &io::Error) -> String {
    let reason = shown(error.to_string().as_bytes());
    assert!(!reason.is_empty());
    reason
}
