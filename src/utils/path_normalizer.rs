//! Path normalization for cross-platform and WSL support
//!
//! Handles:
//! - Windows/Unix path conversion
//! - WSL UNC paths (\\wsl$\... and \\wsl.localhost\...)
//! - /mnt/x/ path detection and conversion

use std::path::{Path, PathBuf};

/// Runtime environment detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEnv {
    /// Native Windows
    Windows,
    /// Running inside WSL
    WslNative,
    /// Unix (Linux/macOS, non-WSL)
    Unix,
}

impl RuntimeEnv {
    /// Detect current runtime environment
    pub fn detect() -> Self {
        #[cfg(windows)]
        {
            RuntimeEnv::Windows
        }
        #[cfg(unix)]
        {
            Self::detect_unix()
        }
    }

    #[cfg(unix)]
    fn detect_unix() -> Self {
        // Primary check: /proc/version contains WSL indicators
        if let Ok(version) = std::fs::read_to_string("/proc/version") {
            let lower = version.to_lowercase();
            if lower.contains("microsoft") || lower.contains("wsl") {
                return RuntimeEnv::WslNative;
            }
        }

        // Fallback check: WSL-specific environment variables
        // WSL_INTEROP is set in WSL2 for Windows interop socket
        // WSL_DISTRO_NAME is set to the distribution name
        if std::env::var("WSL_INTEROP").is_ok() || std::env::var("WSL_DISTRO_NAME").is_ok() {
            return RuntimeEnv::WslNative;
        }

        // Additional fallback: check /proc/sys/kernel/osrelease
        if let Ok(osrelease) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
            let lower = osrelease.to_lowercase();
            if lower.contains("microsoft") || lower.contains("wsl") {
                return RuntimeEnv::WslNative;
            }
        }

        RuntimeEnv::Unix
    }
}

/// Normalized path representation
#[derive(Debug, Clone)]
pub struct NormalizedPath {
    /// Canonical path (Unix-style forward slashes, for hashing and storage)
    pub canonical: String,
    /// Local accessible path (native to current OS)
    pub local: PathBuf,
}

/// Convert Windows path to WSL path
/// Example: C:\Users\foo -> /mnt/c/Users/foo
pub fn win_to_wsl(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let bytes = path.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' {
        return None;
    }

    let drive = bytes[0] as char;
    if !drive.is_ascii_alphabetic() {
        return None;
    }

    let rest = &path[2..];
    if !rest.is_empty() && !rest.starts_with('/') {
        return None;
    }

    Some(format!("/mnt/{}{}", drive.to_ascii_lowercase(), rest))
}

/// Convert WSL path to Windows path
/// Example: /mnt/c/Users/foo -> C:\Users\foo
pub fn wsl_to_win(path: &str) -> Option<String> {
    if path.starts_with("/mnt/") && path.len() >= 6 {
        let chars: Vec<char> = path.chars().collect();
        let drive = chars[5];
        if drive.is_ascii_alphabetic() {
            // Check if it's exactly /mnt/x or /mnt/x/...
            if path.len() == 6 || chars.get(6) == Some(&'/') {
                let rest = if path.len() > 6 { &path[6..] } else { "" };
                return Some(format!(
                    "{}:{}",
                    drive.to_ascii_uppercase(),
                    rest.replace('/', "\\")
                ));
            }
        }
    }
    None
}

/// WSL UNC path info
#[derive(Debug, Clone)]
pub struct WslUncPath {
    /// WSL distribution name (e.g., "Ubuntu")
    pub distro: String,
    /// Inner path within WSL (Unix-style, e.g., "/home/user")
    pub inner_path: String,
}

/// Parse WSL UNC path
/// Formats: \\wsl$\<distro>\path or \\wsl.localhost\<distro>\path
pub fn parse_wsl_unc(path: &str) -> Option<WslUncPath> {
    let path = path.replace('/', "\\");
    let lower = path.to_ascii_lowercase();

    let prefixes = ["\\\\wsl$\\", "\\\\wsl.localhost\\"];

    for prefix in prefixes {
        if lower.starts_with(prefix) {
            let rest = &path[prefix.len()..];
            if rest.is_empty() {
                return None;
            }
            if let Some(idx) = rest.find('\\') {
                if idx == 0 {
                    return None;
                }
                let distro = rest[..idx].to_string();
                let inner_path = rest[idx..].replace('\\', "/");
                return Some(WslUncPath { distro, inner_path });
            } else {
                // Just the distro, root path
                return Some(WslUncPath {
                    distro: rest.to_string(),
                    inner_path: "/".to_string(),
                });
            }
        }
    }
    None
}

/// Build WSL UNC path from distro and inner path
pub fn build_wsl_unc(distro: &str, path: &str) -> String {
    let normalized = path.replace('/', "\\");
    let suffix = if normalized.is_empty() {
        String::new()
    } else if normalized.starts_with('\\') {
        normalized
    } else {
        format!("\\{}", normalized)
    };
    format!("\\\\wsl.localhost\\{}{}", distro, suffix)
}

/// Check if path is a WSL UNC path
pub fn is_wsl_unc_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("\\\\wsl$\\")
        || lower.starts_with("\\\\wsl.localhost\\")
        || lower.starts_with("//wsl$/")
        || lower.starts_with("//wsl.localhost/")
}

/// Check if path is a WSL /mnt/ path
pub fn is_wsl_mnt_path(path: &str) -> bool {
    if path.starts_with("/mnt/") && path.len() >= 6 {
        let chars: Vec<char> = path.chars().collect();
        let drive = chars[5];
        if drive.is_ascii_alphabetic() {
            return path.len() == 6 || chars.get(6) == Some(&'/');
        }
    }
    false
}

/// Normalize a path to canonical form
pub fn normalize_path(input: &Path, env: RuntimeEnv) -> NormalizedPath {
    let input_str = input.to_string_lossy();

    match env {
        RuntimeEnv::Windows => {
            if let Some(unc) = parse_wsl_unc(&input_str) {
                // WSL UNC path: keep UNC for local access, use inner path for canonical
                NormalizedPath {
                    canonical: unc.inner_path,
                    local: input.to_path_buf(),
                }
            } else if let Some(win_path) = wsl_to_win(&input_str) {
                // WSL /mnt/<drive>/... path: convert to Windows path
                NormalizedPath {
                    canonical: win_path.replace('\\', "/"),
                    local: PathBuf::from(win_path),
                }
            } else {
                // Regular Windows path: convert backslashes to forward slashes
                NormalizedPath {
                    canonical: input_str.replace('\\', "/"),
                    local: input.to_path_buf(),
                }
            }
        }
        RuntimeEnv::WslNative => {
            if let Some(unc) = parse_wsl_unc(&input_str) {
                // UNC path pointing to WSL: use inner path for local access
                NormalizedPath {
                    canonical: unc.inner_path.clone(),
                    local: PathBuf::from(unc.inner_path),
                }
            } else if let Some(wsl_path) = win_to_wsl(&input_str) {
                // Windows drive path: convert to /mnt/<drive>/...
                NormalizedPath {
                    canonical: wsl_path.clone(),
                    local: PathBuf::from(wsl_path),
                }
            } else if is_wsl_mnt_path(&input_str) {
                // /mnt/c/... path: keep as-is for both
                NormalizedPath {
                    canonical: input_str.to_string(),
                    local: input.to_path_buf(),
                }
            } else {
                // Pure Linux path
                NormalizedPath {
                    canonical: input_str.to_string(),
                    local: input.to_path_buf(),
                }
            }
        }
        RuntimeEnv::Unix => NormalizedPath {
            canonical: input_str.replace('\\', "/"),
            local: input.to_path_buf(),
        },
    }
}

/// Normalize a relative path string (for blob paths)
pub fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/")
}
