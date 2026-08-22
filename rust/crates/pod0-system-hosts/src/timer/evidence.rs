use std::time::{Duration, Instant, SystemTime};

use super::{
    CAPABILITY, ClockSnapshot, WakeCancelledEvidence, WakeDeadline, WakeFiredEvidence, WakeOutcome,
    WakePlan,
};
use crate::{HostResult, Operation, SystemHostError};

pub(super) fn make_plan(requested: WakeDeadline) -> HostResult<WakePlan> {
    let scheduled_at = ClockSnapshot {
        wall: SystemTime::now(),
        monotonic: Instant::now(),
    };
    let (delay, target_wall) = match requested {
        WakeDeadline::After(delay) => {
            let target_wall = scheduled_at.wall.checked_add(delay).ok_or_else(|| {
                SystemHostError::invalid(
                    CAPABILITY,
                    Operation::ScheduleWake,
                    "wall-clock deadline overflowed",
                )
            })?;
            (delay, target_wall)
        }
        WakeDeadline::At(target) => (
            target
                .duration_since(scheduled_at.wall)
                .unwrap_or(Duration::ZERO),
            target,
        ),
    };
    let target_monotonic = scheduled_at.monotonic.checked_add(delay).ok_or_else(|| {
        SystemHostError::invalid(
            CAPABILITY,
            Operation::ScheduleWake,
            "monotonic deadline overflowed",
        )
    })?;
    Ok(WakePlan {
        requested,
        scheduled_at,
        target_wall,
        target_monotonic,
    })
}

pub(super) fn fired_evidence(plan: WakePlan) -> WakeOutcome {
    let observed_at = ClockSnapshot {
        wall: SystemTime::now(),
        monotonic: Instant::now(),
    };
    WakeOutcome::Fired(WakeFiredEvidence {
        plan,
        observed_at,
        elapsed_monotonic: observed_at
            .monotonic
            .saturating_duration_since(plan.scheduled_at.monotonic),
        overdue_by: match plan.requested {
            WakeDeadline::After(_) => observed_at
                .monotonic
                .saturating_duration_since(plan.target_monotonic),
            WakeDeadline::At(target) => observed_at
                .wall
                .duration_since(target)
                .unwrap_or(Duration::ZERO),
        },
    })
}

pub(super) fn cancelled_evidence(plan: WakePlan) -> WakeOutcome {
    let observed_at = ClockSnapshot {
        wall: SystemTime::now(),
        monotonic: Instant::now(),
    };
    WakeOutcome::Cancelled(WakeCancelledEvidence {
        plan,
        observed_at,
        elapsed_monotonic: observed_at
            .monotonic
            .saturating_duration_since(plan.scheduled_at.monotonic),
    })
}
