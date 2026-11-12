// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for statuses: what each exits with, and which of two is the worse.

use stella_cli::Status;

#[test]
fn each_status_has_its_exit_code() {
    // The codes are the ones the compiler page gives, and scripts depend on.
    assert_eq!(Status::Passed.code(), 0);
    assert_eq!(Status::Refused.code(), 1);
    assert_eq!(Status::Failed.code(), 2);
}

#[test]
fn the_worse_of_two_statuses_is_the_greater() {
    // Linting every program takes the greatest status any of them gets, so the order must be
    // from best to worst, whichever comes first.
    let in_order = [Status::Passed, Status::Refused, Status::Failed];
    for (index, &status) in in_order.iter().enumerate() {
        for (other_index, &other) in in_order.iter().enumerate() {
            let worse = in_order[index.max(other_index)];
            assert_eq!(status.max(other), worse, "for {status:?} and {other:?}");
        }
    }
}
