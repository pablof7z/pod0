use pod0_application::Clock;
use pod0_domain::UnixTimestampMilliseconds;

pub(super) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        let duration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        UnixTimestampMilliseconds::new(i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
    }
}
