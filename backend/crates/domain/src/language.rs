use std::fmt;
use std::str::FromStr;

/// Language of everything the agents write for a topic (ADR 0019). Chosen when
/// the topic is created and never changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ContentLanguage {
    #[default]
    It,
    En,
}

impl ContentLanguage {
    /// ISO 639-1 code used by the API and the database.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::It => "it",
            Self::En => "en",
        }
    }

    /// Name written into prompts, which are in Italian (`{{content_language}}`).
    #[must_use]
    pub const fn italian_name(self) -> &'static str {
        match self {
            Self::It => "italiano",
            Self::En => "inglese",
        }
    }

    /// Picks the text written for this language. For the fixed sentences shown
    /// to the person, which live next to the code that emits them.
    #[must_use]
    pub const fn pick<'a>(self, it: &'a str, en: &'a str) -> &'a str {
        match self {
            Self::It => it,
            Self::En => en,
        }
    }

    /// Name the TTS engine expects.
    #[must_use]
    pub const fn english_name(self) -> &'static str {
        match self {
            Self::It => "Italian",
            Self::En => "English",
        }
    }
}

impl fmt::Display for ContentLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown content language `{0}`")]
pub struct UnknownContentLanguage(pub String);

impl FromStr for ContentLanguage {
    type Err = UnknownContentLanguage;

    /// Exact lowercase codes only, as the contract says.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "it" => Ok(Self::It),
            "en" => Ok(Self::En),
            other => Err(UnknownContentLanguage(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_names_match() {
        for (code, lang, it, en) in [
            ("it", ContentLanguage::It, "italiano", "Italian"),
            ("en", ContentLanguage::En, "inglese", "English"),
        ] {
            assert_eq!(code.parse::<ContentLanguage>().unwrap(), lang);
            assert_eq!(lang.code(), code);
            assert_eq!(lang.italian_name(), it);
            assert_eq!(lang.english_name(), en);
        }
        assert_eq!(ContentLanguage::default(), ContentLanguage::It);
    }

    #[test]
    fn anything_else_is_rejected() {
        for bad in ["", "IT", "En", "fr", "it ", "italiano"] {
            assert!(bad.parse::<ContentLanguage>().is_err(), "{bad}");
        }
    }
}
