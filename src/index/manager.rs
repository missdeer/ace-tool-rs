//! Index manager - Core indexing and search logic

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use encoding_rs::{GB18030, GBK, UTF_8, WINDOWS_1252};
use futures::stream::{self, StreamExt};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{error, info, warn};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::config::{get_upload_strategy, Config};
use crate::utils::project_detector::get_index_file_path;

/// Maximum blob size in bytes (500KB)
const MAX_BLOB_SIZE: usize = 500 * 1024;

/// Maximum batch size in bytes (5MB)
const MAX_BATCH_SIZE: usize = 5 * 1024 * 1024;

/// User-Agent header value (matches augment.mjs format: augment.cli/{version}/{mode})
const USER_AGENT: &str = "augment.cli/0.1.3/mcp";

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
}

impl IndexManager {
    pub fn new(config: Arc<Config>, project_root: PathBuf) -> Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;

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
            Err(_) => return false,
        };

        let path_str = relative_path.to_string_lossy().replace('\\', "/");

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

    /// Load index data from file
    pub fn load_index(&self) -> Vec<String> {
        if !self.index_file_path.exists() {
            return Vec::new();
        }

        match fs::read_to_string(&self.index_file_path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(index) => index,
                Err(e) => {
                    warn!("Failed to parse index file, recreating: {}", e);
                    Vec::new()
                }
            },
            Err(e) => {
                error!("Failed to load index: {}", e);
                Vec::new()
            }
        }
    }

    /// Save index data to file
    pub fn save_index(&self, blob_names: &[String]) -> Result<()> {
        let content = serde_json::to_string_pretty(blob_names)?;
        fs::write(&self.index_file_path, content)?;
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

            let relative_path = path
                .strip_prefix(&self.project_root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");

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

    /// Upload a batch of blobs with retry
    async fn upload_batch(&self, blobs: &[Blob], timeout_ms: u64) -> Result<Vec<String>> {
        let batch_size: usize = blobs.iter().map(|b| b.content.len() + b.path.len()).sum();
        if batch_size > MAX_BATCH_SIZE {
            return Err(anyhow!("Batch too large: {}MB", batch_size / 1024 / 1024));
        }

        let url = format!("{}/batch-upload", self.base_url);
        let request = BatchUploadRequest {
            blobs: blobs.to_vec(),
        };

        let mut last_error = None;
        let max_retries = 3;

        for attempt in 0..max_retries {
            let request_id = generate_request_id();
            let result = self
                .client
                .post(&url)
                .timeout(Duration::from_millis(timeout_ms))
                .header("Content-Type", "application/json")
                .header("User-Agent", USER_AGENT)
                .header("x-request-id", &request_id)
                .header("x-request-session-id", get_session_id())
                .header("Authorization", format!("Bearer {}", self.token))
                .json(&request)
                .send()
                .await;

            match result {
                Ok(response) => {
                    let status = response.status();

                    if status == 401 {
                        return Err(anyhow!("Token invalid or expired"));
                    }
                    if status == 403 {
                        return Err(anyhow!("Access denied, token may be disabled"));
                    }
                    if status == 400 {
                        let text = response.text().await.unwrap_or_default();
                        return Err(anyhow!("Bad request: {}", text));
                    }

                    if status.is_success() {
                        let resp: BatchUploadResponse = response.json().await?;
                        return Ok(resp.blob_names);
                    }

                    // Handle rate limiting (429) with Retry-After header support
                    if status == 429 && attempt < max_retries - 1 {
                        let retry_after = response
                            .headers()
                            .get("Retry-After")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(1);
                        let wait_time = retry_after * 1000;
                        warn!(
                            "Rate limited (attempt {}/{}), retrying in {}ms...",
                            attempt + 1,
                            max_retries,
                            wait_time
                        );
                        tokio::time::sleep(Duration::from_millis(wait_time)).await;
                        continue;
                    }

                    if status.is_server_error() && attempt < max_retries - 1 {
                        let wait_time = 1000 * (1 << attempt);
                        warn!(
                            "Server error (attempt {}/{}), retrying in {}ms...",
                            attempt + 1,
                            max_retries,
                            wait_time
                        );
                        tokio::time::sleep(Duration::from_millis(wait_time)).await;
                        continue;
                    }

                    return Err(anyhow!("HTTP error: {}", status));
                }
                Err(e) => {
                    let error_msg = e.to_string();
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
                    last_error = Some(error_msg);
                }
            }
        }

        Err(anyhow!(
            "All retries failed: {}",
            last_error.unwrap_or_default()
        ))
    }

    /// Index the project
    pub async fn index_project(&self) -> IndexResult {
        info!("Starting project indexing: {:?}", self.project_root);

        // Collect files
        info!("Scanning files...");
        let blobs = match self.collect_files() {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to collect files: {}", e);
                return IndexResult {
                    status: "error".to_string(),
                    message: format!("Failed to collect files: {}", e),
                    stats: None,
                };
            }
        };

        if blobs.is_empty() {
            warn!("No indexable text files found");
            return IndexResult {
                status: "error".to_string(),
                message: "No text files found in project".to_string(),
                stats: None,
            };
        }

        info!("Found {} file chunks", blobs.len());

        // Load existing index
        let existing_blob_names: HashSet<String> = self.load_index().into_iter().collect();

        // Calculate hashes for all blobs
        let mut blob_hash_map: std::collections::HashMap<String, Blob> =
            std::collections::HashMap::new();
        for blob in blobs {
            let hash = Self::calculate_blob_name(&blob.path, &blob.content);
            blob_hash_map.insert(hash, blob);
        }

        // Separate existing and new blobs
        let all_hashes: HashSet<String> = blob_hash_map.keys().cloned().collect();
        let existing_hashes: HashSet<String> = all_hashes
            .intersection(&existing_blob_names)
            .cloned()
            .collect();
        let new_hashes: Vec<String> = all_hashes
            .difference(&existing_blob_names)
            .cloned()
            .collect();

        info!(
            "Incremental indexing: {} existing, {} new",
            existing_hashes.len(),
            new_hashes.len()
        );

        let mut uploaded_blob_names: Vec<String> = Vec::new();
        let mut failed_batch_count: usize = 0;

        if !new_hashes.is_empty() {
            let blobs_to_upload: Vec<Blob> = new_hashes
                .iter()
                .filter_map(|h| blob_hash_map.get(h).cloned())
                .collect();

            let strategy = get_upload_strategy(blobs_to_upload.len());
            info!(
                "Project scale: {} (batch: {}, concurrency: {})",
                strategy.scale_name, strategy.batch_size, strategy.concurrency
            );

            // Upload in batches
            let blobs_count = blobs_to_upload.len();
            let batches = self.build_batches(blobs_to_upload, strategy.batch_size);

            info!(
                "Uploading {} new chunks in {} batches (concurrency: {})",
                blobs_count,
                batches.len(),
                strategy.concurrency
            );

            // Upload batches concurrently with controlled concurrency
            let total_batches = batches.len();
            let timeout_ms = strategy.timeout_ms;
            let concurrency = strategy.concurrency;

            let results: Vec<(usize, Result<Vec<String>>)> = stream::iter(
                batches
                    .into_iter()
                    .enumerate()
                    .map(|(i, batch)| async move {
                        info!("Uploading batch {}/{}...", i + 1, total_batches);
                        let result = self.upload_batch(&batch, timeout_ms).await;
                        (i, result)
                    }),
            )
            .buffer_unordered(concurrency)
            .collect()
            .await;

            // Process results
            for (i, result) in results {
                match result {
                    Ok(names) => {
                        uploaded_blob_names.extend(names);
                    }
                    Err(e) => {
                        error!("Batch {} upload failed: {}", i + 1, e);
                        failed_batch_count += 1;
                    }
                }
            }
        } else {
            info!("No new files to upload, using cached index");
        }

        // Save index
        let all_blob_names: Vec<String> = existing_hashes
            .into_iter()
            .chain(uploaded_blob_names.iter().cloned())
            .collect();

        let save_failed = if let Err(e) = self.save_index(&all_blob_names) {
            error!("Failed to save index: {}", e);
            true
        } else {
            false
        };

        info!("Indexing complete: {} total chunks", all_blob_names.len());

        // Determine status based on failed batches and save result
        let (status, message) = if save_failed {
            (
                "error".to_string(),
                format!(
                    "Failed to save index (indexed {} blobs, {} failed batches)",
                    all_blob_names.len(),
                    failed_batch_count
                ),
            )
        } else if failed_batch_count > 0 {
            (
                "partial".to_string(),
                format!(
                    "Indexed {} blobs with {} failed batches (existing: {}, new: {})",
                    all_blob_names.len(),
                    failed_batch_count,
                    all_blob_names.len() - uploaded_blob_names.len(),
                    uploaded_blob_names.len()
                ),
            )
        } else {
            (
                "success".to_string(),
                format!(
                    "Indexed {} blobs (existing: {}, new: {})",
                    all_blob_names.len(),
                    all_blob_names.len() - uploaded_blob_names.len(),
                    uploaded_blob_names.len()
                ),
            )
        };

        IndexResult {
            status,
            message,
            stats: Some(IndexStats {
                total_blobs: all_blob_names.len(),
                existing_blobs: all_blob_names.len() - uploaded_blob_names.len(),
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
        let blob_names = self.load_index();
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
        let response = self
            .client
            .post(&url)
            .timeout(Duration::from_secs(60))
            .header("Content-Type", "application/json")
            .header("User-Agent", USER_AGENT)
            .header("x-request-id", &request_id)
            .header("x-request-session-id", get_session_id())
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Search failed: {} - {}", status, text));
        }

        let search_response: SearchResponse = response.json().await?;

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
}
