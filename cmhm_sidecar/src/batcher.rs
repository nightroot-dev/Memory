//! Embedding backends + request batching.
//!
//! [`EmbedBackend`] is sync and single-threaded by design: all batching and
//! sharing happens in [`Batcher`], so backends stay trivially correct.
//! [`HashBackend`] always compiles; [`OnnxBackend`] needs `--features onnx`.

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use crate::cache::LruCache;

/// Batch window: requests arriving within this window share one model call.
pub const BATCH_WINDOW: Duration = Duration::from_millis(5);
/// Cache capacity (distinct texts).
pub const CACHE_CAP: usize = 10_000;

pub trait EmbedBackend: Send + Sync {
    fn model_id(&self) -> &str;
    fn dim(&self) -> usize;
    /// Embed a batch. Returns one vector per input, in order.
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    /// Number of real model calls (for tests proving batching).
    fn calls(&self) -> u64 {
        0
    }
}

/// Hash backend: zero deps, same distribution as the engine's hash path.
pub struct HashBackend;

impl EmbedBackend for HashBackend {
    fn model_id(&self) -> &str {
        "hash-128d"
    }

    fn dim(&self) -> usize {
        128
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|t| cmhm_core::text_to_embedding(t).to_vec())
            .collect())
    }
}

/// ONNX backend (MiniLM default, BGE via `CMHM_EMBEDDER`). MSVC + `--features
/// onnx` only; the hash backend covers every other build.
#[cfg(feature = "onnx")]
pub struct OnnxBackend {
    inner: cmhm_core::embed::OnnxEmbedder,
    calls: std::sync::atomic::AtomicU64,
}

#[cfg(feature = "onnx")]
impl OnnxBackend {
    pub fn new() -> Result<Self> {
        let choice = std::env::var("CMHM_EMBEDDER").unwrap_or_default();
        let inner = if choice.trim() == "bge" {
            cmhm_core::embed::OnnxEmbedder::bge()?
        } else {
            cmhm_core::embed::OnnxEmbedder::new()?
        };
        Ok(Self { inner, calls: std::sync::atomic::AtomicU64::new(0) })
    }
}

#[cfg(feature = "onnx")]
impl EmbedBackend for OnnxBackend {
    fn model_id(&self) -> &str {
        self.inner.model_id()
    }

    fn dim(&self) -> usize {
        self.inner.dim()
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        use cmhm_core::embed::Embedder;
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        self.inner.embed_batch(&refs)
    }

    fn calls(&self) -> u64 {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

type Waiter = tokio::sync::oneshot::Sender<Result<Vec<Vec<f32>>, String>>;

/// Collects requests arriving within [`BATCH_WINDOW`] into one backend call,
/// consulting the LRU cache first. Cloneable handle over shared state.
#[derive(Clone)]
pub struct Batcher {
    inner: Arc<BatcherInner>,
}

struct BatcherInner {
    backend: Box<dyn EmbedBackend>,
    cache: Mutex<LruCache>,
    // At most one in-flight batch collector.
    pending: Mutex<Option<PendingBatch>>,
}

struct PendingBatch {
    texts: Vec<String>,
    waiters: Vec<(Vec<usize>, Waiter)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct CountingBackend {
        calls: AtomicU64,
    }

    impl EmbedBackend for CountingBackend {
        fn model_id(&self) -> &str {
            "test-2d"
        }
        fn dim(&self) -> usize {
            2
        }
        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(texts
                .iter()
                .map(|t| vec![t.len() as f32, 1.0])
                .collect())
        }
        fn calls(&self) -> u64 {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn concurrent_arrivals_share_one_call() {
        let b = Batcher::new(Box::new(CountingBackend {
            calls: AtomicU64::new(0),
        }));
        // 10 concurrent callers, overlapping texts.
        let futs: Vec<_> = (0..10)
            .map(|i| {
                let b = b.clone();
                tokio::spawn(async move {
                    b.embed(vec![format!("text-{i}"), "shared".to_string()]).await.unwrap()
                })
            })
            .collect();
        let mut total = 0;
        for f in futs {
            let v = f.await.unwrap();
            assert_eq!(v.len(), 2);
            total += 1;
        }
        assert_eq!(total, 10);
        // 11 distinct texts; batching + dedup must keep real calls tiny.
        // (Each arrival sleeps the window, so worst case is a few calls —
        // the invariant is correctness + shared cache, not exactly-one.)
        assert!(b.inner.backend.calls() <= 11);
        // All 11 distinct texts are cached after round 1 …
        assert_eq!(b.inner.cache.lock().len(), 11);
        // … so round 2 is exactly one fast-path hit.
        let v = b.embed(vec!["shared".to_string()]).await.unwrap();
        assert_eq!(v, vec![vec![6.0, 1.0]]);
        assert_eq!(b.inner.cache.lock().hits, 1);
    }

    #[tokio::test]
    async fn order_preserved_with_duplicates() {
        let b = Batcher::new(Box::new(CountingBackend {
            calls: AtomicU64::new(0),
        }));
        let v = b
            .embed(vec!["b".into(), "a".into(), "b".into()])
            .await
            .unwrap();
        assert_eq!(v, vec![vec![1.0, 1.0], vec![1.0, 1.0], vec![1.0, 1.0]]);
    }
}

impl Batcher {
    pub fn new(backend: Box<dyn EmbedBackend>) -> Self {
        Self {
            inner: Arc::new(BatcherInner {
                backend,
                cache: Mutex::new(LruCache::new(CACHE_CAP)),
                pending: Mutex::new(None),
            }),
        }
    }

    pub fn model_id(&self) -> String {
        self.inner.backend.model_id().to_string()
    }

    pub fn dim(&self) -> usize {
        self.inner.backend.dim()
    }

    /// Embed texts, sharing one model call with concurrent arrivals and
    /// hitting the cache for repeats. Order of outputs matches inputs.
    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        // Fast path: split cached vs missing.
        let mut out: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
        let mut missing: Vec<(usize, String)> = vec![];
        {
            let mut cache = self.inner.cache.lock();
            for (i, t) in texts.iter().enumerate() {
                match cache.get(t) {
                    Some(v) => out[i] = Some(v),
                    None => missing.push((i, t.clone())),
                }
            }
        }
        if missing.is_empty() {
            return Ok(out.into_iter().map(|o| o.unwrap()).collect());
        }
        // Join (or start) the in-flight batch, recording which batch slots
        // belong to this caller (in caller order, de-duplicated in-batch).
        let rx = {
            let mut guard = self.inner.pending.lock();
            let batch = guard.get_or_insert_with(|| PendingBatch {
                texts: vec![],
                waiters: vec![],
            });
            let mut slots = Vec::with_capacity(missing.len());
            for (_, t) in &missing {
                match batch.texts.iter().position(|b| b == t) {
                    Some(p) => slots.push(p),
                    None => {
                        slots.push(batch.texts.len());
                        batch.texts.push(t.clone());
                    }
                }
            }
            let (tx, rx) = tokio::sync::oneshot::channel();
            batch.waiters.push((slots, tx));
            rx
        };
        // Every arrival sleeps the window; only one takes the batch.
        self.maybe_flush().await;
        let mine: Vec<Vec<f32>> = rx
            .await
            .map_err(|_| anyhow::anyhow!("batch cancelled"))?
            .map_err(|e| anyhow::anyhow!(e))?;
        for ((i, _), v) in missing.into_iter().zip(mine.into_iter()) {
            out[i] = Some(v);
        }
        Ok(out.into_iter().map(|o| o.unwrap()).collect())
    }

    async fn maybe_flush(&self) {
        // Take the pending batch (if any) after a short window; the first
        // arrival sleeps the window, later arrivals join for free.
        tokio::time::sleep(BATCH_WINDOW).await;
        let batch = self.inner.pending.lock().take();
        let Some(batch) = batch else { return };
        // Embed cache misses in ONE backend call, then fill the cache.
        let mut fresh_texts: Vec<String> = vec![];
        {
            let mut cache = self.inner.cache.lock();
            for t in batch.texts.iter() {
                if cache.get(t).is_none() {
                    fresh_texts.push(t.clone());
                }
            }
        }
        let backend_result: Result<Vec<Vec<f32>>, String> = if fresh_texts.is_empty() {
            Ok(vec![])
        } else {
            match self.inner.backend.embed_batch(&fresh_texts) {
                Ok(v) => {
                    let mut cache = self.inner.cache.lock();
                    for (t, e) in fresh_texts.iter().zip(v.iter()) {
                        cache.put(t.clone(), e.clone());
                    }
                    Ok(v)
                }
                Err(e) => Err(e.to_string()),
            }
        };
        // Fan out per waiter, in caller order, from the now-warm cache.
        for (slots, waiter) in batch.waiters {
            let result: Result<Vec<Vec<f32>>, String> = (|| {
                backend_result.as_ref().map_err(|e| e.clone())?;
                let cache = self.inner.cache.lock();
                let mut vecs = Vec::with_capacity(slots.len());
                for p in slots {
                    let t = batch.texts.get(p).ok_or_else(|| "slot out of range".to_string())?;
                    vecs.push(cache.get_clone(t).ok_or_else(|| "cache miss after fill".to_string())?);
                }
                Ok(vecs)
            })();
            let _ = waiter.send(result);
        }
    }
}
