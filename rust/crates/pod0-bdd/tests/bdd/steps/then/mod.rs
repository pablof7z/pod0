//! `Then` — the claims, and the one rule they all obey.
//!
//! # The empty-world rule
//!
//! A negative or since-then claim ("does not list", "has not advanced",
//! "no further deliveries") is worthless against a world that never produced
//! the thing it reads: it would pass whatever the core did. Every such step
//! therefore asserts that something EXISTED to observe before it asserts
//! anything about it, through [`nothing_to_observe`] — whose message names
//! WHAT WAS MISSING and is deliberately worded unlike a failed assertion,
//! so the two failure classes are distinguishable at a glance and
//! `NOTHING TO OBSERVE` greps for exactly the scenarios that proved nothing.
//!
//! # How the families below are split
//!
//! BY THE DOMAIN THE CLAIM IS ABOUT, not by the facade operation it happens
//! to read. `snapshot` serves both "the library lists this podcast" (a
//! library claim) and "this operation was cancelled" (an operation-lifecycle
//! claim), and it is the domain that decides where an assertion lives. Each
//! file owns the private decoding helpers its own family needs.
//!
//! - `library` — what the app-visible library shows, and what it must never
//!   show, whether read as a snapshot or as live deliveries.
//! - `operations` — one command's lifecycle: succeeded, cancelled, failed
//!   with a typed reason, and what the state revision did about it.
//! - `host_work` — the bounded work queue the native host drains: what work
//!   exists, what must no longer exist, and what was told to stop.
//!
//! The empty-world rule applies to all three families, so
//! [`nothing_to_observe`] is defined HERE, textually BEFORE the module
//! declarations: a `macro_rules!` is in scope for every module declared
//! after it, which lets one definition serve every family without exporting
//! anything. A new family goes below the macro for the same reason.

/// A step's precondition that the world produced the thing it reads (see
/// this module's doc). `$present` is the PRECONDITION — true when there is
/// something to observe — and the message names what was missing when there
/// is not.
macro_rules! nothing_to_observe {
    ($present:expr, $($missing:tt)+) => {
        assert!(
            $present,
            "NOTHING TO OBSERVE -- {} -- so this step reads an empty world and \
             would pass whatever the core did; a check that cannot fail is not \
             coverage, and the scenario's setup is what needs fixing",
            format_args!($($missing)+)
        )
    };
}

mod host_work;
mod library;
mod operations;
