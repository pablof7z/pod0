//! The closed step-vocabulary catalog. Scenario `.feature` files compose
//! from it; they never invent an ad-hoc step inline, and adding a step is a
//! reviewed change to one of these modules (in the same PR as the first
//! scenario that needs it). The runner enforces the closure: an undefined
//! step fails the suite instead of skipping (`fail_on_skipped` in
//! `main.rs`).
//!
//! `given` and `when` are one file each; `then` is a directory, split by the
//! domain each assertion family addresses (see its own module doc).

pub mod given;
pub mod then;
pub mod when;
