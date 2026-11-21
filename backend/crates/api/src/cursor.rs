//! Opaque pagination cursors (contract: api-conventions §8).
//!
//! A cursor is the sort key of the last item returned, base64url-encoded text.
//! Clients must not parse it; the kind prefix only protects against a cursor
//! from one list being replayed on another.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::{
    ChatCursor, ComponentRequestCursor, DueCursor, FolderCursor, FolderId, InProgressCursor,
    JobCursor, JobId, LessonCursor, LessonId, MessageId, MissionCursor, PreferenceCursor,
    PreferenceId, RecordCursor, ReviewCardId, SuspendedCursor, TermCursor, TopicCursor, TopicId,
    parse_uuid,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::ApiError;
use crate::health::format_instant;

fn invalid() -> ApiError {
    ApiError::bad_request("Cursore non valido.", "Invalid cursor.")
}

fn encode(text: &str) -> String {
    URL_SAFE_NO_PAD.encode(text.as_bytes())
}

fn decode(cursor: &str) -> Result<String, ApiError> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| invalid())?;
    String::from_utf8(bytes).map_err(|_| invalid())
}

#[must_use]
pub fn encode_folder(c: &FolderCursor) -> String {
    encode(&format!("f|{}|{}", c.position, c.id))
}

pub fn decode_folder(cursor: &str) -> Result<FolderCursor, ApiError> {
    let text = decode(cursor)?;
    let mut parts = text.split('|');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("f"), Some(pos), Some(id), None) => Ok(FolderCursor {
            position: pos.parse().map_err(|_| invalid())?,
            id: FolderId::new(parse_uuid(id).ok_or_else(invalid)?),
        }),
        _ => Err(invalid()),
    }
}

#[must_use]
pub fn encode_topic(c: &TopicCursor) -> String {
    encode(&format!("t|{}|{}", format_instant(c.created_at), c.id))
}

pub fn decode_topic(cursor: &str) -> Result<TopicCursor, ApiError> {
    let text = decode(cursor)?;
    let mut parts = text.split('|');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("t"), Some(at), Some(id), None) => Ok(TopicCursor {
            created_at: OffsetDateTime::parse(at, &Rfc3339).map_err(|_| invalid())?,
            id: TopicId::new(parse_uuid(id).ok_or_else(invalid)?),
        }),
        _ => Err(invalid()),
    }
}

#[must_use]
pub fn encode_message(c: &ChatCursor) -> String {
    encode(&format!("m|{}|{}", format_instant(c.created_at), c.id))
}

pub fn decode_message(cursor: &str) -> Result<ChatCursor, ApiError> {
    let text = decode(cursor)?;
    let mut parts = text.split('|');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("m"), Some(at), Some(id), None) => Ok(ChatCursor {
            created_at: OffsetDateTime::parse(at, &Rfc3339).map_err(|_| invalid())?,
            id: MessageId::new(parse_uuid(id).ok_or_else(invalid)?),
        }),
        _ => Err(invalid()),
    }
}

#[must_use]
pub fn encode_mission(c: &MissionCursor) -> String {
    encode(&format!("v|{}", c.version))
}

pub fn decode_mission(cursor: &str) -> Result<MissionCursor, ApiError> {
    let text = decode(cursor)?;
    let mut parts = text.split('|');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("v"), Some(version), None) => Ok(MissionCursor {
            version: version.parse().map_err(|_| invalid())?,
        }),
        _ => Err(invalid()),
    }
}

#[must_use]
pub fn encode_preference(c: &PreferenceCursor) -> String {
    encode(&format!("p|{}|{}", format_instant(c.created_at), c.id))
}

pub fn decode_preference(cursor: &str) -> Result<PreferenceCursor, ApiError> {
    let text = decode(cursor)?;
    let mut parts = text.split('|');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("p"), Some(at), Some(id), None) => Ok(PreferenceCursor {
            created_at: OffsetDateTime::parse(at, &Rfc3339).map_err(|_| invalid())?,
            id: PreferenceId::new(parse_uuid(id).ok_or_else(invalid)?),
        }),
        _ => Err(invalid()),
    }
}

#[must_use]
pub fn encode_job(c: &JobCursor) -> String {
    encode(&format!("j|{}|{}", format_instant(c.created_at), c.id))
}

pub fn decode_job(cursor: &str) -> Result<JobCursor, ApiError> {
    let text = decode(cursor)?;
    let mut parts = text.split('|');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("j"), Some(at), Some(id), None) => Ok(JobCursor {
            created_at: OffsetDateTime::parse(at, &Rfc3339).map_err(|_| invalid())?,
            id: JobId::new(parse_uuid(id).ok_or_else(invalid)?),
        }),
        _ => Err(invalid()),
    }
}

#[must_use]
pub fn encode_lesson(c: &LessonCursor) -> String {
    encode(&format!("l|{}", c.number))
}

pub fn decode_lesson(cursor: &str) -> Result<LessonCursor, ApiError> {
    let text = decode(cursor)?;
    match text.split('|').collect::<Vec<_>>().as_slice() {
        ["l", n] => Ok(LessonCursor {
            number: n.parse().map_err(|_| invalid())?,
        }),
        _ => Err(invalid()),
    }
}

#[must_use]
pub fn encode_in_progress(c: &InProgressCursor) -> String {
    encode(&format!(
        "p|{}|{}",
        format_instant(c.last_activity_at),
        c.id
    ))
}

pub fn decode_in_progress(cursor: &str) -> Result<InProgressCursor, ApiError> {
    let text = decode(cursor)?;
    match text.split('|').collect::<Vec<_>>().as_slice() {
        ["p", at, id] => Ok(InProgressCursor {
            last_activity_at: OffsetDateTime::parse(at, &Rfc3339).map_err(|_| invalid())?,
            id: LessonId::new(parse_uuid(id).ok_or_else(invalid)?),
        }),
        _ => Err(invalid()),
    }
}

#[must_use]
pub fn encode_record(c: &RecordCursor) -> String {
    encode(&format!("r|{}", c.number))
}

pub fn decode_record(cursor: &str) -> Result<RecordCursor, ApiError> {
    let text = decode(cursor)?;
    match text.split('|').collect::<Vec<_>>().as_slice() {
        ["r", n] => Ok(RecordCursor {
            number: n.parse().map_err(|_| invalid())?,
        }),
        _ => Err(invalid()),
    }
}

#[must_use]
pub fn encode_due(c: &DueCursor, topic: Option<TopicId>) -> String {
    match topic {
        // Without a filter the format stays the one of before `topic_id`.
        None => encode(&format!("d|{}|{}", format_instant(c.due), c.id)),
        Some(t) => encode(&format!("d|{}|{}|{t}", format_instant(c.due), c.id)),
    }
}

/// A cursor is only good with the `topic_id` filter it was produced with.
pub fn decode_due(cursor: &str, topic: Option<TopicId>) -> Result<DueCursor, ApiError> {
    let text = decode(cursor)?;
    let parts = text.split('|').collect::<Vec<_>>();
    let (at, id, bound) = match parts.as_slice() {
        ["d", at, id] => (at, id, None),
        ["d", at, id, t] => (
            at,
            id,
            Some(TopicId::new(parse_uuid(t).ok_or_else(invalid)?)),
        ),
        _ => return Err(invalid()),
    };
    if bound != topic {
        return Err(ApiError::bad_request(
            "Il cursore non vale per questo topic_id.",
            "The cursor does not apply to this topic_id.",
        ));
    }
    Ok(DueCursor {
        due: OffsetDateTime::parse(at, &Rfc3339).map_err(|_| invalid())?,
        id: ReviewCardId::new(parse_uuid(id).ok_or_else(invalid)?),
    })
}

#[must_use]
pub fn encode_suspended(c: &SuspendedCursor) -> String {
    encode(&format!("s|{}|{}", format_instant(c.suspended_at), c.id))
}

pub fn decode_suspended(cursor: &str) -> Result<SuspendedCursor, ApiError> {
    let text = decode(cursor)?;
    match text.split('|').collect::<Vec<_>>().as_slice() {
        ["s", at, id] => Ok(SuspendedCursor {
            suspended_at: OffsetDateTime::parse(at, &Rfc3339).map_err(|_| invalid())?,
            id: ReviewCardId::new(parse_uuid(id).ok_or_else(invalid)?),
        }),
        _ => Err(invalid()),
    }
}

/// The sort key is user text, so it travels encoded inside the cursor.
#[must_use]
pub fn encode_term(c: &TermCursor) -> String {
    encode(&format!("g|{}|{}", encode(&c.sort_key), encode(&c.key)))
}

pub fn decode_term(cursor: &str) -> Result<TermCursor, ApiError> {
    let text = decode(cursor)?;
    match text.split('|').collect::<Vec<_>>().as_slice() {
        ["g", sort, key] => Ok(TermCursor {
            sort_key: decode(sort)?,
            key: decode(key)?,
        }),
        _ => Err(invalid()),
    }
}

/// The name travels encoded: it is the group key, whatever it holds.
#[must_use]
pub fn encode_component_request(c: &ComponentRequestCursor) -> String {
    encode(&format!(
        "c|{}|{}|{}",
        c.count,
        format_instant(c.last_requested_at),
        encode(&c.wanted)
    ))
}

pub fn decode_component_request(cursor: &str) -> Result<ComponentRequestCursor, ApiError> {
    let text = decode(cursor)?;
    match text.split('|').collect::<Vec<_>>().as_slice() {
        ["c", count, at, wanted] => Ok(ComponentRequestCursor {
            count: count.parse().map_err(|_| invalid())?,
            last_requested_at: OffsetDateTime::parse(at, &Rfc3339).map_err(|_| invalid())?,
            wanted: decode(wanted)?,
        }),
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;
    use uuid::Uuid;

    #[test]
    fn folder_cursor_round_trips() {
        let c = FolderCursor {
            position: 7,
            id: FolderId::new(Uuid::from_u128(42)),
        };
        assert_eq!(decode_folder(&encode_folder(&c)).unwrap(), c);
    }

    #[test]
    fn topic_cursor_round_trips() {
        let c = TopicCursor {
            created_at: datetime!(2026-11-08 14:36:01.129 UTC),
            id: TopicId::new(Uuid::from_u128(42)),
        };
        assert_eq!(decode_topic(&encode_topic(&c)).unwrap(), c);
    }

    #[test]
    fn job_cursor_round_trips_and_is_not_a_message_cursor() {
        let c = JobCursor {
            created_at: datetime!(2026-11-08 14:36:01.129 UTC),
            id: JobId::new(Uuid::from_u128(42)),
        };
        let text = encode_job(&c);
        assert_eq!(decode_job(&text).unwrap(), c);
        assert!(decode_message(&text).is_err());
    }

    #[test]
    fn message_cursor_round_trips_and_is_not_a_topic_cursor() {
        let c = ChatCursor {
            created_at: datetime!(2026-11-08 14:36:01.129 UTC),
            id: MessageId::new(Uuid::from_u128(42)),
        };
        let encoded = encode_message(&c);
        assert_eq!(decode_message(&encoded).unwrap(), c);
        assert!(decode_topic(&encoded).is_err());
    }

    #[test]
    fn mission_and_preference_cursors_round_trip_and_stay_apart() {
        let m = MissionCursor { version: 3 };
        assert_eq!(decode_mission(&encode_mission(&m)).unwrap(), m);
        let p = PreferenceCursor {
            created_at: datetime!(2026-11-08 14:36:01.129 UTC),
            id: PreferenceId::new(Uuid::from_u128(5)),
        };
        assert_eq!(decode_preference(&encode_preference(&p)).unwrap(), p);
        assert!(decode_preference(&encode_mission(&m)).is_err());
        assert!(decode_mission(&encode_preference(&p)).is_err());
        assert!(decode_mission(&encode("v|x")).is_err());
    }

    #[test]
    fn lesson_record_and_term_cursors_round_trip_and_stay_apart() {
        let l = LessonCursor { number: 4 };
        assert_eq!(decode_lesson(&encode_lesson(&l)).unwrap(), l);
        let r = RecordCursor { number: 4 };
        assert_eq!(decode_record(&encode_record(&r)).unwrap(), r);
        let t = TermCursor {
            sort_key: "a|b é".into(),
            key: "k_1".into(),
        };
        assert_eq!(decode_term(&encode_term(&t)).unwrap(), t);
        assert!(decode_record(&encode_lesson(&l)).is_err());
        assert!(decode_lesson(&encode_term(&t)).is_err());
    }

    #[test]
    fn due_cursor_round_trips_and_is_not_a_record_cursor() {
        let c = DueCursor {
            due: datetime!(2026-11-08 14:36:01.129 UTC),
            id: ReviewCardId::new(Uuid::from_u128(9)),
        };
        let text = encode_due(&c, None);
        assert_eq!(decode_due(&text, None).unwrap(), c);
        let topic = TopicId::new(uuid::Uuid::now_v7());
        let text = encode_due(&c, Some(topic));
        assert_eq!(decode_due(&text, Some(topic)).unwrap(), c);
        assert!(decode_due(&text, None).is_err());
        assert!(decode_due(&text, Some(TopicId::new(uuid::Uuid::now_v7()))).is_err());
        assert!(decode_due(&encode_due(&c, None), Some(topic)).is_err());
        assert!(decode_record(&text).is_err());
        assert!(decode_due(&encode_record(&RecordCursor { number: 1 }), None).is_err());
    }

    #[test]
    fn garbage_and_cross_kind_cursors_are_rejected() {
        let f = encode_folder(&FolderCursor {
            position: 0,
            id: FolderId::new(Uuid::nil()),
        });
        assert!(decode_topic(&f).is_err());
        assert!(decode_folder("%%%").is_err());
        assert!(decode_folder(&encode("f|x|y")).is_err());
        assert!(decode_folder(&encode("f|1|2|3")).is_err());
        assert!(decode_folder("").is_err());
    }
}
