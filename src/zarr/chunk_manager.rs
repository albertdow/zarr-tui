use lru::LruCache;
use ndarray::{ArrayD, IxDyn};
use std::num::NonZeroUsize;
use std::ops::Range;

use super::storage::OpenArray;

/// Key for cached chunks
#[derive(Hash, PartialEq, Eq, Clone)]
pub struct ChunkKey {
    pub variable_name: String,
    pub time_idx: usize,
    pub chunk_lat: usize,
    pub chunk_lon: usize,
}

impl ChunkKey {
    pub fn new(variable_name: &str, time_idx: usize, chunk_lat: usize, chunk_lon: usize) -> Self {
        Self {
            variable_name: variable_name.to_string(),
            time_idx,
            chunk_lat,
            chunk_lon,
        }
    }
}

/// Cached chunk with data and range info
pub struct CachedChunk {
    pub data: ArrayD<f32>,
    pub range: ChunkRange,
}

/// Manages lazy loading and caching of chunks
pub struct ChunkManager {
    cache: LruCache<ChunkKey, CachedChunk>,
    pub chunk_size_lat: usize,
    pub chunk_size_lon: usize,
    pub total_lat: usize,
    pub total_lon: usize,
    /// Number of chunks loaded in the last load call
    pub last_loaded_count: usize,
    /// Number of chunks that were needed but not cached
    pub pending_chunks: usize,
}

impl ChunkManager {
    pub fn new(
        chunk_size_lat: usize,
        chunk_size_lon: usize,
        total_lat: usize,
        total_lon: usize,
        capacity: usize,
    ) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
            chunk_size_lat,
            chunk_size_lon,
            total_lat,
            total_lon,
            last_loaded_count: 0,
            pending_chunks: 0,
        }
    }

    /// Convert array indices to chunk indices
    pub fn indices_to_chunk(&self, lat_idx: usize, lon_idx: usize) -> (usize, usize) {
        (lat_idx / self.chunk_size_lat, lon_idx / self.chunk_size_lon)
    }

    /// Convert chunk indices to array index range (clamped to total size)
    pub fn chunk_to_range(&self, chunk_lat: usize, chunk_lon: usize) -> ChunkRange {
        ChunkRange {
            lat_start: chunk_lat * self.chunk_size_lat,
            lat_end: ((chunk_lat + 1) * self.chunk_size_lat).min(self.total_lat),
            lon_start: chunk_lon * self.chunk_size_lon,
            lon_end: ((chunk_lon + 1) * self.chunk_size_lon).min(self.total_lon),
        }
    }

    /// Get value at indices if the chunk is cached, None otherwise
    pub fn get_value_if_cached(
        &mut self,
        variable_name: &str,
        time_idx: usize,
        lat_idx: usize,
        lon_idx: usize,
        lat_axis: usize,
        lon_axis: usize,
        ndim: usize,
    ) -> Option<f32> {
        let (chunk_lat, chunk_lon) = self.indices_to_chunk(lat_idx, lon_idx);
        let key = ChunkKey::new(variable_name, time_idx, chunk_lat, chunk_lon);

        if let Some(cached) = self.cache.get(&key) {
            let (local_lat, local_lon) = cached.range.to_local(lat_idx, lon_idx);
            let mut idx = vec![0; ndim];
            idx[lat_axis] = local_lat;
            idx[lon_axis] = local_lon;
            cached.data.get(IxDyn(&idx)).copied()
        } else {
            None
        }
    }

    /// Get averaged value over an index range (for zoomed-out views).
    /// Returns the mean of all valid (finite) values in the range.
    /// Falls back to center point if no valid samples found.
    pub fn get_averaged_value_if_cached(
        &mut self,
        variable_name: &str,
        time_idx: usize,
        lat_range: (usize, usize),
        lon_range: (usize, usize),
        lat_axis: usize,
        lon_axis: usize,
        ndim: usize,
    ) -> Option<f32> {
        let (lat_start, lat_end) = lat_range;
        let (lon_start, lon_end) = lon_range;

        // For small ranges, just use center point (faster)
        let lat_span = lat_end.saturating_sub(lat_start);
        let lon_span = lon_end.saturating_sub(lon_start);
        if lat_span <= 2 && lon_span <= 2 {
            let center_lat = (lat_start + lat_end) / 2;
            let center_lon = (lon_start + lon_end) / 2;
            return self.get_value_if_cached(
                variable_name,
                time_idx,
                center_lat,
                center_lon,
                lat_axis,
                lon_axis,
                ndim,
            );
        }

        // For larger ranges, subsample (max 4x4 = 16 samples for performance)
        const MAX_SAMPLES: usize = 4;
        let lat_step = (lat_span / MAX_SAMPLES).max(1);
        let lon_step = (lon_span / MAX_SAMPLES).max(1);

        let mut sum = 0.0f32;
        let mut count = 0usize;

        let mut lat_idx = lat_start;
        while lat_idx < lat_end {
            let mut lon_idx = lon_start;
            while lon_idx < lon_end {
                if let Some(v) = self.get_value_if_cached(
                    variable_name,
                    time_idx,
                    lat_idx,
                    lon_idx,
                    lat_axis,
                    lon_axis,
                    ndim,
                ) {
                    if v.is_finite() {
                        sum += v;
                        count += 1;
                    }
                }
                lon_idx += lon_step;
            }
            lat_idx += lat_step;
        }

        if count > 0 {
            Some(sum / count as f32)
        } else {
            // Fallback: try center point (handles sparse data)
            let center_lat = (lat_start + lat_end) / 2;
            let center_lon = (lon_start + lon_end) / 2;
            self.get_value_if_cached(
                variable_name,
                time_idx,
                center_lat,
                center_lon,
                lat_axis,
                lon_axis,
                ndim,
            )
        }
    }

    /// Load visible chunks that are not yet cached (loads all at once)
    pub fn load_visible_chunks(
        &mut self,
        variable_name: &str,
        time_idx: usize,
        chunks: &[(usize, usize)],
        array: &OpenArray,
        lat_axis: usize,
        lon_axis: usize,
        ndim: usize,
    ) {
        self.load_visible_chunks_limited(
            variable_name,
            time_idx,
            chunks,
            array,
            lat_axis,
            lon_axis,
            ndim,
            usize::MAX,
        );
    }

    /// Load visible chunks with a per-call limit for responsive UI
    pub fn load_visible_chunks_limited(
        &mut self,
        variable_name: &str,
        time_idx: usize,
        chunks: &[(usize, usize)],
        array: &OpenArray,
        lat_axis: usize,
        lon_axis: usize,
        ndim: usize,
        max_chunks: usize,
    ) {
        let shape = array.shape();
        let mut loaded = 0;
        let mut pending = 0;

        for &(chunk_lat, chunk_lon) in chunks {
            let key = ChunkKey::new(variable_name, time_idx, chunk_lat, chunk_lon);
            if self.cache.contains(&key) {
                continue;
            }

            // Count as pending
            pending += 1;

            if loaded >= max_chunks {
                continue; // Still count pending but don't load
            }

            let range = self.chunk_to_range(chunk_lat, chunk_lon);

            // Clamp ranges to actual array shape to prevent out-of-bounds
            let lat_end = (range.lat_end as u64).min(shape[lat_axis]);
            let lon_end = (range.lon_end as u64).min(shape[lon_axis]);

            // Skip if range is invalid (start >= end after clamping)
            if range.lat_start as u64 >= lat_end || range.lon_start as u64 >= lon_end {
                pending -= 1; // Not actually pending, just invalid
                continue;
            }

            // build ranges for all dimensions
            let ranges: Vec<Range<u64>> = (0..ndim)
                .map(|dim| {
                    if dim == lat_axis {
                        range.lat_start as u64..lat_end
                    } else if dim == lon_axis {
                        range.lon_start as u64..lon_end
                    } else {
                        // for non-spatial dims like time, load single slice
                        let idx = if dim == 0 { time_idx } else { 0 };
                        let idx = (idx as u64).min(shape[dim].saturating_sub(1));
                        idx..idx + 1
                    }
                })
                .collect();

            match array.retrieve_subset(&ranges) {
                Ok(data) => {
                    self.cache.put(key, CachedChunk { data, range });
                    loaded += 1;
                    pending -= 1; // No longer pending
                }
                Err(e) => {
                    eprintln!("Chunk load error at ({},{}): {}", chunk_lat, chunk_lon, e);
                    pending -= 1; // Failed, not pending
                }
            }
        }

        self.last_loaded_count = loaded;
        self.pending_chunks = pending;
    }

    /// Current cache size
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }
}

/// Range of array indices covered by a chunk
#[derive(Debug, Clone)]
pub struct ChunkRange {
    pub lat_start: usize,
    pub lat_end: usize,
    pub lon_start: usize,
    pub lon_end: usize,
}

impl ChunkRange {
    /// Convert global indices to local chunk indices
    pub fn to_local(&self, lat_idx: usize, lon_idx: usize) -> (usize, usize) {
        (lat_idx - self.lat_start, lon_idx - self.lon_start)
    }
}

/// Calculate which chunks are visible given viewport bounds in geo coordinates
pub fn visible_chunks(
    lat_min: f32,
    lat_max: f32,
    lon_min: f32,
    lon_max: f32,
    lat_data: &[f32],
    lon_data: &[f32],
    chunk_size_lat: usize,
    chunk_size_lon: usize,
) -> Vec<(usize, usize)> {
    // convert to array indices with bounds
    let lat_idx_a = find_nearest_index(lat_data, lat_min).unwrap_or(0);
    let lat_idx_b =
        find_nearest_index(lat_data, lat_max).unwrap_or(lat_data.len().saturating_sub(1));
    let lon_idx_a = find_nearest_index(lon_data, lon_min).unwrap_or(0);
    let lon_idx_b =
        find_nearest_index(lon_data, lon_max).unwrap_or(lon_data.len().saturating_sub(1));

    // ensure min <= max (handles descending coordinate arrays)
    let lat_idx_min = lat_idx_a.min(lat_idx_b);
    let lat_idx_max = lat_idx_a.max(lat_idx_b);
    let lon_idx_min = lon_idx_a.min(lon_idx_b);
    let lon_idx_max = lon_idx_a.max(lon_idx_b);

    // convert to chunk indices
    let chunk_lat_min = lat_idx_min / chunk_size_lat;
    let chunk_lat_max = lat_idx_max / chunk_size_lat;
    let chunk_lon_min = lon_idx_min / chunk_size_lon;
    let chunk_lon_max = lon_idx_max / chunk_size_lon;

    // collect all visible chunks
    let mut chunks = Vec::new();
    for chunk_lat in chunk_lat_min..=chunk_lat_max {
        for chunk_lon in chunk_lon_min..=chunk_lon_max {
            chunks.push((chunk_lat, chunk_lon));
        }
    }
    chunks
}

fn find_nearest_index(data: &[f32], value: f32) -> Option<usize> {
    if data.is_empty() {
        return None;
    }

    // determine if ascending or descending
    let ascending = data.len() < 2 || data[0] < data[data.len() - 1];

    let idx = if ascending {
        data.partition_point(|&x| x < value)
    } else {
        data.partition_point(|&x| x > value)
    };

    // find nearest between idx-1 and idx
    let idx = idx.min(data.len() - 1);
    if idx == 0 {
        return Some(0);
    }

    let prev = idx - 1;
    if (data[prev] - value).abs() < (data[idx] - value).abs() {
        Some(prev)
    } else {
        Some(idx)
    }
}
