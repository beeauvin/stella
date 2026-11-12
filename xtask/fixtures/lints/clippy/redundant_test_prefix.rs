// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A test whose name starts with `test_`.

/// The most of anything.
pub const LIMIT: u32 = 8;

#[cfg(test)]
mod tests {
    use super::LIMIT;

    #[test]
    fn test_limit() {
        assert_eq!(LIMIT, 8);
    }
}
