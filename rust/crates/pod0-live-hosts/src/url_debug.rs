use std::fmt;

pub(crate) struct RedactedUrl<'a>(&'a str);

impl<'a> RedactedUrl<'a> {
    pub(crate) const fn new(value: &'a str) -> Self {
        Self(value)
    }
}

impl fmt::Debug for RedactedUrl<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        redacted_url(self.0).fmt(formatter)
    }
}

fn redacted_url(value: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(value) else {
        return "[INVALID OR RELATIVE URL REDACTED]".to_owned();
    };
    if !url.username().is_empty() {
        let _ = url.set_username("REDACTED");
    }
    if url.password().is_some() {
        let _ = url.set_password(Some("REDACTED"));
    }
    let pairs = url
        .query_pairs()
        .map(|(key, value)| {
            let value = if is_sensitive_query_key(&key) {
                "REDACTED".to_owned()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect::<Vec<_>>();
    if url.query().is_some() {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.to_string()
}

fn is_sensitive_query_key(key: &str) -> bool {
    let normalized = key
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let key = String::from_utf8(normalized).expect("ASCII normalization is valid UTF-8");
    matches!(
        key.as_str(),
        "accesskey"
            | "apikey"
            | "authorization"
            | "auth"
            | "awsaccesskeyid"
            | "bearer"
            | "code"
            | "jwt"
            | "key"
            | "password"
            | "passwd"
            | "privatekey"
            | "pwd"
            | "secret"
            | "secretkey"
            | "session"
            | "sessionid"
            | "sig"
            | "signature"
            | "signingkey"
            | "token"
    ) || key.ends_with("credential")
        || key.ends_with("key")
        || key.ends_with("password")
        || key.ends_with("secret")
        || key.ends_with("signature")
        || key.ends_with("token")
}
