#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserDataErasureExclusion {
    CredentialsKeychain,
    UserPreferences,
    NetworkCache,
}

impl UserDataErasureExclusion {
    pub const ALL: [Self; 3] = [
        Self::CredentialsKeychain,
        Self::UserPreferences,
        Self::NetworkCache,
    ];

    pub const fn rationale(self) -> &'static str {
        match self {
            Self::CredentialsKeychain => "authentication secret, not product-generated data",
            Self::UserPreferences => "retained non-product settings; product projection is erased",
            Self::NetworkCache => "operating-system cache with no authoritative product state",
        }
    }
}
