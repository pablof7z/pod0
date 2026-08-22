use std::fmt;

use zeroize::Zeroize as _;

pub struct ProviderSecret(String);

impl ProviderSecret {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProviderSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProviderSecret([REDACTED])")
    }
}

impl Drop for ProviderSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let secret = ProviderSecret::new("provider-secret");
        let debug = format!("{secret:?}");
        assert_eq!(debug, "ProviderSecret([REDACTED])");
        assert!(!debug.contains("provider-secret"));
    }
}
