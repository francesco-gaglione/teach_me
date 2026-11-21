//! A user's personal review weights change the previews and the votes of
//! their own cards only (ADR 0014 point 12), over the real router, SQLite and
//! scheduler, with two users.

mod common;

use axum::http::{Method, StatusCode};
use common::TOKEN;
use common::harness::Harness;
use domain::{ReviewScheduler, UserId};
use infrastructure::FsrsScheduler;
use serde_json::{Value, json};

const ADOPTED_AT: &str = "2026-11-01T00:00:00.000Z";

/// A card due since a week, voted twice on the current model, with the votes
/// in the log (what a replay reads). The memory values are not what a replay
/// of those votes gives, so a card that is replayed shows it.
async fn seed_card(h: &Harness, token: &str, name: &str, n: u128) -> String {
    let folder = h.folder(token, &format!("Folder {name}")).await;
    let topic = h.topic(token, &folder, &format!("Topic {name}")).await;
    let lesson = format!("00000000-0000-0000-0000-0000000001{n:02x}");
    let term = format!("00000000-0000-0000-0000-0000000002{n:02x}");
    let card = format!("00000000-0000-0000-0000-0000000003{n:02x}");
    let at = "2026-10-01T09:00:00.000Z";
    for (query, binds) in [
        (
            "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, \
             catalog_version, mission_version, document, plan, generation_job_id, created_at) \
             VALUES (?, ?, 1, 'L', 'o', 5, 1, 1, '{}', '{}', ?, ?)",
            vec![&lesson, &topic, &lesson, &at.to_owned()],
        ),
        (
            "INSERT INTO glossary_terms (id, topic_id, key, term, term_key, definition, avoid, \
             lesson_id, created_at, updated_at) VALUES (?, ?, ?, 'x', 'x', 'd', '[]', ?, ?, ?)",
            vec![
                &term,
                &topic,
                &term,
                &lesson,
                &at.to_owned(),
                &at.to_owned(),
            ],
        ),
        (
            "INSERT INTO review_cards (id, topic_id, lesson_id, term_id, front, back, created_at, \
             state, stability, difficulty, stability_fast, due, last_reviewed_at, review_count, \
             model) VALUES (?, ?, ?, ?, 'f', 'b', ?, 'review', 10, 5, 10, \
             '2026-11-01T00:00:00.000Z', '2026-10-25T10:00:00.000Z', 2, 'fsrs7')",
            vec![&card, &topic, &lesson, &term, &at.to_owned()],
        ),
    ] {
        let mut q = sqlx::query(query);
        for b in binds {
            q = q.bind(b);
        }
        q.execute(&h.pool).await.unwrap();
    }
    for (i, reviewed) in ["2026-10-15T10:00:00.000Z", "2026-10-25T10:00:00.000Z"]
        .into_iter()
        .enumerate()
    {
        sqlx::query(
            "INSERT INTO review_logs (id, card_id, rating, reviewed_at, state_before, \
             state_after, created_at) VALUES (?, ?, 'good', ?, '{}', '{}', ?)",
        )
        .bind(format!("00000000-0000-0000-0000-0000000004{n:02x}{i:02x}"))
        .bind(&card)
        .bind(reviewed)
        .bind(reviewed)
        .execute(&h.pool)
        .await
        .unwrap();
    }
    card
}

async fn due_card(h: &Harness, token: &str) -> Value {
    let r = h
        .request(Some(token), Method::GET, "/api/v1/reviews/due", None)
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["items"].as_array().unwrap().len(), 1);
    r.body["items"][0].clone()
}

async fn vote(h: &Harness, token: &str, card: &str) -> Value {
    let r = h
        .request(
            Some(token),
            Method::POST,
            &format!("/api/v1/reviews/{card}"),
            Some(json!({ "rating": "good", "review_count": 2 })),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    r.body
}

#[tokio::test]
async fn the_weights_of_a_user_change_the_preview_and_the_vote_of_their_cards_only() {
    let h = Harness::new(vec![]).await;
    let bob = h.user("bob", false).await;
    let alice_card = seed_card(&h, TOKEN, "alice", 1).await;
    let bob_card = seed_card(&h, &bob, "bob", 2).await;

    // Same history, same memory, same weights (the defaults): same preview.
    let alice_before = due_card(&h, TOKEN).await["intervals_preview"]["good"].clone();
    let bob_before = due_card(&h, &bob).await["intervals_preview"]["good"].clone();
    assert_eq!(alice_before, bob_before);

    // Weights for the owner only: the first weight (initial stability of
    // "again") is doubled.
    let mut weights = FsrsScheduler::new().default_parameters().weights;
    weights[0] *= 2.0;
    sqlx::query(
        "INSERT INTO user_review_parameters (user_id, model, weights, adopted_at) \
         VALUES (?, 'fsrs7', ?, ?)",
    )
    .bind(UserId::OWNER.to_string())
    .bind(serde_json::to_string(&weights).unwrap())
    .bind(ADOPTED_AT)
    .execute(&h.pool)
    .await
    .unwrap();

    let alice = due_card(&h, TOKEN).await;
    let bob_card_now = due_card(&h, &bob).await;
    assert_ne!(
        alice["intervals_preview"]["good"], bob_before,
        "the owner's weights change the owner's preview"
    );
    assert_eq!(
        bob_card_now["intervals_preview"]["good"], bob_before,
        "and nobody else's"
    );

    // The vote applies what was announced, with each user's own weights.
    let voted_alice = vote(&h, TOKEN, &alice_card).await;
    let voted_bob = vote(&h, &bob, &bob_card).await;
    let now = common::harness::T0;
    for (voted, announced) in [
        (&voted_alice, &alice["intervals_preview"]["good"]),
        (&voted_bob, &bob_before),
    ] {
        let due = time::OffsetDateTime::parse(
            voted["due"].as_str().unwrap(),
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        assert_eq!((due - now).whole_seconds(), announced.as_i64().unwrap());
    }
    assert_ne!(voted_alice["due"], voted_bob["due"]);

    // The stamp on each card says which weights its memory comes from.
    let stamp = |card: String| {
        let pool = h.pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT weights_adopted_at FROM review_cards WHERE id = ?",
            )
            .bind(card)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(stamp(alice_card).await.as_deref(), Some(ADOPTED_AT));
    assert_eq!(stamp(bob_card).await, None);
}
