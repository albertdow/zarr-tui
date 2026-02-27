use std::ops::Range;
use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use ndarray::ArrayD;
use object_store::ObjectStore;
use object_store::aws::AmazonS3Builder;
use object_store::prefix::PrefixStore;
use zarrs::array::{Array, DataType};
use zarrs::array_subset::ArraySubset;
use zarrs::storage::ReadableStorageTraits;
use zarrs_filesystem::FilesystemStore;
use zarrs_object_store::AsyncObjectStore;
use zarrs_storage::storage_adapter::async_to_sync::{
    AsyncToSyncBlockOn, AsyncToSyncStorageAdapter,
};

const AWS_DEFAULT_REGION: &str = "eu-west-2";

/// Read AWS credentials from `~/.aws/credentials` for the default or active profile
fn read_aws_credentials() -> Option<(String, String)> {
    let home = std::env::var("HOME").ok()?;
    let profile = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".to_string());
    let contents = std::fs::read_to_string(format!("{}/.aws/credentials", home)).ok()?;

    let mut in_profile = false;
    let mut key_id = None;
    let mut secret = None;
    let header = format!("[{}]", profile);

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == header {
            in_profile = true;
        } else if trimmed.starts_with('[') {
            if in_profile {
                break;
            }
        } else if in_profile && let Some((k, v)) = trimmed.split_once('=') {
            match k.trim() {
                "aws_access_key_id" => key_id = Some(v.trim().to_string()),
                "aws_secret_access_key" => secret = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }

    key_id.zip(secret)
}

/// Tokio runtime blocker for async-to-sync conversion
pub struct TokioBlockOn(tokio::runtime::Handle);

impl AsyncToSyncBlockOn for TokioBlockOn {
    fn block_on<F: core::future::Future>(&self, future: F) -> F::Output {
        // use block_in_place to allow blocking within async context
        tokio::task::block_in_place(|| self.0.block_on(future))
    }
}

pub type S3ObjectStore = PrefixStore<object_store::aws::AmazonS3>;
pub type S3AsyncStore = AsyncObjectStore<S3ObjectStore>;
pub type S3Store = AsyncToSyncStorageAdapter<S3AsyncStore, TokioBlockOn>;

/// Opened array without loaded data (for lazy chunk loading)
pub enum OpenArray {
    Filesystem(Arc<Array<FilesystemStore>>),
    S3(Arc<Array<S3Store>>),
}

impl OpenArray {
    /// Get array shape
    pub fn shape(&self) -> Vec<u64> {
        match self {
            Self::Filesystem(arr) => arr.shape().to_vec(),
            Self::S3(arr) => arr.shape().to_vec(),
        }
    }

    /// Get native chunk shape for given axes (returns None if irregular chunking)
    pub fn native_chunk_shape(&self, lat_axis: usize, lon_axis: usize) -> Option<(usize, usize)> {
        fn get_chunk_shape<S: ReadableStorageTraits + ?Sized + 'static>(
            arr: &Array<S>,
            lat_axis: usize,
            lon_axis: usize,
        ) -> Option<(usize, usize)> {
            let ndim = arr.shape().len();
            let chunk_indices: Vec<u64> = vec![0; ndim];
            let grid = arr.chunk_grid();
            let chunk_shape = grid.chunk_shape(&chunk_indices).ok()??;
            let lat_chunk = chunk_shape.get(lat_axis)?.get() as usize;
            let lon_chunk = chunk_shape.get(lon_axis)?.get() as usize;
            Some((lat_chunk, lon_chunk))
        }
        match self {
            Self::Filesystem(arr) => get_chunk_shape(arr.as_ref(), lat_axis, lon_axis),
            Self::S3(arr) => get_chunk_shape(arr.as_ref(), lat_axis, lon_axis),
        }
    }

    /// Retrieve a subset of the array as f32
    pub fn retrieve_subset(&self, ranges: &[Range<u64>]) -> Result<ArrayD<f32>> {
        let start: Vec<u64> = ranges.iter().map(|r| r.start).collect();
        let shape: Vec<u64> = ranges.iter().map(|r| r.end - r.start).collect();
        let subset = ArraySubset::new_with_start_shape(start, shape)?;

        match self {
            Self::Filesystem(arr) => retrieve_subset_as_f32(arr.as_ref(), &subset),
            Self::S3(arr) => retrieve_subset_as_f32(arr.as_ref(), &subset),
        }
    }

    /// Get lat/lon axis metadata
    pub fn meta(&self) -> Option<ArrayMeta> {
        match self {
            Self::Filesystem(arr) => ArrayMeta::from_array(arr.as_ref()),
            Self::S3(arr) => ArrayMeta::from_array(arr.as_ref()),
        }
    }
}

fn retrieve_subset_as_f32<S: ReadableStorageTraits + ?Sized + 'static>(
    array: &Array<S>,
    subset: &ArraySubset,
) -> Result<ArrayD<f32>> {
    let data: ArrayD<f32> = match array.data_type() {
        DataType::Float32 => array.retrieve_array_subset_ndarray(subset)?,
        DataType::Float64 => {
            let arr: ArrayD<f64> = array.retrieve_array_subset_ndarray(subset)?;
            arr.mapv(|v| v as f32)
        }
        DataType::Int32 => {
            let arr: ArrayD<i32> = array.retrieve_array_subset_ndarray(subset)?;
            arr.mapv(|v| v as f32)
        }
        DataType::Int64 => {
            let arr: ArrayD<i64> = array.retrieve_array_subset_ndarray(subset)?;
            arr.mapv(|v| v as f32)
        }
        DataType::UInt8 => {
            let arr: ArrayD<u8> = array.retrieve_array_subset_ndarray(subset)?;
            arr.mapv(|v| v as f32)
        }
        DataType::UInt16 => {
            let arr: ArrayD<u16> = array.retrieve_array_subset_ndarray(subset)?;
            arr.mapv(|v| v as f32)
        }
        dt => anyhow::bail!("Unsupported data type: {:?}", dt),
    };
    Ok(data)
}

/// Unified store supporting both local filesystem and S3
pub enum UnifiedStore {
    Filesystem(Arc<FilesystemStore>, String),
    S3 {
        store: Arc<S3Store>,
        object_store: Arc<dyn ObjectStore>,
        prefix: String,
    },
}

impl UnifiedStore {
    /// Create a store from a path (local or s3://)
    pub fn open(path: &str) -> Result<Self> {
        if path.starts_with("s3://") {
            Self::open_s3(path)
        } else {
            Self::open_filesystem(path)
        }
    }

    fn open_filesystem(path: &str) -> Result<Self> {
        let store = FilesystemStore::new(path)?;
        Ok(Self::Filesystem(Arc::new(store), path.to_string()))
    }

    fn open_s3(path: &str) -> Result<Self> {
        let url = url::Url::parse(path)?;
        let bucket = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("Missing bucket in S3 URL"))?;
        let prefix = url.path().trim_start_matches('/').to_string();

        let mut builder = AmazonS3Builder::from_env().with_bucket_name(bucket);

        // get region from env or default
        let region = std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|_| AWS_DEFAULT_REGION.to_string());
        builder = builder.with_region(&region);

        eprintln!("  S3 bucket: {}, region: {}", bucket, region);
        eprintln!(
            "  AWS_ACCESS_KEY_ID (env): {:?}",
            std::env::var("AWS_ACCESS_KEY_ID").ok()
        );
        eprintln!(
            "  AWS_SECRET_ACCESS_KEY (env): {}",
            if std::env::var("AWS_SECRET_ACCESS_KEY").is_ok() {
                "set"
            } else {
                "not set"
            }
        );
        eprintln!(
            "  AWS_PROFILE (env): {:?}",
            std::env::var("AWS_PROFILE").ok()
        );

        // try env vars first, then ~/.aws/credentials, then anonymous
        if std::env::var("AWS_ACCESS_KEY_ID").is_ok()
            && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok()
        {
            eprintln!("  Using credentials from environment variables");
        } else if let Some((key_id, secret)) = read_aws_credentials() {
            eprintln!("  Using credentials from ~/.aws/credentials");
            builder = builder
                .with_access_key_id(key_id)
                .with_secret_access_key(secret);
        } else {
            eprintln!("  No credentials found, trying anonymous access");
            builder = builder.with_skip_signature(true);
        }

        let s3 = builder.build()?;

        // wrap with prefix to handle zarr path within bucket
        let prefix_path = object_store::path::Path::from(prefix.as_str());
        let prefixed = PrefixStore::new(s3.clone(), prefix_path);
        let object_store: Arc<dyn ObjectStore> = Arc::new(s3);

        let async_store = AsyncObjectStore::new(prefixed);
        let handle = tokio::runtime::Handle::current();
        let store = AsyncToSyncStorageAdapter::new(Arc::new(async_store), TokioBlockOn(handle));

        Ok(Self::S3 {
            store: Arc::new(store),
            object_store,
            prefix,
        })
    }

    /// Discover all zarr arrays in the store
    pub async fn discover_arrays(&self) -> Result<Vec<String>> {
        match self {
            Self::Filesystem(_, path) => discover_arrays_filesystem(path),
            Self::S3 {
                object_store,
                prefix,
                ..
            } => discover_arrays_s3(object_store, prefix).await,
        }
    }

    /// Load a 1D coordinate array as f32
    pub fn load_coord_array(&self, array_path: &str) -> Result<Vec<f32>> {
        match self {
            Self::Filesystem(store, _) => load_coord_array_impl(store.clone(), array_path),
            Self::S3 { store, .. } => load_coord_array_impl(store.clone(), array_path),
        }
    }

    /// Open an array without loading data (for lazy chunk loading)
    pub fn open_array(&self, array_path: &str) -> Result<OpenArray> {
        match self {
            Self::Filesystem(store, _) => {
                let array = Array::open(store.clone(), array_path)?;
                Ok(OpenArray::Filesystem(Arc::new(array)))
            }
            Self::S3 { store, .. } => {
                let array = Array::open(store.clone(), array_path)?;
                Ok(OpenArray::S3(Arc::new(array)))
            }
        }
    }
}

/// Array axis metadata
pub struct ArrayMeta {
    pub lon_axis: usize,
    pub lat_axis: usize,
}

impl ArrayMeta {
    fn from_array<TStorage: ReadableStorageTraits + ?Sized>(
        array: &Array<TStorage>,
    ) -> Option<Self> {
        // Try dimension_names first, but only if ALL names are present
        let dim_names: Vec<String> = array
            .dimension_names()
            .as_ref()
            .and_then(|names| {
                let collected: Vec<String> = names.iter().filter_map(|n| n.clone()).collect();
                // Only use if we got all dimension names
                if collected.len() == names.len() {
                    Some(collected)
                } else {
                    None
                }
            })
            .or_else(|| {
                // zarr v2: xarray stores dimension names in _ARRAY_DIMENSIONS attr
                array
                    .attributes()
                    .get("_ARRAY_DIMENSIONS")
                    .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
            })?;

        let lon_axis = dim_names
            .iter()
            .position(|s| matches!(s.as_str(), "lon" | "longitude" | "x"))?;
        let lat_axis = dim_names
            .iter()
            .position(|s| matches!(s.as_str(), "lat" | "latitude" | "y"))?;

        Some(Self { lon_axis, lat_axis })
    }
}

fn discover_arrays_filesystem(base_path: &str) -> Result<Vec<String>> {
    use std::fs;

    let base = std::path::Path::new(base_path);
    let mut arrays = Vec::new();

    for entry in fs::read_dir(base)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if name.starts_with('.') || name == "__pycache__" {
                continue;
            }

            let is_zarr_array = path.join(".zarray").exists() || path.join("zarr.json").exists();

            if is_zarr_array {
                arrays.push(format!("/{}", name));
            }
        }
    }

    arrays.sort();
    Ok(arrays)
}

async fn discover_arrays_s3(
    object_store: &Arc<dyn ObjectStore>,
    prefix: &str,
) -> Result<Vec<String>> {
    use std::collections::HashSet;

    let list_prefix = if prefix.is_empty() {
        None
    } else {
        Some(object_store::path::Path::from(prefix))
    };

    let mut stream = object_store.list(list_prefix.as_ref());
    let mut array_dirs: HashSet<String> = HashSet::new();

    while let Some(item) = stream.next().await {
        let meta = item?;
        let path_str = meta.location.to_string();

        // look for .zarray or zarr.json files
        if path_str.ends_with(".zarray") || path_str.ends_with("zarr.json") {
            // extract the array name (parent directory)
            let rel_path = if prefix.is_empty() {
                path_str
            } else {
                path_str
                    .strip_prefix(prefix)
                    .unwrap_or(&path_str)
                    .trim_start_matches('/')
                    .to_string()
            };

            // get the first path component (array name)
            if let Some(array_name) = rel_path.split('/').next()
                && !array_name.is_empty()
                && !array_name.starts_with('.')
            {
                array_dirs.insert(format!("/{}", array_name));
            }
        }
    }

    let mut arrays: Vec<String> = array_dirs.into_iter().collect();
    arrays.sort();
    Ok(arrays)
}

fn load_coord_array_impl<S: ReadableStorageTraits + ?Sized + 'static>(
    store: Arc<S>,
    path: &str,
) -> Result<Vec<f32>> {
    let array = Array::open(store, path)?;
    let shape = array.shape().to_vec();
    let subset = ArraySubset::new_with_shape(shape);

    let data: Vec<f32> = match array.data_type() {
        DataType::Float32 => {
            let arr: ArrayD<f32> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.iter().copied().collect()
        }
        DataType::Float64 => {
            let arr: ArrayD<f64> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.iter().map(|&v| v as f32).collect()
        }
        DataType::Int32 => {
            let arr: ArrayD<i32> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.iter().map(|&v| v as f32).collect()
        }
        DataType::Int64 => {
            let arr: ArrayD<i64> = array.retrieve_array_subset_ndarray(&subset)?;
            arr.iter().map(|&v| v as f32).collect()
        }
        dt => anyhow::bail!("Unsupported coordinate data type: {:?}", dt),
    };

    Ok(data)
}
