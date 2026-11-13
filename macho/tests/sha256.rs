// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for SHA-256. Nothing here works a hash out a second way: what a hash should be comes
//! from macOS's own `shasum`.
//!
//! `PINNED` holds messages with the hash `shasum -a 256` gave each on macOS 27.2: FIPS 180-4's
//! examples, a message at every length where the padding changes, a page of zeros, a page that
//! isn't, and two of about a megabyte. Each row's comment is the command that gave its hash, so
//! anyone can run it again. `shasum_agrees` runs it again every time: it writes every pinned
//! message, every length up to 300 bytes and seeded random messages to files, hashes them all
//! with `/usr/bin/shasum` in one run, and compares each with `sha256`, and each pin with what
//! `shasum` says now. So a pin can't go stale without a test saying so, and a machine without
//! `shasum` fails rather than passing unchecked.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::process::{Command, Stdio};

use stella_macho::{HASH_BYTES, sha256};
use stella_test_support::{Rng, Scratch, write};

/// A message a pinned hash is of, as what it's made of, so the largest don't have to be written
/// out.
#[derive(Clone, Copy, Debug)]
enum Message {
    /// This text's bytes.
    Text(&'static str),
    /// This many bytes of 0.
    Zeros(usize),
    /// This many bytes of the letter `a`.
    LetterA(usize),
    /// This many bytes of the pattern: the byte at each index is the index modulo 251, so every
    /// block of a long message differs from the one before.
    Pattern(usize),
}

impl Message {
    /// Returns the message's bytes.
    fn bytes(self) -> Vec<u8> {
        match self {
            Self::Text(text) => text.as_bytes().to_vec(),
            Self::Zeros(size_bytes) => vec![0; size_bytes],
            Self::LetterA(size_bytes) => vec![b'a'; size_bytes],
            Self::Pattern(size_bytes) => (0..size_bytes).map(pattern_byte).collect(),
        }
    }
}

/// These are the lengths where the padding changes, with the lengths either side of each. A
/// message's last block has room for its length after 55 bytes at most, a block is 64 bytes, and
/// the same goes for a second block.
const PADDING_EDGES: [usize; 13] = [0, 1, 54, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128];

/// These are the pinned messages, each with its hash as `shasum -a 256` printed it on macOS 27.2
/// on 2026-11-12. The command above each row is the one that was run. `pattern N` stands for
/// `perl -e 'print pack("C*", map { $_ % 251 } 0..$ARGV[0]-1)' N`.
const PINNED: [(Message, &str); 20] = [
    // printf '' | shasum -a 256
    (Message::Text(""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
    // printf 'abc' | shasum -a 256
    (Message::Text("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
    // printf 'abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq' | shasum -a 256
    (
        Message::Text("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    ),
    // printf '<the 112 characters below>' | shasum -a 256
    (
        Message::Text(concat!(
            "abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmn",
            "hijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
        )),
        "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
    ),
    // pattern 1 | shasum -a 256, and so on for each length.
    (Message::Pattern(1), "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d"),
    (Message::Pattern(54), "675f28acc0b90a72d1c3a570fe83ac565555db358cf01826dc8eefb2bf7ca0f3"),
    (Message::Pattern(55), "463eb28e72f82e0a96c0a4cc53690c571281131f672aa229e0d45ae59b598b59"),
    (Message::Pattern(56), "da2ae4d6b36748f2a318f23e7ab1dfdf45acdc9d049bd80e59de82a60895f562"),
    (Message::Pattern(57), "2fe741af801cc238602ac0ec6a7b0c3a8a87c7fc7d7f02a3fe03d1c12eac4d8f"),
    (Message::Pattern(63), "29af2686fd53374a36b0846694cc342177e428d1647515f078784d69cdb9e488"),
    (Message::Pattern(64), "fdeab9acf3710362bd2658cdc9a29e8f9c757fcf9811603a8c447cd1d9151108"),
    (Message::Pattern(65), "4bfd2c8b6f1eec7a2afeb48b934ee4b2694182027e6d0fc075074f2fabb31781"),
    (Message::Pattern(119), "da18797ed7c3a777f0847f429724a2d8cd5138e6ed2895c3fa1a6d39d18f7ec6"),
    (Message::Pattern(120), "f52b23db1fbb6ded89ef42a23ce0c8922c45f25c50b568a93bf1c075420bbb7c"),
    (Message::Pattern(127), "92ca0fa6651ee2f97b884b7246a562fa71250fedefe5ebf270d31c546bfea976"),
    (Message::Pattern(128), "471fb943aa23c511f6f72f8d1652d9c880cfa392ad80503120547703e56a2be5"),
    // head -c 4096 /dev/zero | shasum -a 256
    // This is the hash the signature holds for every page of a binary that's all zeros.
    (Message::Zeros(4_096), "ad7facb2586fc6e966c004d7d1d16b024f5805ff7cb47c7a85dabd8b48892ca7"),
    // pattern 4096 | shasum -a 256
    (Message::Pattern(4_096), "d67c656e01756650d77717b0839985a056ec28ffe174601d690fc407a2ceffca"),
    // head -c 1000000 /dev/zero | tr '\0' 'a' | shasum -a 256
    // This is FIPS 180-4's long example, a million of the letter `a`.
    (
        Message::LetterA(1_000_000),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
    ),
    // pattern 1048577 | shasum -a 256
    // A byte more than a mebibyte, so the hash goes through 16,384 whole blocks and one byte.
    (
        Message::Pattern(1_048_577),
        "5769f52bc3eef28afa39c6fc68cadb7d0bd69812ae3a3d71452f519ec3c7aa56",
    ),
];

/// Returns the byte at `index` of the pattern: the index modulo 251, the largest prime a byte
/// holds, so the pattern never lines up with a block.
fn pattern_byte(index: usize) -> u8 {
    u8::try_from(index % 251).expect("a number below 251 fits in a byte")
}

/// Returns `hash` as lowercase hex, as `shasum` writes it.
fn hex(hash: [u8; HASH_BYTES]) -> String {
    let mut text = String::with_capacity(2 * HASH_BYTES);
    for byte in hash {
        write!(text, "{byte:02x}").expect("writing to a string can't fail");
    }
    text
}

/// Returns `size_bytes` random bytes from `rng`.
fn random_bytes(rng: &mut Rng, size_bytes: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(size_bytes);
    for _ in 0..size_bytes {
        bytes.push(u8::try_from(rng.below(256)).expect("a number below 256 fits in a byte"));
    }
    bytes
}

/// Hashes each of `messages` with `/usr/bin/shasum -a 256`, all in one run, from files in a new
/// scratch directory named `name`, and returns the hashes in order, as hex.
fn shasum(name: &str, messages: &[Vec<u8>]) -> Vec<String> {
    let directory = Scratch::of("macho-sha256").fresh(name);
    let mut file_names = Vec::with_capacity(messages.len());
    for (index, message) in messages.iter().enumerate() {
        let file_name = format!("{index:04}");
        write(&directory, &file_name, message);
        file_names.push(file_name);
    }
    let output = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .args(&file_names)
        .current_dir(&directory)
        .stdin(Stdio::null())
        .output()
        .expect("`/usr/bin/shasum` is there to run on macOS");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "shasum hashes files it can read, but said: {said}");
    let printed = String::from_utf8(output.stdout).expect("shasum prints ASCII");
    // Each line is a file's hash, then its name.
    let mut hashes = Vec::with_capacity(messages.len());
    for (line, file_name) in printed.lines().zip(&file_names) {
        let mut words = line.split_whitespace();
        let hash = words.next().expect("a line starts with a hash");
        assert_eq!(words.next(), Some(file_name.as_str()), "the hashes are in the files' order");
        assert_eq!(words.next(), None, "a line is a hash and a name: {line:?}");
        let is_hex =
            hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        assert!(is_hex, "a hash is lowercase hex: {hash:?}");
        assert_eq!(hash.len(), 2 * HASH_BYTES, "a hash is 32 bytes, so 64 digits: {hash:?}");
        hashes.push(String::from(hash));
    }
    assert_eq!(hashes.len(), messages.len(), "shasum prints one line for each file");
    hashes
}

#[test]
fn pinned_hashes() {
    // Each pinned message hashes to what `shasum` gave when it was pinned. The pins hold every
    // length where the padding changes, so none can be dropped without this saying so.
    assert_eq!(HASH_BYTES, 32);
    for (message, pinned) in PINNED {
        assert_eq!(hex(sha256(&message.bytes())), pinned, "for {message:?}");
    }
    for edge in PADDING_EDGES {
        let is_pinned = PINNED.iter().any(|(message, _)| message.bytes().len() == edge);
        assert!(is_pinned, "no pinned message is {edge} bytes");
    }
}

#[test]
fn shasum_agrees() {
    // Every pinned message, a random message of every length from 0 to 300 bytes, which crosses
    // each place the padding changes twice over, and 200 random messages of up to 70 KiB are
    // written to files and hashed by `shasum` in one run. `sha256` gives the same hash for each,
    // and each pin is what `shasum` gives today.
    let mut rng = Rng::new(0x5348_4132_3536_0001);
    let mut messages: Vec<Vec<u8>> = PINNED.iter().map(|(message, _)| message.bytes()).collect();
    for size_bytes in 0..=300 {
        messages.push(random_bytes(&mut rng, size_bytes));
    }
    for _ in 0..200_u32 {
        let size_bytes = rng.below(70 * 1024);
        messages.push(random_bytes(&mut rng, size_bytes));
    }
    for edge in PADDING_EDGES {
        let tried_count = messages.iter().filter(|message| message.len() == edge).count();
        assert!(tried_count >= 2, "{edge} bytes is tried {tried_count} times");
    }
    let hashes = shasum("agrees", &messages);
    assert_eq!(hashes.len(), messages.len());
    for (message, hash) in messages.iter().zip(&hashes) {
        assert_eq!(&hex(sha256(message)), hash, "a message of {} bytes", message.len());
    }
    for ((message, pinned), hash) in PINNED.iter().zip(&hashes) {
        assert_eq!(hash, pinned, "what shasum gives today for {message:?}");
    }
}

#[test]
fn every_bit_counts() {
    // The same bytes always hash the same. A message of 150 bytes takes two whole blocks and
    // part of a third, and flipping any one of its 1,200 bits changes the hash.
    let message = Message::Pattern(150).bytes();
    let hash = sha256(&message);
    assert_eq!(sha256(&message), hash);
    for index in 0..message.len() {
        for bit in 0..8_u32 {
            let mut changed = message.clone();
            changed[index] ^= 1 << bit;
            assert_ne!(sha256(&changed), hash, "bit {bit} of byte {index}");
        }
    }
}

#[test]
fn the_length_counts() {
    // Messages of nothing but zeros differ only in how long they are, and the padding puts a
    // message's length in its last block. So zeros of every length from 0 to 300 bytes hash to
    // 301 different hashes.
    let mut hashes: Vec<[u8; HASH_BYTES]> =
        (0..=300).map(|size_bytes| sha256(&Message::Zeros(size_bytes).bytes())).collect();
    hashes.sort_unstable();
    hashes.dedup();
    assert_eq!(hashes.len(), 301);
}
