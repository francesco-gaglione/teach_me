use std::fmt;

use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub const fn new(id: Uuid) -> Self {
                Self(id)
            }

            #[must_use]
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(UserId);
id_type!(SessionId);
id_type!(FolderId);
id_type!(TopicId);
id_type!(MessageId);
id_type!(PreferenceId);
id_type!(JobId);
id_type!(ResourceId);
id_type!(LessonId);
id_type!(AttemptId);
id_type!(LearningRecordId);
id_type!(GlossaryTermId);
id_type!(ReviewCardId);
id_type!(ReviewLogId);
id_type!(ComponentRequestId);
id_type!(DocumentId);
id_type!(ClipId);

impl UserId {
    /// The user the migration creates for the data that existed before there
    /// were users. The same literal is in `migrations/0014_users.sql`.
    pub const OWNER: Self = Self(Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0001));
}

/// Parses the canonical hyphenated form only (36 chars), as the API contract
/// specifies; `Uuid::parse_str` alone would also take braced/simple/urn forms.
#[must_use]
pub fn parse_uuid(raw: &str) -> Option<Uuid> {
    if raw.len() != 36 {
        return None;
    }
    Uuid::parse_str(raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_hyphenated_form() {
        let ok = "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f";
        assert!(parse_uuid(ok).is_some());
        assert!(parse_uuid(&ok.to_uppercase()).is_some());
        assert!(parse_uuid(&ok.replace('-', "")).is_none());
        assert!(parse_uuid(&format!("{{{ok}}}")).is_none());
        assert!(parse_uuid("").is_none());
        assert!(parse_uuid("not-a-uuid").is_none());
    }
}
