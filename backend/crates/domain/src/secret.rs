//! A credential that cannot leak through `Debug`, `Display` or logs.

use std::fmt;

/// Holds a password-like value or an API key. The only way to read it is
/// [`Secret::expose`], which makes every use greppable.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Trims surrounding whitespace (secret files end with a newline). Returns
    /// `None` for an empty value so "unset" and "blank" behave the same.
    #[must_use]
    pub fn new(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        (!raw.is_empty()).then(|| Self(raw.to_owned()))
    }

    /// Keeps the value exactly as given (passwords may start or end with a
    /// space). `None` only for the empty string.
    #[must_use]
    pub fn verbatim(raw: &str) -> Option<Self> {
        (!raw.is_empty()).then(|| Self(raw.to_owned()))
    }

    /// For values the program made itself, which cannot be empty.
    #[must_use]
    pub fn generated(raw: &str) -> Self {
        Self(raw.to_owned())
    }

    /// The only way out; call it where the value is put on the wire.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_prints_the_value() {
        let s = Secret::new(" sk-ant-123\n").unwrap();
        assert_eq!(s.expose(), "sk-ant-123");
        assert_eq!(format!("{s:?} {s}"), "[redacted] [redacted]");
        assert!(Secret::new("  \n").is_none());
    }

    #[test]
    fn verbatim_keeps_surrounding_spaces() {
        assert_eq!(Secret::verbatim(" a ").unwrap().expose(), " a ");
        assert!(Secret::verbatim("").is_none());
    }
}
