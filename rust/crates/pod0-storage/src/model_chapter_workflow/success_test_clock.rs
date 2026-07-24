#[derive(Clone, Copy)]
struct FixedClock;

impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_800_000_200_000
    }
}
