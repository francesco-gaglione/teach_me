use time::OffsetDateTime;

use crate::ids::{FolderId, UserId};
use crate::validation::{ValidationIssue, normalize_text};

pub const FOLDER_NAME_MAX_CHARS: usize = 80;

/// Validated folder name: trimmed, 1-80 code points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderName(String);

impl FolderName {
    pub fn parse(raw: &str) -> Result<Self, ValidationIssue> {
        normalize_text(raw, FOLDER_NAME_MAX_CHARS, "name").map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Case-insensitive identity used for uniqueness. Lowercasing is Unicode
    /// aware, unlike SQLite's `lower()`, so the key is computed here.
    #[must_use]
    pub fn unique_key(&self) -> String {
        self.0.to_lowercase()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub user_id: UserId,
    pub name: FolderName,
    pub position: u32,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl Folder {
    /// Renames the folder. Returns whether anything changed, so callers can
    /// skip the write and leave `updated_at` alone on a no-op.
    pub fn rename(&mut self, name: FolderName, now: OffsetDateTime) -> bool {
        if self.name == name {
            return false;
        }
        self.name = name;
        self.updated_at = now;
        true
    }
}

/// Folder to be stored; the repository assigns `position` atomically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFolder {
    pub id: FolderId,
    pub user_id: UserId,
    pub name: FolderName,
    pub created_at: OffsetDateTime,
}

/// Folder plus the computed number of topics it contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSummary {
    pub folder: Folder,
    pub topic_count: u64,
}

/// Keyset position in the folder ordering (`position`, then `id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderCursor {
    pub position: u32,
    pub id: FolderId,
}

impl FolderCursor {
    #[must_use]
    pub fn after(folder: &Folder) -> Self {
        Self {
            position: folder.position,
            id: folder.id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::IssueKind;

    fn issue(raw: &str) -> IssueKind {
        FolderName::parse(raw).unwrap_err().issue
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(FolderName::parse("  Lavoro \n").unwrap().as_str(), "Lavoro");
    }

    #[test]
    fn rejects_empty_and_blank() {
        assert_eq!(issue(""), IssueKind::Empty);
        assert_eq!(issue(" \t\n\u{a0}"), IssueKind::Empty);
    }

    #[test]
    fn length_boundary_is_80() {
        assert!(FolderName::parse(&"a".repeat(80)).is_ok());
        assert_eq!(issue(&"a".repeat(81)), IssueKind::TooLong);
    }

    #[test]
    fn length_is_measured_after_trim() {
        assert!(FolderName::parse(&format!("  {}  ", "a".repeat(80))).is_ok());
    }

    #[test]
    fn length_counts_code_points_not_bytes() {
        assert!(FolderName::parse(&"è".repeat(80)).is_ok());
        assert!(FolderName::parse(&"😀".repeat(80)).is_ok());
        assert_eq!(issue(&"😀".repeat(81)), IssueKind::TooLong);
    }

    #[test]
    fn unique_key_ignores_case_including_non_ascii() {
        let a = FolderName::parse("Èconomia").unwrap();
        let b = FolderName::parse("èCONOMIA").unwrap();
        assert_eq!(a.unique_key(), b.unique_key());
    }

    #[test]
    fn rename_reports_no_change_for_identical_name() {
        let t = time::macros::datetime!(2026-11-08 10:00 UTC);
        let later = time::macros::datetime!(2026-11-09 10:00 UTC);
        let mut f = Folder {
            id: FolderId::new(uuid::Uuid::nil()),
            user_id: UserId::OWNER,
            name: FolderName::parse("A").unwrap(),
            position: 0,
            created_at: t,
            updated_at: t,
        };
        assert!(!f.rename(FolderName::parse(" A ").unwrap(), later));
        assert_eq!(f.updated_at, t);
        assert!(f.rename(FolderName::parse("a").unwrap(), later));
        assert_eq!(f.updated_at, later);
    }
}
