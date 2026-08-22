use std::fmt;

use zeroize::Zeroize;

pub struct Secret(Vec<u8>);

impl Secret {
    #[must_use]
    pub fn new(value: impl Into<Vec<u8>>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn expose_secret(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Secret([REDACTED])")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn debug_and_display_are_redacted() {
        let secret = Secret::new(b"never-print-this".to_vec());
        let debug = format!("{secret:?}");
        let display = format!("{secret}");

        assert_eq!(debug, "Secret([REDACTED])");
        assert_eq!(display, "[REDACTED]");
        assert!(!debug.contains("never-print-this"));
        assert!(!display.contains("never-print-this"));
    }
}
