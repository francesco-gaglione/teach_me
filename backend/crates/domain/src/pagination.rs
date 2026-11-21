/// A page request: keyset cursor plus a page size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest<C> {
    pub after: Option<C>,
    pub limit: u32,
}

/// A page of results; `next` is set when more items follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T, C> {
    pub items: Vec<T>,
    pub next: Option<C>,
}
