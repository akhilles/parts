use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;
use tracing::debug;

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Cache miss")]
    Miss,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheMeta {
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub url: String,
    pub method: String,
    pub path_params: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug)]
pub struct CacheRequest<'a> {
    pub method: &'a str,
    pub base_url: &'a str,
    pub path: &'a str,
    pub path_params: &'a HashMap<String, String>,
    pub query_params: &'a HashMap<String, String>,
}

#[derive(Debug)]
pub struct ApiCache {
    cache_dir: PathBuf,
    default_ttl_seconds: u64,
}

impl ApiCache {
    pub fn new(cache_dir: PathBuf, default_ttl_seconds: u64) -> Self {
        Self {
            cache_dir,
            default_ttl_seconds,
        }
    }

    /// Build canonical cache path from request components
    fn build_cache_path(
        &self,
        method: &str,
        _base_url: &str,
        path: &str,
        path_params: &HashMap<String, String>,
        query_params: &HashMap<String, String>,
    ) -> PathBuf {
        let mut cache_path = self.cache_dir.clone();

        // Method directory
        cache_path.push(method.to_uppercase());

        // Path segments, with parameter substitution
        let mut resolved_path = path.to_string();
        for (key, value) in path_params {
            let placeholder = format!("{{{key}}}");
            resolved_path = resolved_path.replace(&placeholder, value);
        }

        // Remove leading slash and split into segments
        let path_segments: Vec<&str> = resolved_path
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        for segment in path_segments {
            cache_path.push(segment);
        }

        // Query parameters as final directory if present
        if !query_params.is_empty() {
            let mut query_pairs: Vec<(String, String)> = query_params
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            query_pairs.sort_by(|a, b| a.0.cmp(&b.0));

            let query_string = query_pairs
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
                .collect::<Vec<_>>()
                .join("&");

            // Use hash if query string is too long
            let dir_name = if query_string.len() > 100 {
                format!("_{:x}", md5::compute(&query_string))
            } else {
                format!("_{query_string}")
            };

            cache_path.push(dir_name);
        }

        cache_path
    }

    /// Check if cached response exists and is still valid
    pub fn get(
        &self,
        method: &str,
        base_url: &str,
        path: &str,
        path_params: &HashMap<String, String>,
        query_params: &HashMap<String, String>,
        force_refresh: bool,
    ) -> Result<String, CacheError> {
        if force_refresh {
            debug!("Cache lookup skipped due to force refresh");
            return Err(CacheError::Miss);
        }

        let cache_path = self.build_cache_path(method, base_url, path, path_params, query_params);
        let meta_file = cache_path.join("meta.json");
        let response_file = cache_path.join("response.json");

        if !meta_file.exists() || !response_file.exists() {
            debug!("Cache miss: files don't exist at {:?}", cache_path);
            return Err(CacheError::Miss);
        }

        // Check if cache is expired
        let meta_content = fs::read_to_string(&meta_file)?;
        let meta: CacheMeta = serde_json::from_str(&meta_content)?;

        let ttl = meta.ttl_seconds.unwrap_or(self.default_ttl_seconds);
        let age = chrono::Utc::now() - meta.fetched_at;

        if age.num_seconds() >= ttl as i64 {
            debug!("Cache expired: age={:?}, ttl={}s", age, ttl);
            return Err(CacheError::Miss);
        }

        let response_content = fs::read_to_string(&response_file)?;
        debug!("Cache hit at {:?}", cache_path);
        Ok(response_content)
    }

    /// Store response in cache
    pub fn put(
        &self,
        request: &CacheRequest,
        response: &str,
        ttl_seconds: Option<u64>,
    ) -> Result<(), CacheError> {
        let cache_path = self.build_cache_path(
            request.method,
            request.base_url,
            request.path,
            request.path_params,
            request.query_params,
        );

        // Create directory structure
        fs::create_dir_all(&cache_path)?;

        let meta = CacheMeta {
            fetched_at: chrono::Utc::now(),
            url: format!("{}{}", request.base_url, request.path),
            method: request.method.to_string(),
            path_params: request.path_params.clone(),
            query_params: request.query_params.clone(),
            ttl_seconds,
        };

        let meta_file = cache_path.join("meta.json");
        let response_file = cache_path.join("response.json");

        fs::write(&meta_file, serde_json::to_string_pretty(&meta)?)?;
        fs::write(&response_file, response)?;

        debug!("Cached response at {:?}", cache_path);
        Ok(())
    }
}
