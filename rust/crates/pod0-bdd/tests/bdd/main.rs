//! The `harness = false` cucumber entry point: parses every `.feature` file
//! under the repo-root `features/` directory and runs the closed step
//! catalog (`steps::{given,when,then}`) against [`world::PodWorld`] — a REAL
//! `Pod0Facade` opened over a freshly prepared authoritative store, never a
//! mocked core and never the application layer underneath the facade.
//!
//! Tag filtering happens HERE, in Rust, not via CLI flags:
//!
//! - **`@wip`** is ALWAYS excluded: it means "this is built and BROKEN, and
//!   here is the report" — a genuine, reported gap must never masquerade as
//!   a passing proof.
//! - **`@designed`** is ALWAYS excluded and means something different:
//!   "this is NOT BUILT YET, and this scenario is the agreed acceptance
//!   criterion for building it". A `@designed` scenario carries no step
//!   definitions by construction, which is precisely why it must never
//!   reach the runner; removing the tag is the definition of done for the
//!   work it describes. (pod0 currently ships zero `@designed` scenarios —
//!   every scenario in `features/` executes — but the gate exists so the
//!   first one added behaves correctly from day one.)
//!
//! `fail_on_skipped` closes the catalog from the runner's side: a scenario
//! step with no definition fails the suite instead of silently skipping.
//!
//! Scenarios run one at a time. They are isolated (each owns a temp store),
//! but serial execution keeps suite output deterministic and failure
//! interleaving readable, and this suite is nowhere near needing the speed.

use std::path::PathBuf;

use cucumber::World as _;

mod steps;
mod world;

fn main() {
    let features = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../features"));
    futures::executor::block_on(
        world::PodWorld::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .filter_run_and_exit(features, |_feature, _rule, scenario| {
                !scenario
                    .tags
                    .iter()
                    .any(|tag| tag == "wip" || tag == "designed")
            }),
    );
}
