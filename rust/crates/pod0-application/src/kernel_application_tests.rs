use super::*;

#[derive(Clone, Copy)]
struct FixedClock(UnixTimestampMilliseconds);

impl Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        self.0
    }
}

#[test]
fn identical_command_and_time_produce_identical_projection() {
    let time = UnixTimestampMilliseconds::new(1_700_000_000_123);
    let command = KernelProbeCommand {
        command_id: CommandId::from_bytes([9; 16]),
    };
    let first = KernelApplication::new(FixedClock(time)).dispatch_probe(command);
    let second = KernelApplication::new(FixedClock(time)).dispatch_probe(command);
    assert_eq!(first, second);
    assert_eq!(first.observed_at, time);
    assert_eq!(first.core_schema_version, CORE_SCHEMA_VERSION);
}
