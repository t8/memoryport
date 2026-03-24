use crate::models::{Batch, Chunk};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum BatcherError {
    #[error("flush callback failed: {0}")]
    FlushFailed(String),
}

/// Type alias for the async flush callback.
pub type FlushCallback = Arc<
    dyn Fn(Batch) -> futures::future::BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>
        + Send
        + Sync,
>;

struct BatcherInner {
    buffer: Vec<Chunk>,
    max_chunks: usize,
    flush_interval: Duration,
    last_flush: Instant,
    on_flush: FlushCallback,
}

/// Accumulates chunks and flushes them as batches based on count or time triggers.
pub struct Batcher {
    inner: Arc<Mutex<BatcherInner>>,
}

impl Batcher {
    pub fn new(max_chunks: usize, flush_interval: Duration, on_flush: FlushCallback) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BatcherInner {
                buffer: Vec::new(),
                max_chunks,
                flush_interval,
                last_flush: Instant::now(),
                on_flush,
            })),
        }
    }

    /// Add a chunk to the buffer. May trigger a flush if the buffer reaches capacity.
    pub async fn add(&self, chunk: Chunk) -> Result<(), BatcherError> {
        let should_flush = {
            let mut inner = self.inner.lock().await;
            inner.buffer.push(chunk);
            inner.buffer.len() >= inner.max_chunks
        };

        if should_flush {
            self.flush().await?;
        }

        Ok(())
    }

    /// Add multiple chunks at once.
    pub async fn add_many(&self, chunks: Vec<Chunk>) -> Result<(), BatcherError> {
        for chunk in chunks {
            self.add(chunk).await?;
        }
        Ok(())
    }

    /// Force flush all buffered chunks.
    pub async fn flush(&self) -> Result<(), BatcherError> {
        let (batch, callback) = {
            let mut inner = self.inner.lock().await;
            if inner.buffer.is_empty() {
                return Ok(());
            }

            let chunks = std::mem::take(&mut inner.buffer);
            inner.last_flush = Instant::now();
            let batch = Batch::new(chunks);

            debug!(
                batch_id = %batch.id,
                chunk_count = batch.chunks.len(),
                "flushing batch"
            );

            (batch, inner.on_flush.clone())
        };

        callback(batch)
            .await
            .map_err(|e| BatcherError::FlushFailed(e.to_string()))
    }

    /// Start a background timer that flushes at the configured interval.
    pub fn start_timer(&self) -> tokio::task::JoinHandle<()> {
        let inner = self.inner.clone();

        tokio::spawn(async move {
            loop {
                let sleep_duration = {
                    let inner = inner.lock().await;
                    inner.flush_interval
                };

                tokio::time::sleep(sleep_duration).await;

                let should_flush = {
                    let inner = inner.lock().await;
                    !inner.buffer.is_empty()
                        && inner.last_flush.elapsed() >= inner.flush_interval
                };

                if should_flush {
                    let (batch, callback) = {
                        let mut inner = inner.lock().await;
                        if inner.buffer.is_empty() {
                            continue;
                        }
                        let chunks = std::mem::take(&mut inner.buffer);
                        inner.last_flush = Instant::now();
                        let batch = Batch::new(chunks);
                        (batch, inner.on_flush.clone())
                    };

                    if let Err(e) = callback(batch).await {
                        warn!(error = %e, "timer-triggered flush failed");
                    }
                }
            }
        })
    }

    /// Number of chunks currently buffered.
    pub async fn pending_count(&self) -> usize {
        self.inner.lock().await.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChunkMetadata, ChunkType, Role};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    fn test_chunk() -> Chunk {
        Chunk {
            id: Uuid::new_v4(),
            chunk_type: ChunkType::Conversation,
            session_id: "s1".into(),
            timestamp: 1000,
            role: Some(Role::User),
            content: "test".into(),
            metadata: ChunkMetadata::default(),
        }
    }

    #[tokio::test]
    async fn test_flush_on_count() {
        let flush_count = Arc::new(AtomicUsize::new(0));
        let fc = flush_count.clone();

        let callback: FlushCallback = Arc::new(move |_batch| {
            let fc = fc.clone();
            Box::pin(async move {
                fc.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });

        let batcher = Batcher::new(3, Duration::from_secs(60), callback);

        batcher.add(test_chunk()).await.unwrap();
        batcher.add(test_chunk()).await.unwrap();
        assert_eq!(flush_count.load(Ordering::SeqCst), 0);

        batcher.add(test_chunk()).await.unwrap(); // triggers flush
        assert_eq!(flush_count.load(Ordering::SeqCst), 1);
        assert_eq!(batcher.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_explicit_flush() {
        let flush_count = Arc::new(AtomicUsize::new(0));
        let fc = flush_count.clone();

        let callback: FlushCallback = Arc::new(move |_batch| {
            let fc = fc.clone();
            Box::pin(async move {
                fc.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });

        let batcher = Batcher::new(100, Duration::from_secs(60), callback);
        batcher.add(test_chunk()).await.unwrap();
        batcher.flush().await.unwrap();
        assert_eq!(flush_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_empty_flush_is_noop() {
        let callback: FlushCallback = Arc::new(|_| {
            Box::pin(async { panic!("should not be called") })
        });

        let batcher = Batcher::new(10, Duration::from_secs(60), callback);
        batcher.flush().await.unwrap(); // no-op
    }
}
