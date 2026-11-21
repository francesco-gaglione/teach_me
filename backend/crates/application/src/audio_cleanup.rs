//! The daily cleanup of the clip cache (ADR 0013 §Consequences): a first pass
//! a few minutes after the start, then one a day, until the task is aborted at
//! shutdown. What a pass deletes and why it is safe is in
//! [`crate::ClipCache::clean_up`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::audio::ClipCache;

/// How long after the start the first pass runs: not in the way of the start.
pub const FIRST_RUN_DELAY: Duration = Duration::from_mins(5);
/// Time between two passes.
pub const RUN_INTERVAL: Duration = Duration::from_hours(24);

pub struct AudioCleaner {
    cache: Arc<ClipCache>,
}

impl AudioCleaner {
    #[must_use]
    pub fn new(cache: Arc<ClipCache>) -> Self {
        Self { cache }
    }

    /// Runs one pass and logs what it did. A pass cut short by the shutdown
    /// loses nothing: rows go before files and the leftovers are swept by the
    /// next pass.
    pub async fn run_once(&self) {
        let started = Instant::now();
        match self.cache.clean_up().await {
            Ok(report) => tracing::info!(
                clips_deleted = report.clips_deleted,
                bytes_freed = report.bytes_freed,
                orphans_deleted = report.orphans_deleted,
                orphan_bytes_freed = report.orphan_bytes_freed,
                failed = report.failed,
                elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                "audio cache cleaned"
            ),
            Err(error) => tracing::error!(%error, "could not clean the audio cache"),
        }
    }

    pub async fn run(self: Arc<Self>) {
        tokio::time::sleep(FIRST_RUN_DELAY).await;
        loop {
            self.run_once().await;
            tokio::time::sleep(RUN_INTERVAL).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::{ClipStore, Clock, IdGenerator};
    use time::macros::datetime;

    use super::*;
    use crate::audio::tests::{FakeTts, MemClips, MemStore};
    use crate::fakes::{SeqIds, TestClock};

    fn cleaner(clips: Arc<MemClips>, store: Arc<MemStore>) -> (Arc<AudioCleaner>, Arc<TestClock>) {
        let clock = Arc::new(TestClock::new(datetime!(2026-11-13 10:00 UTC)));
        let clock_dyn: Arc<dyn Clock> = clock.clone();
        let ids: Arc<dyn IdGenerator> = Arc::new(SeqIds::default());
        let cache = Arc::new(ClipCache::new(
            Arc::new(FakeTts::new("m")),
            clips,
            store,
            clock_dyn,
            ids,
        ));
        (Arc::new(AudioCleaner::new(cache)), clock)
    }

    #[tokio::test(start_paused = true)]
    async fn passes_run_after_the_first_delay_then_every_interval() {
        let clips = Arc::new(MemClips::default());
        let store = Arc::new(MemStore::default());
        let (cleaner, _) = cleaner(clips, store.clone());
        // A file with no row, older than a day, is what a pass removes.
        let orphan = domain::ClipId::new(uuid::Uuid::from_u128(1));
        store.put(orphan, b"x").await.unwrap();
        store
            .1
            .lock()
            .unwrap()
            .insert(orphan, datetime!(2026-01-01 00:00 UTC));

        let task = tokio::spawn(cleaner.run());
        tokio::time::sleep(FIRST_RUN_DELAY.saturating_sub(Duration::from_secs(1))).await;
        assert!(store.exists(orphan).await.unwrap(), "ran too early");
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(!store.exists(orphan).await.unwrap(), "first pass missing");

        let again = domain::ClipId::new(uuid::Uuid::from_u128(2));
        store.put(again, b"x").await.unwrap();
        store
            .1
            .lock()
            .unwrap()
            .insert(again, datetime!(2026-01-01 00:00 UTC));
        tokio::time::sleep(RUN_INTERVAL.saturating_sub(Duration::from_secs(10))).await;
        assert!(store.exists(again).await.unwrap(), "second pass too early");
        tokio::time::sleep(Duration::from_secs(20)).await;
        assert!(!store.exists(again).await.unwrap(), "second pass missing");
        task.abort();
    }

    /// Fails to list the files for the first pass, works after.
    struct FlakyStore {
        inner: MemStore,
        failures: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl domain::ClipStore for FlakyStore {
        async fn put(&self, id: domain::ClipId, audio: &[u8]) -> std::io::Result<()> {
            self.inner.put(id, audio).await
        }
        async fn exists(&self, id: domain::ClipId) -> std::io::Result<bool> {
            self.inner.exists(id).await
        }
        async fn remove(&self, id: domain::ClipId) -> std::io::Result<Option<u64>> {
            self.inner.remove(id).await
        }
        async fn list(&self) -> std::io::Result<Vec<domain::StoredFile>> {
            use std::sync::atomic::Ordering;
            if self.failures.load(Ordering::SeqCst) > 0 {
                self.failures.fetch_sub(1, Ordering::SeqCst);
                return Err(std::io::Error::other("disk gone"));
            }
            self.inner.list().await
        }
        async fn read(&self, id: domain::ClipId) -> std::io::Result<Option<Vec<u8>>> {
            self.inner.read(id).await
        }
        async fn size(&self, id: domain::ClipId) -> std::io::Result<Option<u64>> {
            self.inner.size(id).await
        }
        async fn read_range(
            &self,
            id: domain::ClipId,
            offset: u64,
            len: usize,
        ) -> std::io::Result<Option<Vec<u8>>> {
            self.inner.read_range(id, offset, len).await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_pass_does_not_stop_the_loop() {
        let store = Arc::new(FlakyStore {
            inner: MemStore::default(),
            failures: std::sync::atomic::AtomicU32::new(1),
        });
        let orphan = domain::ClipId::new(uuid::Uuid::from_u128(1));
        store.put(orphan, b"x").await.unwrap();
        store
            .inner
            .1
            .lock()
            .unwrap()
            .insert(orphan, datetime!(2026-01-01 00:00 UTC));
        let clock: Arc<dyn Clock> = Arc::new(TestClock::new(datetime!(2026-11-13 10:00 UTC)));
        let cache = Arc::new(ClipCache::new(
            Arc::new(FakeTts::new("m")),
            Arc::new(MemClips::default()),
            store.clone(),
            clock,
            Arc::new(SeqIds::default()),
        ));
        let task = tokio::spawn(Arc::new(AudioCleaner::new(cache)).run());
        tokio::time::sleep(FIRST_RUN_DELAY + Duration::from_secs(1)).await;
        assert!(store.exists(orphan).await.unwrap(), "the first pass fails");
        tokio::time::sleep(RUN_INTERVAL).await;
        assert!(!store.exists(orphan).await.unwrap(), "the loop went on");
        task.abort();
    }
}
