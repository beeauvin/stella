// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A function with thirty branches, more than anyone can hold in mind.

/// Returns `count` when it is from 1 to 30, and none otherwise.
#[must_use]
pub const fn kept(count: u32) -> u32 {
    let mut total = 0_u32;
    if count == 1 { total = total.wrapping_add(1); }
    if count == 2 { total = total.wrapping_add(2); }
    if count == 3 { total = total.wrapping_add(3); }
    if count == 4 { total = total.wrapping_add(4); }
    if count == 5 { total = total.wrapping_add(5); }
    if count == 6 { total = total.wrapping_add(6); }
    if count == 7 { total = total.wrapping_add(7); }
    if count == 8 { total = total.wrapping_add(8); }
    if count == 9 { total = total.wrapping_add(9); }
    if count == 10 { total = total.wrapping_add(10); }
    if count == 11 { total = total.wrapping_add(11); }
    if count == 12 { total = total.wrapping_add(12); }
    if count == 13 { total = total.wrapping_add(13); }
    if count == 14 { total = total.wrapping_add(14); }
    if count == 15 { total = total.wrapping_add(15); }
    if count == 16 { total = total.wrapping_add(16); }
    if count == 17 { total = total.wrapping_add(17); }
    if count == 18 { total = total.wrapping_add(18); }
    if count == 19 { total = total.wrapping_add(19); }
    if count == 20 { total = total.wrapping_add(20); }
    if count == 21 { total = total.wrapping_add(21); }
    if count == 22 { total = total.wrapping_add(22); }
    if count == 23 { total = total.wrapping_add(23); }
    if count == 24 { total = total.wrapping_add(24); }
    if count == 25 { total = total.wrapping_add(25); }
    if count == 26 { total = total.wrapping_add(26); }
    if count == 27 { total = total.wrapping_add(27); }
    if count == 28 { total = total.wrapping_add(28); }
    if count == 29 { total = total.wrapping_add(29); }
    if count == 30 { total = total.wrapping_add(30); }
    total
}
