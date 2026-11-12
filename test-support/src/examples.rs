// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Examples are the sources a test tries, each with what's expected of it.
//!
//! A test adds its examples and then takes them all at once to check them. That lets it try an
//! oracle on every example before the code under test sees any, so every example checks the
//! oracle even while the code can't run. Examples dropped without being taken fail the test, so
//! a test can't pass by forgetting to check them.

use std::panic::Location;
use std::thread;

/// An example is a source, what's expected of it, and where the test added it.
#[derive(Debug)]
pub struct Example<Expected> {
    /// The source the test tries.
    pub source: String,
    /// What the test expects of it.
    pub expected: Expected,
    /// The line of the test that added it, so a failure points at the example.
    pub at: &'static Location<'static>,
}

/// This is the most examples one test can hold. It's far more than any test tries, so a loop
/// that adds without end fails here, with a reason, instead of filling memory.
pub const EXAMPLES_MAX: usize = 100_000;

/// The examples a test tries.
#[derive(Debug)]
pub struct Examples<Expected> {
    examples: Vec<Example<Expected>>,
}

impl<Expected> Examples<Expected> {
    /// Returns no examples yet.
    #[must_use]
    pub const fn new() -> Self {
        Self { examples: Vec::new() }
    }

    /// Adds `source`, with what's `expected` of it.
    ///
    /// # Panics
    ///
    /// Panics if the test already holds `EXAMPLES_MAX` examples.
    #[track_caller]
    pub fn add(&mut self, source: &str, expected: Expected) {
        assert!(self.examples.len() < EXAMPLES_MAX, "a test tries at most {EXAMPLES_MAX} examples");
        self.examples.push(Example {
            source: String::from(source),
            expected,
            at: Location::caller(),
        });
    }

    /// Returns every example, in the order they were added, for the test to check.
    ///
    /// # Panics
    ///
    /// Panics if there are none, since a test that tries nothing proves nothing.
    #[must_use]
    pub fn take(mut self) -> Vec<Example<Expected>> {
        assert!(!self.examples.is_empty(), "a test tries at least one example");
        std::mem::take(&mut self.examples)
    }
}

impl<Expected> Default for Examples<Expected> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Expected> Drop for Examples<Expected> {
    /// Fails the test if any example was added but never taken. A test that has already failed is
    /// left to report its own failure.
    fn drop(&mut self) {
        if thread::panicking() {
            return;
        }
        assert!(self.examples.is_empty(), "examples were added but never checked");
    }
}
