use axum::Json;
use axum::extract::State;
use serde::Serialize;
use time::OffsetDateTime;
use time::macros::format_description;

use crate::state::AppState;

pub const API_VERSION: u32 = 1;
pub const CATALOG_VERSION: u32 = 1;

#[derive(Serialize)]
pub struct Liveness {
    status: &'static str,
}

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    version: &'static str,
    api_version: u32,
    catalog_version: u32,
    server_time: String,
}

/// `GET /health`: public liveness.
pub async fn liveness() -> Json<Liveness> {
    Json(Liveness { status: "ok" })
}

/// `GET /api/v1/health`: used by the app's connection test.
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        api_version: API_VERSION,
        catalog_version: CATALOG_VERSION,
        server_time: format_instant(state.clock.now()),
    })
}

/// RFC 3339, UTC, millisecond precision, `Z` suffix (contract §5).
#[must_use]
pub fn format_instant(at: OffsetDateTime) -> String {
    at.to_offset(time::UtcOffset::UTC)
        .format(format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        ))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn formats_with_millis_and_z() {
        let t = datetime!(2026-11-08 15:36:01.129_456 +1);
        assert_eq!(format_instant(t), "2026-11-08T14:36:01.129Z");
        assert_eq!(
            format_instant(datetime!(2026-11-08 14:36:01 UTC)),
            "2026-11-08T14:36:01.000Z"
        );
    }
}
