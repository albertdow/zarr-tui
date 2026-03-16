use std::{collections::HashSet, ops::Range, sync::Arc};

use anyhow::Result;
use parking_lot::Mutex;
use tokio::sync::{Semaphore, mpsc};

use crate::zarr::{
    chunk_manager::{CachedChunk, ChunkKey, ChunkRange},
    storage::OpenArray,
};

pub struct ChunkRequest {
    pub key: ChunkKey,
    pub ranges: Vec<Range<u64>>,
    pub range: ChunkRange,
    pub array: OpenArray,
}

pub struct ChunkResult {
    pub key: ChunkKey,
    pub data: Result<CachedChunk>,
}

pub struct ChunkLoader {
    request_tx: mpsc::Sender<ChunkRequest>,
    result_rx: mpsc::Receiver<ChunkResult>,
}

impl ChunkLoader {
    pub fn new(max_concurrent: usize) -> Self {
        let (request_tx, mut request_rx) = mpsc::channel::<ChunkRequest>(256);
        let (result_tx, result_rx) = mpsc::channel::<ChunkResult>(256);

        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        let in_flight = Arc::new(Mutex::new(HashSet::<ChunkKey>::new()));

        tokio::spawn(async move {
            while let Some(req) = request_rx.recv().await {
                {
                    let mut flight = in_flight.lock();
                    if flight.contains(&req.key) {
                        continue;
                    }
                    flight.insert(req.key.clone());
                }

                let sem = semaphore.clone();
                let tx = result_tx.clone();
                let flight = in_flight.clone();

                tokio::spawn(async move {
                    let _permit = sem.acquire().await.unwrap();
                    let key = req.key.clone();
                    let range = req.range.clone();

                    let data =
                        tokio::task::spawn_blocking(move || req.array.retrieve_subset(&req.ranges))
                            .await
                            .unwrap();
                    let result = ChunkResult {
                        key: key.clone(),
                        data: data.map(|d| CachedChunk { data: d, range }),
                    };

                    let _ = tx.send(result).await;
                    flight.lock().remove(&key);
                });
            }
        });
        Self {
            request_tx,
            result_rx,
        }
    }

    pub fn request(&self, req: ChunkRequest) {
        let _ = self.request_tx.try_send(req);
    }

    pub fn drain_results(&mut self) -> Vec<ChunkResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.result_rx.try_recv() {
            results.push(result);
        }
        results
    }
}
