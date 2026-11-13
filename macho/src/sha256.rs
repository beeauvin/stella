// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! SHA-256, as FIPS 180-4 defines it.
//!
//! The code signature hashes each page of the binary with it, and the binary's UUID is taken
//! from it. It reads the bytes once, a 64-byte block at a time. All it keeps is on the stack:
//! its eight words of state, the last one or two blocks with their padding, and the 64-word
//! schedule of the block it's mixing in.

/// This is how many bytes a hash takes.
pub const HASH_BYTES: usize = 32;

/// This is how many bytes a block takes.
const BLOCK_BYTES: usize = 64;

/// This is how many bytes the message's length takes, at the end of the last block.
const LENGTH_BYTES: usize = 8;

/// This is how many bytes a word of the state takes.
const WORD_BYTES: usize = 4;

/// This is how many words the state holds.
const STATE_WORDS: usize = 8;

/// This is how many rounds mix one block in, and how many words a block's schedule holds.
const ROUNDS_COUNT: usize = 64;

/// A message's length is counted in bits, in 64 bits, so it's less than this many bytes.
const MESSAGE_BYTES_END: u64 = 1 << 61;

/// These are the first eight words of state: the first 32 bits of the fractional parts of the
/// square roots of the first eight primes.
const INITIAL_STATE: [u32; STATE_WORDS] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// These are the round constants: the first 32 bits of the fractional parts of the cube roots
/// of the first 64 primes.
const ROUND_CONSTANTS: [u32; ROUNDS_COUNT] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

const _: () = assert!(
    HASH_BYTES == STATE_WORDS * WORD_BYTES,
    "the hash is the state's eight words, four bytes each"
);
const _: () = assert!(
    BLOCK_BYTES.is_multiple_of(WORD_BYTES),
    "a block is whole words, the first 16 of its schedule"
);

/// Returns the SHA-256 hash of `bytes`.
///
/// # Panics
///
/// Panics if `bytes` is 2^61 bytes or more, whose length in bits no longer fits in the 64 bits
/// the hash gives it. Nothing the compiler hashes is anywhere near that.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; HASH_BYTES] {
    let message_bytes = u64::try_from(bytes.len()).expect("a length in memory fits in 64 bits");
    assert!(message_bytes < MESSAGE_BYTES_END, "a message is less than 2^61 bytes");
    let mut state = INITIAL_STATE;
    let (blocks, rest) = bytes.as_chunks::<BLOCK_BYTES>();
    for block in blocks {
        sha256_mix_block(&mut state, block);
    }
    // What follows the whole blocks is padded out to one block more, or to two when the length
    // no longer fits after it.
    let (padding, padding_blocks_count) = sha256_padding(rest, message_bytes);
    let (padding_blocks, after) = padding.as_chunks::<BLOCK_BYTES>();
    assert!(after.is_empty(), "the padding is two whole blocks of room");
    for block in &padding_blocks[..padding_blocks_count] {
        sha256_mix_block(&mut state, block);
    }
    let mut hash = [0_u8; HASH_BYTES];
    let (hash_words, after) = hash.as_chunks_mut::<WORD_BYTES>();
    assert!(after.is_empty(), "the hash is whole words");
    for (word, hash_word) in state.iter().zip(hash_words) {
        *hash_word = word.to_be_bytes();
    }
    hash
}

/// Returns what follows a message's whole blocks, padded as the hash wants it: `rest`, then a 1
/// bit, then zeros, then the length of the whole message, `message_bytes`, in bits. It's one
/// block, or two when the length no longer fits after `rest`, and how many is returned with it.
fn sha256_padding(rest: &[u8], message_bytes: u64) -> ([u8; 2 * BLOCK_BYTES], usize) {
    assert!(rest.len() < BLOCK_BYTES, "what's left after the whole blocks is less than a block");
    assert!(message_bytes < MESSAGE_BYTES_END, "the length in bits fits in 64 bits");
    let mut padding = [0_u8; 2 * BLOCK_BYTES];
    padding[..rest.len()].copy_from_slice(rest);
    // A 1 bit, then seven of the zeros.
    padding[rest.len()] = 0x80;
    // The 1 bit takes a byte, and the length takes its eight, so a block has room for both
    // after 55 bytes of message at most.
    let blocks_count = if rest.len() < BLOCK_BYTES - LENGTH_BYTES { 1 } else { 2 };
    let end = blocks_count * BLOCK_BYTES;
    let message_bits = message_bytes * 8;
    padding[end - LENGTH_BYTES..end].copy_from_slice(&message_bits.to_be_bytes());
    (padding, blocks_count)
}

/// Mixes one block into `state`, in 64 rounds.
#[expect(
    clippy::many_single_char_names,
    reason = "KS-NO-ABBREVIATIONS: `a` to `h` are the eight working variables, by the names \
              FIPS 180-4 gives them, so the rounds read against the standard line for line"
)]
fn sha256_mix_block(state: &mut [u32; STATE_WORDS], block: &[u8; BLOCK_BYTES]) {
    let schedule = sha256_schedule(block);
    let mut working = *state;
    for (round_constant, word) in ROUND_CONSTANTS.iter().zip(schedule) {
        let [a, b, c, d, e, f, g, h] = working;
        let big_sigma_1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choice = (e & f) ^ (!e & g);
        let first = h
            .wrapping_add(big_sigma_1)
            .wrapping_add(choice)
            .wrapping_add(*round_constant)
            .wrapping_add(word);
        let big_sigma_0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let second = big_sigma_0.wrapping_add(majority);
        working = [first.wrapping_add(second), a, b, c, d.wrapping_add(first), e, f, g];
    }
    for (word, mixed) in state.iter_mut().zip(working) {
        *word = word.wrapping_add(mixed);
    }
}

/// Returns the schedule of `block`: its 16 words, big-endian, and then 48 more, each mixed from
/// four of the words before it.
fn sha256_schedule(block: &[u8; BLOCK_BYTES]) -> [u32; ROUNDS_COUNT] {
    let mut schedule = [0_u32; ROUNDS_COUNT];
    let (block_words, after) = block.as_chunks::<WORD_BYTES>();
    assert!(after.is_empty(), "a block is whole words");
    for (word, block_word) in schedule.iter_mut().zip(block_words) {
        *word = u32::from_be_bytes(*block_word);
    }
    for index in block_words.len()..ROUNDS_COUNT {
        let (early, late) = (schedule[index - 15], schedule[index - 2]);
        let small_sigma_0 = early.rotate_right(7) ^ early.rotate_right(18) ^ (early >> 3_u32);
        let small_sigma_1 = late.rotate_right(17) ^ late.rotate_right(19) ^ (late >> 10_u32);
        schedule[index] = schedule[index - 16]
            .wrapping_add(small_sigma_0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(small_sigma_1);
    }
    schedule
}
