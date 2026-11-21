use domain::{Page, PageRequest};

pub const DEFAULT_PAGE_SIZE: u32 = 50;
pub const MAX_PAGE_SIZE: u32 = 200;

/// Clamps a requested size into the contract range. The HTTP layer rejects
/// out-of-range values before they get here; this is defence in depth.
#[must_use]
pub fn clamp_limit(limit: Option<u32>) -> u32 {
    limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE)
}

/// Fetches `limit + 1` rows to know whether another page exists without a
/// second query, then trims and derives the next cursor from the last item.
pub(crate) async fn paginate<T, C, F, Fut, E>(
    request: PageRequest<C>,
    fetch: F,
    cursor_of: impl Fn(&T) -> C,
) -> Result<Page<T, C>, E>
where
    F: FnOnce(Option<C>, u32) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<T>, E>>,
{
    let limit = clamp_limit(Some(request.limit));
    let mut items = fetch(request.after, limit + 1).await?;
    let has_more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next = if has_more {
        items.last().map(&cursor_of)
    } else {
        None
    };
    Ok(Page { items, next })
}
