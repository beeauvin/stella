// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Writes a program's code as a signed Mach-O executable for macOS, the last step of a build.
//!
//! `write` takes the `Code` that only `generate` makes, and a `BinaryName`, so it has nothing to
//! refuse. It lays the file out with `layout`, asks the code for its bytes as they are where the
//! layout put it, and signs the file, hashing it with our own SHA-256. It returns an
//! `Executable`, which only it makes: the bytes the command writes to `.build/`.
//!
//! - `layout` works out where each segment, the code and the stack's top go.
//! - `BinaryName` is the name the signature identifies the binary by, checked where it arrives.
//! - `sha256` hashes each page for the signature, and gives the binary its UUID.

#![forbid(unsafe_code)]

pub use layout::{
    BASE_ADDRESS, CODE_OFFSET, HASH_PAGE_BYTES, IMAGE_BYTES_MAX, Layout, PAGE_BYTES,
    SIGNATURE_BYTES_MAX, STACK_BYTES_MAX, Sizes, TEXT_BYTES_MAX, layout,
};
pub use name::{BinaryName, BinaryNameProblem, NAME_BYTES_MAX};
pub use sha256::{HASH_BYTES, sha256};
pub use write::{Executable, write};

mod commands;
mod image;
mod layout;
mod name;
mod sha256;
mod signature;
mod write;
