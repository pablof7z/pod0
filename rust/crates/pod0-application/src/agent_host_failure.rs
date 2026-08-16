#[must_use]
pub const fn agent_host_failure_outcome(code: crate::HostFailureCode) -> crate::EffectOutcome {
    let code = match code {
        crate::HostFailureCode::Offline => crate::ActivityFailureCode::Offline,
        crate::HostFailureCode::TimedOut => crate::ActivityFailureCode::TimedOut,
        crate::HostFailureCode::PermissionDenied => crate::ActivityFailureCode::PermissionDenied,
        crate::HostFailureCode::InvalidResponse => crate::ActivityFailureCode::InvalidResponse,
        crate::HostFailureCode::ResponseTooLarge => crate::ActivityFailureCode::ResponseTooLarge,
        crate::HostFailureCode::MediaUnavailable => crate::ActivityFailureCode::MediaUnavailable,
        crate::HostFailureCode::ProviderUnavailable | crate::HostFailureCode::IndexUnavailable => {
            crate::ActivityFailureCode::ProviderUnavailable
        }
        crate::HostFailureCode::Unauthorized => crate::ActivityFailureCode::Unauthorized,
        crate::HostFailureCode::PlatformFailure => crate::ActivityFailureCode::PlatformFailure,
        crate::HostFailureCode::Unsupported { wire_code } => {
            crate::ActivityFailureCode::Unsupported { wire_code }
        }
    };
    crate::EffectOutcome::Failed { code }
}
