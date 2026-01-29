//! Index manager - Core indexing and search logic

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use bincode::Options;
use encoding_rs::{GB18030, GBK, UTF_8, WINDOWS_1252};
use futures::stream::{FuturesUnordered, StreamExt};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use rayon::prelude::*;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{error, info, warn};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::config::Config;
use crate::http_logger::{self, HttpRequestLog, HttpResponseLog};
use crate::strategy::{AdaptiveStrategy, ErrorType};
use crate::utils::path_normalizer::{normalize_path, normalize_relative_path, RuntimeEnv};
use crate::utils::project_detector::get_index_file_path;

/// Maximum blob size in bytes (128KB, aligned with official augment.mjs)
const MAX_BLOB_SIZE: usize = 128 * 1024;

/// Maximum batch size in bytes (1MB, aligned with official augment.mjs)
const MAX_BATCH_SIZE: usize = 1024 * 1024;

/// Maximum index size in bytes (256MB)
const MAX_INDEX_BYTES: u64 = 256 * 1024 * 1024;

/// Current index format version
const CURRENT_INDEX_VERSION: u32 = 2;

/// User-Agent header value (matches augment.mjs format: augment.cli/{version}/{mode})
const USER_AGENT: &str = "augment.cli/0.12.0/mcp";

/// Generate a unique request ID
fn generate_request_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a session ID (persistent for the lifetime of the process)
fn get_session_id() -> &'static str {
    use std::sync::OnceLock;
    static SESSION_ID: OnceLock<String> = OnceLock::new();
    SESSION_ID.get_or_init(|| Uuid::new_v4().to_string())
}

/// Blob data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blob {
    pub path: String,
    pub content: String,
}

/// Index data structure (v2 format with mtime support)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexData {
    /// Index format version
    pub version: u32,
    /// Configuration fingerprint for detecting chunking config changes
    pub config_hash: String,
    /// File entries, key is normalized relative path (forward slashes)
    pub entries: HashMap<String, FileEntry>,
}

impl IndexData {
    /// Get all blob hashes from all entries
    pub fn get_all_blob_hashes(&self) -> Vec<String> {
        self.entries
            .values()
            .flat_map(|e| e.blob_hashes.iter().cloned())
            .collect()
    }
}

/// Single file index entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Modification time (seconds since UNIX epoch)
    pub mtime_secs: u64,
    /// Modification time (nanoseconds part, 0-999_999_999)
    pub mtime_nanos: u32,
    /// File size in bytes
    pub size: u64,
    /// Blob hashes produced by this file (large files may have multiple chunks)
    pub blob_hashes: Vec<String>,
}

/// Result of processing a single file
#[derive(Debug)]
struct ProcessedFile {
    /// Normalized relative path
    rel_path: String,
    /// Processing result
    result: ProcessedResult,
}

/// Processing result variants
#[derive(Debug)]
enum ProcessedResult {
    /// Cache hit - reuse existing entry
    Cached { entry: FileEntry },
    /// New or modified file - contains blobs to upload
    New { blobs: Vec<Blob>, entry: FileEntry },
}

/// Index result
#[derive(Debug, Clone)]
pub struct IndexResult {
    pub status: String,
    pub message: String,
    pub stats: Option<IndexStats>,
}

#[derive(Debug, Clone)]
pub struct IndexStats {
    pub total_blobs: usize,
    pub existing_blobs: usize,
    pub new_blobs: usize,
    pub failed_batches: Option<usize>,
}

/// Batch upload request
#[derive(Debug, Serialize)]
struct BatchUploadRequest {
    blobs: Vec<Blob>,
}

/// Batch upload response
#[derive(Debug, Deserialize)]
struct BatchUploadResponse {
    blob_names: Vec<String>,
}

/// Result of a single batch upload attempt
#[derive(Debug)]
struct BatchUploadResult {
    blob_names: Vec<String>,
    latency_ms: u64,
    error_type: Option<ErrorType>,
    success: bool,
}

/// Search request payload
#[derive(Debug, Serialize)]
struct SearchRequest {
    information_request: String,
    blobs: BlobsPayload,
    dialog: Vec<serde_json::Value>,
    max_output_length: i32,
    disable_codebase_retrieval: bool,
    enable_commit_retrieval: bool,
}

#[derive(Debug, Serialize)]
struct BlobsPayload {
    checkpoint_id: Option<String>,
    added_blobs: Vec<String>,
    deleted_blobs: Vec<String>,
}

/// Search response
#[derive(Debug, Deserialize)]
struct SearchResponse {
    formatted_retrieval: Option<String>,
}

/// Calculate configuration fingerprint for detecting index-affecting config changes
///
/// Note: Currently only max_lines_per_blob affects blob splitting and hash calculation.
/// If new config options affecting indexing are added, they must be included here.
fn calculate_config_hash(max_lines_per_blob: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"v1:");
    hasher.update(max_lines_per_blob.to_le_bytes());
    hex::encode(&hasher.finalize()[..8])
}

/// Index manager
pub struct IndexManager {
    project_root: PathBuf,
    base_url: String,
    token: String,
    text_extensions: HashSet<String>,
    text_filenames: HashSet<String>,
    max_lines_per_blob: usize,
    compiled_patterns: Vec<(String, Option<Regex>)>,
    index_file_path: PathBuf,
    client: Client,
    runtime_env: RuntimeEnv,
    config_hash: String,
    retrieval_timeout_secs: u64,
    no_adaptive: bool,
    cli_overrides: crate::config::CliOverrides,
}

impl IndexManager {
    pub fn new(config: Arc<Config>, project_root: PathBuf) -> Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;

        // Detect runtime environment for WSL support
        let runtime_env = RuntimeEnv::detect();

        // Normalize project root path
        let normalized = normalize_path(&project_root, runtime_env);
        let project_root = normalized.local;

        let index_file_path = get_index_file_path(&project_root);

        // Precompile exclude patterns to regex
        let compiled_patterns: Vec<(String, Option<Regex>)> = config
            .exclude_patterns
            .iter()
            .map(|pattern| {
                let regex_pattern = pattern
                    .replace('.', "\\.")
                    .replace('*', ".*")
                    .replace('?', ".");
                let regex = Regex::new(&format!("^{}$", regex_pattern)).ok();
                (pattern.clone(), regex)
            })
            .collect();

        let config_hash = calculate_config_hash(config.max_lines_per_blob);

        Ok(Self {
            project_root,
            base_url: config.base_url.clone(),
            token: config.token.clone(),
            text_extensions: config.text_extensions.clone(),
            text_filenames: config.text_filenames.clone(),
            max_lines_per_blob: config.max_lines_per_blob,
            compiled_patterns,
            index_file_path,
            client,
            runtime_env,
            config_hash,
            retrieval_timeout_secs: config.retrieval_timeout_secs,
            no_adaptive: config.no_adaptive,
            cli_overrides: config.cli_overrides.clone(),
        })
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the token
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Get the project root
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the runtime environment
    pub fn runtime_env(&self) -> RuntimeEnv {
        self.runtime_env
    }

    /// Get the config hash
    pub fn config_hash(&self) -> &str {
        &self.config_hash
    }

    /// Load gitignore patterns
    fn load_gitignore(&self) -> Option<Gitignore> {
        let gitignore_path = self.project_root.join(".gitignore");
        if !gitignore_path.exists() {
            return None;
        }

        let mut builder = GitignoreBuilder::new(&self.project_root);
        // Log warning if gitignore has errors, but continue with valid patterns
        if let Some(err) = builder.add(&gitignore_path) {
            warn!(
                "Error parsing .gitignore (continuing with valid patterns): {}",
                err
            );
        }

        builder.build().ok()
    }

    /// Check if a path should be excluded
    /// `is_dir` parameter avoids extra filesystem stat calls when available from DirEntry
    fn should_exclude(&self, path: &Path, is_dir: bool, gitignore: Option<&Gitignore>) -> bool {
        let relative_path = match path.strip_prefix(&self.project_root) {
            Ok(p) => p,
            Err(_) => {
                // Fail-closed: if we can't determine the relative path, exclude the file for safety
                // This can happen due to path normalization issues (e.g., Windows \\?\ prefixes)
                warn!(
                    "Path prefix mismatch, excluding for safety: {:?} vs {:?}",
                    path, self.project_root
                );
                return true;
            }
        };

        let path_str = normalize_relative_path(&relative_path.to_string_lossy());

        // Check gitignore
        if let Some(gi) = gitignore {
            if gi.matched(&path_str, is_dir).is_ignore() {
                return true;
            }
        }

        // Check exclude patterns using precompiled regexes
        let path_parts: Vec<&str> = path_str.split('/').collect();
        for (pattern, compiled_regex) in &self.compiled_patterns {
            if let Some(regex) = compiled_regex {
                // Check each path component
                for part in &path_parts {
                    if regex.is_match(part) {
                        return true;
                    }
                }
                // Check full path
                if regex.is_match(&path_str) {
                    return true;
                }
            } else {
                // Fallback to string matching if regex failed to compile
                for part in &path_parts {
                    if *part == pattern {
                        return true;
                    }
                }
                if path_str == *pattern {
                    return true;
                }
            }
        }

        false
    }

    /// Simple pattern matching (supports * and ?) - kept for tests
    pub fn match_pattern(&self, s: &str, pattern: &str) -> bool {
        let regex_pattern = pattern
            .replace('.', "\\.")
            .replace('*', ".*")
            .replace('?', ".");
        if let Ok(regex) = Regex::new(&format!("^{}$", regex_pattern)) {
            regex.is_match(s)
        } else {
            false
        }
    }

    /// Load index data from file (bincode format)
    pub fn load_index(&self) -> IndexData {
        if !self.index_file_path.exists() {
            return IndexData::default();
        }

        let metadata = match fs::metadata(&self.index_file_path) {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to stat index file: {}", e);
                return IndexData::default();
            }
        };

        if metadata.len() > MAX_INDEX_BYTES {
            warn!(
                "Index file too large ({} bytes), rebuilding",
                metadata.len()
            );
            return IndexData::default();
        }

        let bytes = match fs::read(&self.index_file_path) {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to read index file: {}", e);
                return IndexData::default();
            }
        };

        let options = bincode::DefaultOptions::new().with_limit(bytes.len() as u64);
        match options.deserialize::<IndexData>(&bytes) {
            Ok(data) => {
                if data.version == CURRENT_INDEX_VERSION && data.config_hash == self.config_hash {
                    return data;
                }
                info!(
                    "Index version/config mismatch (v{} vs v{}, config {} vs {}), rebuilding",
                    data.version, CURRENT_INDEX_VERSION, data.config_hash, self.config_hash
                );
                IndexData::default()
            }
            Err(e) => {
                warn!("Failed to deserialize index: {}, rebuilding", e);
                IndexData::default()
            }
        }
    }

    /// Save index data to file (atomic write, bincode format)
    pub fn save_index(&self, data: &IndexData) -> Result<()> {
        let options = bincode::DefaultOptions::new().with_limit(MAX_INDEX_BYTES);
        let bytes = options.serialize(data)?;
        let tmp_path = self.index_file_path.with_extension("bin.tmp");

        // Write to temporary file
        fs::write(&tmp_path, &bytes)?;

        // Atomic rename (on Windows, need to remove target first)
        #[cfg(windows)]
        if self.index_file_path.exists() {
            fs::remove_file(&self.index_file_path)?;
        }

        fs::rename(&tmp_path, &self.index_file_path)?;
        Ok(())
    }

    /// Read file with encoding detection (avoids updating file access time on Windows)
    fn read_file_with_encoding(path: &Path) -> Result<String> {
        let bytes = Self::read_file_bytes(path)?;

        // Try different encodings
        let encodings = [UTF_8, GBK, GB18030, WINDOWS_1252];

        for encoding in encodings {
            let (content, _, had_errors) = encoding.decode(&bytes);
            if !had_errors {
                let content_str = content.to_string();
                // Check for replacement characters
                let replacement_count = content_str.matches('\u{FFFD}').count();
                let threshold = if content_str.len() < 100 {
                    5
                } else {
                    (content_str.len() as f64 * 0.05) as usize
                };

                if replacement_count <= threshold {
                    return Ok(content_str);
                }
            }
        }

        // Fallback to UTF-8 with lossy conversion
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    /// Decode bytes with encoding detection (for use when bytes are already read)
    fn decode_bytes_with_encoding(bytes: &[u8]) -> Result<String> {
        let encodings = [UTF_8, GBK, GB18030, WINDOWS_1252];

        for encoding in encodings {
            let (content, _, had_errors) = encoding.decode(bytes);
            if !had_errors {
                let content_str = content.to_string();
                let replacement_count = content_str.matches('\u{FFFD}').count();
                let threshold = if content_str.len() < 100 {
                    5
                } else {
                    (content_str.len() as f64 * 0.05) as usize
                };

                if replacement_count <= threshold {
                    return Ok(content_str);
                }
            }
        }

        Ok(String::from_utf8_lossy(bytes).to_string())
    }

    /// Read file bytes
    fn read_file_bytes(path: &Path) -> Result<Vec<u8>> {
        Ok(fs::read(path)?)
    }

    /// Calculate blob name (SHA-256 hash)
    pub fn calculate_blob_name(path: &str, content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(path.as_bytes());
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Sanitize content by removing problematic characters
    pub fn sanitize_content(content: &str) -> String {
        content
            .chars()
            .filter(|c| {
                // Keep printable characters, newlines, carriage returns, and tabs
                !matches!(*c, '\x00'..='\x08' | '\x0B' | '\x0C' | '\x0E'..='\x1F' | '\x7F')
            })
            .collect()
    }

    /// Check if content appears to be binary
    pub fn is_binary_content(content: &str) -> bool {
        let total_chars = content.chars().count();
        if total_chars == 0 {
            return false;
        }
        let non_printable: usize = content
            .chars()
            .filter(|c| matches!(*c, '\x00'..='\x08' | '\x0E'..='\x1F' | '\x7F'))
            .count();
        non_printable > total_chars / 10 // More than 10% non-printable
    }

    /// Split file content into blobs
    pub fn split_file_content(&self, file_path: &str, content: &str) -> Vec<Blob> {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // Guard against zero max_lines_per_blob to prevent div_ceil panic
        let max_lines = if self.max_lines_per_blob == 0 {
            800 // Use default value
        } else {
            self.max_lines_per_blob
        };

        if total_lines <= max_lines {
            return vec![Blob {
                path: file_path.to_string(),
                content: content.to_string(),
            }];
        }

        let num_chunks = total_lines.div_ceil(max_lines);
        let mut blobs = Vec::new();

        for chunk_idx in 0..num_chunks {
            let start_line = chunk_idx * max_lines;
            let end_line = (start_line + max_lines).min(total_lines);
            let chunk_lines: Vec<&str> = lines[start_line..end_line].to_vec();
            let chunk_content = chunk_lines.join("\n");
            let chunk_path = format!("{}#chunk{}of{}", file_path, chunk_idx + 1, num_chunks);

            blobs.push(Blob {
                path: chunk_path,
                content: chunk_content,
            });
        }

        blobs
    }

    /// Collect all text files
    pub fn collect_files(&self) -> Result<Vec<Blob>> {
        let mut blobs = Vec::new();
        let gitignore = self.load_gitignore();

        for entry in WalkDir::new(&self.project_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                !self.should_exclude(e.path(), e.file_type().is_dir(), gitignore.as_ref())
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("Failed to access entry during directory walk: {}", e);
                    continue;
                }
            };

            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();

            // Check if file should be included based on extension or filename
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{}", e.to_lowercase()))
                .unwrap_or_default();

            let is_known_filename = self.text_filenames.contains(filename);
            let is_known_extension = !ext.is_empty() && self.text_extensions.contains(&ext);

            if !is_known_filename && !is_known_extension {
                continue;
            }

            // Check file size before reading to avoid memory spikes
            match fs::metadata(path) {
                Ok(metadata) => {
                    if metadata.len() > MAX_BLOB_SIZE as u64 {
                        let relative_path = path
                            .strip_prefix(&self.project_root)
                            .unwrap_or(path)
                            .to_string_lossy();
                        warn!(
                            "Skipping large file (pre-check): {} ({}KB)",
                            relative_path,
                            metadata.len() / 1024
                        );
                        continue;
                    }
                }
                Err(e) => {
                    warn!("Failed to get metadata for {:?}: {}, skipping", path, e);
                    continue;
                }
            }

            // Read and process file
            let content = match Self::read_file_with_encoding(path) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to read file {:?}: {}", path, e);
                    continue;
                }
            };

            // Skip binary files
            if Self::is_binary_content(&content) {
                continue;
            }

            // Sanitize content
            let clean_content = Self::sanitize_content(&content);

            // Skip too large files
            if clean_content.len() > MAX_BLOB_SIZE {
                let relative_path = path
                    .strip_prefix(&self.project_root)
                    .unwrap_or(path)
                    .to_string_lossy();
                warn!(
                    "Skipping large file: {} ({}KB)",
                    relative_path,
                    clean_content.len() / 1024
                );
                continue;
            }

            let relative_path = normalize_relative_path(
                &path
                    .strip_prefix(&self.project_root)
                    .unwrap_or(path)
                    .to_string_lossy(),
            );

            let file_blobs = self.split_file_content(&relative_path, &clean_content);
            blobs.extend(file_blobs);
        }

        Ok(blobs)
    }

    /// Build batches that respect both count and size limits
    fn build_batches(&self, blobs: Vec<Blob>, max_blobs_per_batch: usize) -> Vec<Vec<Blob>> {
        let max_blobs_per_batch = max_blobs_per_batch.max(1);
        let mut batches = Vec::new();
        let mut current = Vec::new();
        let mut current_size = 0usize;

        for blob in blobs {
            let blob_size = blob.content.len() + blob.path.len();
            let would_exceed_size = current_size + blob_size > MAX_BATCH_SIZE;
            let would_exceed_count = current.len() >= max_blobs_per_batch;

            if !current.is_empty() && (would_exceed_size || would_exceed_count) {
                batches.push(current);
                current = Vec::new();
                current_size = 0;
            }

            current_size += blob_size;
            current.push(blob);
        }

        if !current.is_empty() {
            batches.push(current);
        }

        batches
    }

    /// Upload blobs with adaptive strategy
    async fn upload_blobs_adaptive(
        &self,
        blobs: Vec<Blob>,
        strategy: &mut AdaptiveStrategy,
    ) -> (Vec<String>, usize) {
        let batches = self.build_batches(blobs, strategy.batch_size());
        let total_batches = batches.len();
        let mut uploaded_blob_names: Vec<String> = Vec::new();
        let mut failed_batch_count: usize = 0;

        info!(
            "Uploading {} batches (adaptive: concurrency={}, timeout={}s)",
            total_batches,
            strategy.concurrency(),
            strategy.timeout_ms() / 1000
        );

        // Use a queue for pending batches and FuturesUnordered for active tasks
        let mut batch_queue: VecDeque<(usize, Vec<Blob>)> =
            batches.into_iter().enumerate().collect();
        let mut active_tasks = FuturesUnordered::new();

        // Helper to spawn a task
        let spawn_task = |index: usize,
                          batch: Vec<Blob>,
                          timeout_ms: u64,
                          client: Client,
                          base_url: String,
                          token: String,
                          project_root: PathBuf| {
            async move {
                let result = Self::upload_batch_internal(
                    &client,
                    &base_url,
                    &token,
                    &project_root,
                    &batch,
                    timeout_ms,
                )
                .await;
                (index, result)
            }
        };

        // Loop until all batches are processed
        while !batch_queue.is_empty() || !active_tasks.is_empty() {
            // 1. Fill active tasks up to current concurrency limit
            let current_concurrency = strategy.concurrency();
            while active_tasks.len() < current_concurrency && !batch_queue.is_empty() {
                if let Some((i, batch)) = batch_queue.pop_front() {
                    info!("Starting batch {}/{}...", i + 1, total_batches);
                    active_tasks.push(spawn_task(
                        i,
                        batch,
                        strategy.timeout_ms(),
                        self.client.clone(),
                        self.base_url.clone(),
                        self.token.clone(),
                        self.project_root.clone(),
                    ));
                }
            }

            // 2. Wait for the next task to complete
            if let Some((i, result)) = active_tasks.next().await {
                // 3. Record outcome and adjust strategy
                strategy.record_outcome(result.success, result.latency_ms, result.error_type);

                if result.success {
                    uploaded_blob_names.extend(result.blob_names);
                } else {
                    error!("Batch {} upload failed", i + 1);
                    failed_batch_count += 1;
                }
            }
        }

        (uploaded_blob_names, failed_batch_count)
    }

    /// Internal batch upload with metrics (static method)
    async fn upload_batch_internal(
        client: &Client,
        base_url: &str,
        token: &str,
        project_root: &Path,
        blobs: &[Blob],
        timeout_ms: u64,
    ) -> BatchUploadResult {
        let batch_size: usize = blobs.iter().map(|b| b.content.len() + b.path.len()).sum();
        if batch_size > MAX_BATCH_SIZE {
            return BatchUploadResult {
                blob_names: Vec::new(),
                latency_ms: 0,
                error_type: Some(ErrorType::ClientError),
                success: false,
            };
        }

        let url = format!("{}/batch-upload", base_url);
        let request = BatchUploadRequest {
            blobs: blobs.to_vec(),
        };

        let request_body = if http_logger::is_enabled() {
            serde_json::to_string(&request).ok()
        } else {
            None
        };

        let mut last_error_type = None;
        let mut total_latency_ms = 0u64;
        let max_retries = 3;

        for attempt in 0..max_retries {
            let request_id = generate_request_id();
            let start_time = Instant::now();

            let http_request_log = if http_logger::is_enabled() {
                Some(HttpRequestLog {
                    method: "POST".to_string(),
                    url: url.clone(),
                    headers: http_logger::extract_headers_from_builder(
                        "application/json",
                        USER_AGENT,
                        &request_id,
                        get_session_id(),
                        token,
                    ),
                    body: request_body.clone(),
                })
            } else {
                None
            };

            let result = client
                .post(&url)
                .timeout(Duration::from_millis(timeout_ms))
                .header("Content-Type", "application/json")
                .header("User-Agent", USER_AGENT)
                .header("x-request-id", &request_id)
                .header("x-request-session-id", get_session_id())
                .header("Authorization", format!("Bearer {}", token))
                .json(&request)
                .send()
                .await;

            let duration_ms = start_time.elapsed().as_millis() as u64;
            total_latency_ms += duration_ms;

            match result {
                Ok(response) => {
                    let status = response.status();
                    let response_headers = if http_logger::is_enabled() {
                        http_logger::extract_response_headers(&response)
                    } else {
                        Vec::new()
                    };

                    // Extract Retry-After header before consuming response
                    let retry_after = response
                        .headers()
                        .get("Retry-After")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(1);

                    if status == 401 || status == 403 {
                        if let Some(ref req_log) = http_request_log {
                            let response_log = HttpResponseLog {
                                status: status.as_u16(),
                                headers: response_headers,
                                body: Some("Auth error".to_string()),
                            };
                            http_logger::log_request(
                                Some(project_root),
                                req_log,
                                Some(&response_log),
                                duration_ms,
                                None,
                            );
                        }
                        return BatchUploadResult {
                            blob_names: Vec::new(),
                            latency_ms: total_latency_ms,
                            error_type: Some(ErrorType::ClientError),
                            success: false,
                        };
                    }

                    if status == 400 {
                        let text = response.text().await.unwrap_or_default();
                        if let Some(ref req_log) = http_request_log {
                            let response_log = HttpResponseLog {
                                status: 400,
                                headers: response_headers,
                                body: Some(text),
                            };
                            http_logger::log_request(
                                Some(project_root),
                                req_log,
                                Some(&response_log),
                                duration_ms,
                                None,
                            );
                        }
                        return BatchUploadResult {
                            blob_names: Vec::new(),
                            latency_ms: total_latency_ms,
                            error_type: Some(ErrorType::ClientError),
                            success: false,
                        };
                    }

                    if status.is_success() {
                        let body_text = response.text().await.unwrap_or_default();
                        if let Some(ref req_log) = http_request_log {
                            let response_log = HttpResponseLog {
                                status: status.as_u16(),
                                headers: response_headers.clone(),
                                body: Some(body_text.clone()),
                            };
                            http_logger::log_request(
                                Some(project_root),
                                req_log,
                                Some(&response_log),
                                duration_ms,
                                None,
                            );
                        }
                        if let Ok(resp) = serde_json::from_str::<BatchUploadResponse>(&body_text) {
                            return BatchUploadResult {
                                blob_names: resp.blob_names,
                                latency_ms: total_latency_ms,
                                error_type: None,
                                success: true,
                            };
                        }
                    }

                    if status == 429 && attempt < max_retries - 1 {
                        let wait_time = retry_after * 1000;
                        if let Some(ref req_log) = http_request_log {
                            let response_log = HttpResponseLog {
                                status: status.as_u16(),
                                headers: response_headers.clone(),
                                body: None,
                            };
                            http_logger::log_request(
                                Some(project_root),
                                req_log,
                                Some(&response_log),
                                duration_ms,
                                Some(&format!("Rate limited, retrying in {}ms", wait_time)),
                            );
                        }
                        warn!(
                            "Rate limited (attempt {}/{}), retrying in {}ms...",
                            attempt + 1,
                            max_retries,
                            wait_time
                        );
                        last_error_type = Some(ErrorType::RateLimit);
                        tokio::time::sleep(Duration::from_millis(wait_time)).await;
                        continue;
                    }

                    if status.is_server_error() && attempt < max_retries - 1 {
                        let wait_time = 1000 * (1 << attempt);
                        if let Some(ref req_log) = http_request_log {
                            let response_log = HttpResponseLog {
                                status: status.as_u16(),
                                headers: response_headers.clone(),
                                body: None,
                            };
                            http_logger::log_request(
                                Some(project_root),
                                req_log,
                                Some(&response_log),
                                duration_ms,
                                Some(&format!("Server error, retrying in {}ms", wait_time)),
                            );
                        }
                        warn!(
                            "Server error (attempt {}/{}), retrying in {}ms...",
                            attempt + 1,
                            max_retries,
                            wait_time
                        );
                        last_error_type = Some(ErrorType::ServerError);
                        tokio::time::sleep(Duration::from_millis(wait_time)).await;
                        continue;
                    }

                    if let Some(ref req_log) = http_request_log {
                        let response_log = HttpResponseLog {
                            status: status.as_u16(),
                            headers: response_headers,
                            body: None,
                        };
                        http_logger::log_request(
                            Some(project_root),
                            req_log,
                            Some(&response_log),
                            duration_ms,
                            Some(&format!("HTTP error: {}", status)),
                        );
                    }

                    return BatchUploadResult {
                        blob_names: Vec::new(),
                        latency_ms: total_latency_ms,
                        error_type: if status == 429 {
                            Some(ErrorType::RateLimit)
                        } else if status.is_server_error() {
                            Some(ErrorType::ServerError)
                        } else {
                            Some(ErrorType::ClientError)
                        },
                        success: false,
                    };
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    let is_timeout =
                        error_msg.contains("timeout") || error_msg.contains("timed out");

                    if let Some(ref req_log) = http_request_log {
                        http_logger::log_request(
                            Some(project_root),
                            req_log,
                            None,
                            duration_ms,
                            Some(&error_msg),
                        );
                    }

                    if attempt < max_retries - 1 {
                        let wait_time = 1000 * (1 << attempt);
                        warn!(
                            "Request failed (attempt {}/{}): {}, retrying in {}ms...",
                            attempt + 1,
                            max_retries,
                            &error_msg,
                            wait_time
                        );
                        tokio::time::sleep(Duration::from_millis(wait_time)).await;
                    }

                    last_error_type = Some(if is_timeout {
                        ErrorType::Timeout
                    } else {
                        ErrorType::NetworkError
                    });
                }
            }
        }

        BatchUploadResult {
            blob_names: Vec::new(),
            latency_ms: total_latency_ms,
            error_type: last_error_type,
            success: false,
        }
    }

    /// Index the project with mtime caching and parallel processing
    pub async fn index_project(&self) -> IndexResult {
        info!("Starting project indexing: {:?}", self.project_root);

        // Step 1: Collect file paths (via spawn_blocking to avoid blocking async runtime)
        info!("Scanning files...");
        let project_root_scan = self.project_root.clone();
        let text_extensions_scan = self.text_extensions.clone();
        let text_filenames_scan = self.text_filenames.clone();
        let compiled_patterns_scan = self.compiled_patterns.clone();

        let file_paths = tokio::task::spawn_blocking(move || {
            collect_file_paths_standalone(
                &project_root_scan,
                &text_extensions_scan,
                &text_filenames_scan,
                &compiled_patterns_scan,
            )
        })
        .await
        .unwrap_or_else(|e| {
            error!("File scanning failed: {}", e);
            Vec::new()
        });

        if file_paths.is_empty() {
            warn!("No indexable text files found");
            return IndexResult {
                status: "error".to_string(),
                message: "No text files found in project".to_string(),
                stats: None,
            };
        }

        info!("Found {} files to process", file_paths.len());

        // Step 2: Load old index
        let old_index = self.load_index();

        // Step 3: Process files in parallel using rayon (via spawn_blocking)
        let old_index_arc = Arc::new(old_index);
        let project_root = self.project_root.clone();
        let text_extensions = self.text_extensions.clone();
        let text_filenames = self.text_filenames.clone();
        let compiled_patterns = self.compiled_patterns.clone();
        let max_lines_per_blob = self.max_lines_per_blob;

        let results: Vec<ProcessedFile> = tokio::task::spawn_blocking(move || {
            file_paths
                .par_iter()
                .filter_map(|path| {
                    // We need to inline the processing logic here since we can't capture &self
                    process_file_standalone(
                        path,
                        &old_index_arc,
                        &project_root,
                        &text_extensions,
                        &text_filenames,
                        &compiled_patterns,
                        max_lines_per_blob,
                    )
                })
                .collect()
        })
        .await
        .unwrap_or_else(|e| {
            error!("Parallel processing failed: {}", e);
            Vec::new()
        });

        if results.is_empty() {
            warn!("No files were successfully processed");
            return IndexResult {
                status: "error".to_string(),
                message: "No files could be processed".to_string(),
                stats: None,
            };
        }

        // Step 4: Build new index from results (not extend - ensures deleted files are removed)
        let mut new_index = IndexData {
            version: CURRENT_INDEX_VERSION,
            config_hash: self.config_hash.clone(),
            entries: HashMap::with_capacity(results.len()),
        };

        let mut cached_count = 0usize;
        let mut new_blobs: Vec<Blob> = Vec::new();

        for pf in results {
            match pf.result {
                ProcessedResult::Cached { entry } => {
                    cached_count += entry.blob_hashes.len();
                    new_index.entries.insert(pf.rel_path, entry);
                }
                ProcessedResult::New { blobs, entry } => {
                    new_index.entries.insert(pf.rel_path, entry);
                    new_blobs.extend(blobs);
                }
            }
        }

        info!(
            "Incremental indexing: {} cached blobs, {} new blobs",
            cached_count,
            new_blobs.len()
        );

        // Step 5: Upload new blobs with adaptive strategy
        let mut uploaded_blob_names: Vec<String> = Vec::new();
        let mut failed_batch_count: usize = 0;

        if !new_blobs.is_empty() {
            let blobs_count = new_blobs.len();
            let mut strategy =
                AdaptiveStrategy::new(blobs_count, self.cli_overrides.clone(), !self.no_adaptive);

            info!(
                "Uploading {} new chunks (adaptive: {}, initial concurrency: {}, timeout: {}s)",
                blobs_count,
                !self.no_adaptive,
                strategy.concurrency(),
                strategy.timeout_ms() / 1000
            );

            let (names, failed) = self.upload_blobs_adaptive(new_blobs, &mut strategy).await;
            uploaded_blob_names = names;
            failed_batch_count = failed;
        } else {
            info!("No new files to upload, using cached index");
        }

        // Step 6: Save new index (atomic write)
        let total_blobs = cached_count + uploaded_blob_names.len();
        let save_failed = if let Err(e) = self.save_index(&new_index) {
            error!("Failed to save index: {}", e);
            true
        } else {
            false
        };

        info!(
            "Indexing complete: {} files, {} total blobs",
            new_index.entries.len(),
            total_blobs
        );

        // Step 7: Determine result status
        let (status, message) = if save_failed {
            (
                "error".to_string(),
                format!(
                    "Failed to save index (indexed {} blobs, {} failed batches)",
                    total_blobs, failed_batch_count
                ),
            )
        } else if failed_batch_count > 0 {
            (
                "partial".to_string(),
                format!(
                    "Indexed {} blobs with {} failed batches (cached: {}, new: {})",
                    total_blobs,
                    failed_batch_count,
                    cached_count,
                    uploaded_blob_names.len()
                ),
            )
        } else {
            (
                "success".to_string(),
                format!(
                    "Indexed {} blobs (cached: {}, new: {})",
                    total_blobs,
                    cached_count,
                    uploaded_blob_names.len()
                ),
            )
        };

        IndexResult {
            status,
            message,
            stats: Some(IndexStats {
                total_blobs,
                existing_blobs: cached_count,
                new_blobs: uploaded_blob_names.len(),
                failed_batches: if failed_batch_count > 0 {
                    Some(failed_batch_count)
                } else {
                    None
                },
            }),
        }
    }

    /// Search code context
    pub async fn search_context(&self, query: &str) -> Result<String> {
        info!("Starting search: {}", query);

        // Auto-index first
        let index_result = self.index_project().await;
        if index_result.status == "error" {
            return Err(anyhow!("Failed to index project: {}", index_result.message));
        }
        if index_result.status == "partial" {
            warn!(
                "Indexing completed with some failures: {}",
                index_result.message
            );
        }

        // Load index
        let index_data = self.load_index();
        let blob_names = index_data.get_all_blob_hashes();
        if blob_names.is_empty() {
            return Err(anyhow!("No blobs found after indexing"));
        }

        // Execute search
        info!("Searching {} chunks...", blob_names.len());

        let url = format!("{}/agents/codebase-retrieval", self.base_url);
        let request = SearchRequest {
            information_request: query.to_string(),
            blobs: BlobsPayload {
                checkpoint_id: None,
                added_blobs: blob_names,
                deleted_blobs: Vec::new(),
            },
            dialog: Vec::new(),
            max_output_length: 0,
            disable_codebase_retrieval: false,
            enable_commit_retrieval: false,
        };

        let request_id = generate_request_id();
        let start_time = Instant::now();

        // Lazy serialization: only serialize body if logging is enabled
        let http_request_log = if http_logger::is_enabled() {
            let request_body = serde_json::to_string(&request).ok();
            Some(HttpRequestLog {
                method: "POST".to_string(),
                url: url.clone(),
                headers: http_logger::extract_headers_from_builder(
                    "application/json",
                    USER_AGENT,
                    &request_id,
                    get_session_id(),
                    &self.token,
                ),
                body: request_body,
            })
        } else {
            None
        };

        let response = self
            .client
            .post(&url)
            .timeout(Duration::from_secs(self.retrieval_timeout_secs))
            .header("Content-Type", "application/json")
            .header("User-Agent", USER_AGENT)
            .header("x-request-id", &request_id)
            .header("x-request-session-id", get_session_id())
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&request)
            .send()
            .await;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        match response {
            Ok(resp) => {
                let status = resp.status();
                let response_headers = if http_logger::is_enabled() {
                    http_logger::extract_response_headers(&resp)
                } else {
                    Vec::new()
                };

                if !status.is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    if let Some(ref req_log) = http_request_log {
                        let response_log = HttpResponseLog {
                            status: status.as_u16(),
                            headers: response_headers,
                            body: Some(text.clone()),
                        };
                        http_logger::log_request(
                            Some(&self.project_root),
                            req_log,
                            Some(&response_log),
                            duration_ms,
                            Some(&format!("Search failed: {} - {}", status, text)),
                        );
                    }
                    return Err(anyhow!("Search failed: {} - {}", status, text));
                }

                let body_text = resp.text().await.unwrap_or_default();
                if let Some(ref req_log) = http_request_log {
                    let response_log = HttpResponseLog {
                        status: status.as_u16(),
                        headers: response_headers,
                        body: Some(body_text.clone()),
                    };
                    http_logger::log_request(
                        Some(&self.project_root),
                        req_log,
                        Some(&response_log),
                        duration_ms,
                        None,
                    );
                }

                let search_response: SearchResponse = serde_json::from_str(&body_text)?;

                match search_response.formatted_retrieval {
                    Some(result) if !result.is_empty() => {
                        info!("Search complete");
                        Ok(result)
                    }
                    _ => {
                        info!("No relevant code found");
                        Ok("No relevant code context found for your query.".to_string())
                    }
                }
            }
            Err(e) => {
                let error_msg = e.to_string();
                if let Some(ref req_log) = http_request_log {
                    http_logger::log_request(
                        Some(&self.project_root),
                        req_log,
                        None,
                        duration_ms,
                        Some(&error_msg),
                    );
                }
                Err(anyhow!("Search request failed: {}", error_msg))
            }
        }
    }
}

/// Standalone file processing function for use in parallel context
/// (cannot capture &self in spawn_blocking closure)
fn process_file_standalone(
    path: &Path,
    old_index: &IndexData,
    project_root: &Path,
    _text_extensions: &HashSet<String>,
    _text_filenames: &HashSet<String>,
    _compiled_patterns: &[(String, Option<Regex>)],
    max_lines_per_blob: usize,
) -> Option<ProcessedFile> {
    // Calculate relative path
    let rel_path = match path.strip_prefix(project_root) {
        Ok(p) => normalize_relative_path(&p.to_string_lossy()),
        Err(_) => return None,
    };

    // Helper to preserve old entry on transient errors
    let preserve_old = || -> Option<ProcessedFile> {
        old_index
            .entries
            .get(&rel_path)
            .map(|cached| ProcessedFile {
                rel_path: rel_path.clone(),
                result: ProcessedResult::Cached {
                    entry: cached.clone(),
                },
            })
    };

    // Get metadata
    // NotFound = file deleted, don't preserve; other errors = transient, preserve old entry
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => return preserve_old(),
    };

    // Check file size before reading
    if metadata.len() > MAX_BLOB_SIZE as u64 {
        return None;
    }

    let mtime = match metadata.modified() {
        Ok(t) => t,
        Err(_) => return preserve_old(),
    };

    let duration = mtime.duration_since(UNIX_EPOCH).unwrap_or_default();
    let mtime_secs = duration.as_secs();
    let mtime_nanos = duration.subsec_nanos();
    let size = metadata.len();

    // Check cache
    // For high-precision filesystems (mtime_nanos != 0): use mtime+size for cache hit
    // For low-precision filesystems (mtime_nanos == 0): use mtime_secs+size, then verify by hash
    if let Some(cached) = old_index.entries.get(&rel_path) {
        if cached.mtime_secs == mtime_secs && cached.size == size && !cached.blob_hashes.is_empty()
        {
            // High precision: mtime_nanos match confirms cache hit
            if mtime_nanos != 0 && cached.mtime_nanos == mtime_nanos {
                return Some(ProcessedFile {
                    rel_path,
                    result: ProcessedResult::Cached {
                        entry: cached.clone(),
                    },
                });
            }
            // Low precision (mtime_nanos=0): read file, compute hash, compare with cached
            // If hash matches, it's a cache hit (avoid re-upload)
            if mtime_nanos == 0 {
                if let Ok(content) = IndexManager::read_file_with_encoding(path) {
                    if !IndexManager::is_binary_content(&content) {
                        let clean_content = IndexManager::sanitize_content(&content);
                        if clean_content.len() <= MAX_BLOB_SIZE {
                            let blobs = split_file_content_standalone(
                                &rel_path,
                                &clean_content,
                                max_lines_per_blob,
                            );
                            let new_hashes: Vec<String> = blobs
                                .iter()
                                .map(|b| IndexManager::calculate_blob_name(&b.path, &b.content))
                                .collect();
                            // Hash match = content unchanged, use cached entry with updated mtime
                            if new_hashes == cached.blob_hashes {
                                let updated_entry = FileEntry {
                                    mtime_secs,
                                    mtime_nanos: 0,
                                    size,
                                    blob_hashes: cached.blob_hashes.clone(),
                                };
                                return Some(ProcessedFile {
                                    rel_path,
                                    result: ProcessedResult::Cached {
                                        entry: updated_entry,
                                    },
                                });
                            }
                            // Hash mismatch = content changed, return as new
                            return Some(ProcessedFile {
                                rel_path,
                                result: ProcessedResult::New {
                                    blobs,
                                    entry: FileEntry {
                                        mtime_secs,
                                        mtime_nanos: 0,
                                        size,
                                        blob_hashes: new_hashes,
                                    },
                                },
                            });
                        }
                    }
                }
            }
        }
    }

    // Cache miss - read and process file
    // Try to read file; handle deletion that may occur between metadata check and read
    let content = match fs::read(path) {
        Ok(bytes) => {
            // Decode with encoding detection
            match IndexManager::decode_bytes_with_encoding(&bytes) {
                Ok(c) => c,
                Err(_) => return preserve_old(),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => return preserve_old(), // Other read errors: preserve old entry
    };

    // Skip binary files
    if IndexManager::is_binary_content(&content) {
        return None;
    }

    let clean_content = IndexManager::sanitize_content(&content);

    if clean_content.len() > MAX_BLOB_SIZE {
        return None;
    }

    let blobs = split_file_content_standalone(&rel_path, &clean_content, max_lines_per_blob);
    let blob_hashes: Vec<String> = blobs
        .iter()
        .map(|b| IndexManager::calculate_blob_name(&b.path, &b.content))
        .collect();

    let entry = FileEntry {
        mtime_secs,
        mtime_nanos,
        size,
        blob_hashes,
    };

    Some(ProcessedFile {
        rel_path,
        result: ProcessedResult::New { blobs, entry },
    })
}

/// Standalone file content splitting for use in parallel context
fn split_file_content_standalone(
    file_path: &str,
    content: &str,
    max_lines_per_blob: usize,
) -> Vec<Blob> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let max_lines = if max_lines_per_blob == 0 {
        800
    } else {
        max_lines_per_blob
    };

    if total_lines <= max_lines {
        return vec![Blob {
            path: file_path.to_string(),
            content: content.to_string(),
        }];
    }

    let num_chunks = total_lines.div_ceil(max_lines);
    let mut blobs = Vec::new();

    for chunk_idx in 0..num_chunks {
        let start_line = chunk_idx * max_lines;
        let end_line = (start_line + max_lines).min(total_lines);
        let chunk_lines: Vec<&str> = lines[start_line..end_line].to_vec();
        let chunk_content = chunk_lines.join("\n");
        let chunk_path = format!("{}#chunk{}of{}", file_path, chunk_idx + 1, num_chunks);

        blobs.push(Blob {
            path: chunk_path,
            content: chunk_content,
        });
    }

    blobs
}

/// Standalone file path collection for use in spawn_blocking
fn collect_file_paths_standalone(
    project_root: &Path,
    text_extensions: &HashSet<String>,
    text_filenames: &HashSet<String>,
    compiled_patterns: &[(String, Option<Regex>)],
) -> Vec<PathBuf> {
    // Load gitignore
    let gitignore = {
        let gitignore_path = project_root.join(".gitignore");
        if gitignore_path.exists() {
            let mut builder = GitignoreBuilder::new(project_root);
            let _ = builder.add(&gitignore_path);
            builder.build().ok()
        } else {
            None
        }
    };

    WalkDir::new(project_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            !should_exclude_standalone(
                e.path(),
                e.file_type().is_dir(),
                project_root,
                gitignore.as_ref(),
                compiled_patterns,
            )
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_indexable_file_standalone(e.path(), text_extensions, text_filenames))
        .map(|e| e.into_path())
        .collect()
}

/// Standalone exclude check for use in spawn_blocking
fn should_exclude_standalone(
    path: &Path,
    is_dir: bool,
    project_root: &Path,
    gitignore: Option<&Gitignore>,
    compiled_patterns: &[(String, Option<Regex>)],
) -> bool {
    let relative_path = match path.strip_prefix(project_root) {
        Ok(p) => p,
        Err(_) => {
            // Fail-closed: if we can't determine the relative path, exclude the file for safety
            // This can happen due to path normalization issues (e.g., Windows \\?\ prefixes)
            warn!(
                "Path prefix mismatch, excluding for safety: {:?} vs {:?}",
                path, project_root
            );
            return true;
        }
    };

    let path_str = normalize_relative_path(&relative_path.to_string_lossy());

    // Check gitignore
    if let Some(gi) = gitignore {
        if gi.matched(&path_str, is_dir).is_ignore() {
            return true;
        }
    }

    // Check exclude patterns using precompiled regexes
    let path_parts: Vec<&str> = path_str.split('/').collect();
    for (pattern, compiled_regex) in compiled_patterns {
        if let Some(regex) = compiled_regex {
            // Check each path component
            for part in &path_parts {
                if regex.is_match(part) {
                    return true;
                }
            }
            // Check full path
            if regex.is_match(&path_str) {
                return true;
            }
        } else {
            // Fallback to string matching if regex failed to compile
            for part in &path_parts {
                if *part == pattern {
                    return true;
                }
            }
            if path_str == *pattern {
                return true;
            }
        }
    }

    false
}

/// Standalone indexable file check for use in spawn_blocking
fn is_indexable_file_standalone(
    path: &Path,
    text_extensions: &HashSet<String>,
    text_filenames: &HashSet<String>,
) -> bool {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    text_filenames.contains(filename) || (!ext.is_empty() && text_extensions.contains(&ext))
}
