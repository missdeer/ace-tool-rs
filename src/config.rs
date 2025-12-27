//! Configuration module - CLI arguments and settings

use anyhow::{anyhow, Result};
use std::collections::HashSet;
use std::sync::Arc;

/// Main configuration struct
#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub token: String,
    pub max_lines_per_blob: usize,
    pub text_extensions: HashSet<String>,
    pub text_filenames: HashSet<String>,
    pub exclude_patterns: Vec<String>,
}

/// Upload strategy based on project scale
#[derive(Debug, Clone)]
pub struct UploadStrategy {
    pub batch_size: usize,
    pub concurrency: usize,
    pub timeout_ms: u64,
    pub scale_name: &'static str,
}

impl Config {
    pub fn new(base_url: String, token: String) -> Result<Arc<Self>> {
        // Ensure base_url uses https://
        let base_url = if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            format!("https://{}", base_url)
        } else if base_url.starts_with("http://") {
            base_url.replace("http://", "https://")
        } else {
            base_url
        };

        // Remove trailing slash
        let base_url = base_url.trim_end_matches('/').to_string();

        if base_url.is_empty() {
            return Err(anyhow!("base_url cannot be empty"));
        }

        if token.is_empty() {
            return Err(anyhow!("token cannot be empty"));
        }

        Ok(Arc::new(Self {
            base_url,
            token,
            max_lines_per_blob: 800,
            text_extensions: default_text_extensions(),
            text_filenames: default_text_filenames(),
            exclude_patterns: default_exclude_patterns(),
        }))
    }
}

/// Get adaptive upload strategy based on blob count
pub fn get_upload_strategy(blob_count: usize) -> UploadStrategy {
    if blob_count < 100 {
        // Small project: conservative settings
        UploadStrategy {
            batch_size: 10,
            concurrency: 1,
            timeout_ms: 30000,
            scale_name: "小型",
        }
    } else if blob_count < 500 {
        // Medium project: moderate concurrency
        UploadStrategy {
            batch_size: 30,
            concurrency: 2,
            timeout_ms: 45000,
            scale_name: "中型",
        }
    } else if blob_count < 2000 {
        // Large project: efficient concurrency
        UploadStrategy {
            batch_size: 50,
            concurrency: 3,
            timeout_ms: 60000,
            scale_name: "大型",
        }
    } else {
        // Extra large project: maximize throughput
        UploadStrategy {
            batch_size: 70,
            concurrency: 4,
            timeout_ms: 90000,
            scale_name: "超大型",
        }
    }
}

/// Default supported text file extensions
fn default_text_extensions() -> HashSet<String> {
    [
        // Main programming languages
        ".py",
        ".js",
        ".ts",
        ".jsx",
        ".tsx",
        ".mjs",
        ".cjs",
        ".java",
        ".go",
        ".rs",
        ".cpp",
        ".c",
        ".cc",
        ".h",
        ".hpp",
        ".hxx",
        ".cs",
        ".rb",
        ".php",
        ".swift",
        ".kt",
        ".kts",
        ".scala",
        ".clj",
        ".cljs",
        // Other programming languages
        ".lua",
        ".dart",
        ".m",
        ".mm",
        ".pl",
        ".pm",
        ".r",
        ".R",
        ".jl",
        ".ex",
        ".exs",
        ".erl",
        ".hs",
        ".zig",
        ".v",
        ".nim",
        ".f90",
        ".f95",
        ".groovy",
        ".gradle",
        ".sol",
        ".move",
        // Config and data
        ".md",
        ".mdx",
        ".txt",
        ".json",
        ".jsonc",
        ".json5",
        ".yaml",
        ".yml",
        ".toml",
        ".xml",
        ".ini",
        ".conf",
        ".cfg",
        ".properties",
        ".editorconfig",
        // Web related
        ".html",
        ".htm",
        ".css",
        ".scss",
        ".sass",
        ".less",
        ".styl",
        ".vue",
        ".svelte",
        ".astro",
        // Template engines
        ".ejs",
        ".hbs",
        ".pug",
        ".jade",
        ".jinja",
        ".jinja2",
        ".erb",
        ".liquid",
        ".twig",
        ".mustache",
        ".njk",
        // Scripts and build
        ".sql",
        ".sh",
        ".bash",
        ".zsh",
        ".fish",
        ".ps1",
        ".psm1",
        ".bat",
        ".cmd",
        ".makefile",
        ".mk",
        ".cmake",
        // API and data formats
        ".graphql",
        ".gql",
        ".proto",
        ".prisma",
        ".csv",
        ".tsv",
        // Documentation
        ".rst",
        ".adoc",
        ".tex",
        ".org",
        // Docker and CI/CD
        ".dockerfile",
        ".containerfile",
        // Other
        ".vim",
        ".el",
        ".rkt",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Default exclude patterns
fn default_exclude_patterns() -> Vec<String> {
    [
        // Virtual environments and dependencies
        ".venv",
        "venv",
        ".env",
        "env",
        "node_modules",
        "vendor",
        ".pnpm",
        ".yarn",
        "bower_components",
        // Version control
        ".git",
        ".svn",
        ".hg",
        ".gitmodules",
        // Python cache
        "__pycache__",
        ".pytest_cache",
        ".mypy_cache",
        ".tox",
        ".eggs",
        "*.egg-info",
        ".ruff_cache",
        // Build artifacts
        "dist",
        "build",
        "target",
        "out",
        "bin",
        "obj",
        ".next",
        ".nuxt",
        ".output",
        ".vercel",
        ".netlify",
        ".turbo",
        ".parcel-cache",
        ".cache",
        ".temp",
        ".tmp",
        // Test coverage
        "coverage",
        ".nyc_output",
        "htmlcov",
        // IDE configuration
        ".idea",
        ".vscode",
        ".vs",
        "*.swp",
        "*.swo",
        // System files
        ".DS_Store",
        "Thumbs.db",
        "desktop.ini",
        // Compiled and binary files
        "*.pyc",
        "*.pyo",
        "*.pyd",
        "*.so",
        "*.dll",
        "*.dylib",
        "*.exe",
        "*.o",
        "*.obj",
        "*.class",
        "*.jar",
        "*.war",
        // Compressed and packaged files
        "*.min.js",
        "*.min.css",
        "*.bundle.js",
        "*.chunk.js",
        "*.map",
        "*.gz",
        "*.zip",
        "*.tar",
        "*.rar",
        // Lock files
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "Gemfile.lock",
        "poetry.lock",
        "Cargo.lock",
        "composer.lock",
        // Logs and temp files
        "*.log",
        "logs",
        "tmp",
        "temp",
        // Media files
        "*.png",
        "*.jpg",
        "*.jpeg",
        "*.gif",
        "*.ico",
        "*.svg",
        "*.mp3",
        "*.mp4",
        "*.wav",
        "*.avi",
        "*.mov",
        "*.pdf",
        "*.doc",
        "*.docx",
        "*.xls",
        "*.xlsx",
        // Font files
        "*.woff",
        "*.woff2",
        "*.ttf",
        "*.eot",
        "*.otf",
        // Database files
        "*.db",
        "*.sqlite",
        "*.sqlite3",
        // Ace-tool directory
        ".ace-tool",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Default known text filenames (without extension)
fn default_text_filenames() -> HashSet<String> {
    [
        // Build files
        "Makefile",
        "makefile",
        "GNUmakefile",
        "Dockerfile",
        "Containerfile",
        "Jenkinsfile",
        "Vagrantfile",
        "Procfile",
        // Config files
        ".gitignore",
        ".gitattributes",
        ".gitmodules",
        ".dockerignore",
        ".npmignore",
        ".eslintignore",
        ".prettierignore",
        ".stylelintignore",
        ".editorconfig",
        ".browserslistrc",
        ".npmrc",
        ".yarnrc",
        ".nvmrc",
        ".node-version",
        ".ruby-version",
        ".python-version",
        ".env.example",
        ".env.sample",
        ".env.template",
        // Tool configs
        ".eslintrc",
        ".prettierrc",
        ".stylelintrc",
        ".babelrc",
        ".postcssrc",
        ".huskyrc",
        ".lintstagedrc",
        ".commitlintrc",
        // Lock files and manifests
        "Gemfile",
        "Rakefile",
        "Brewfile",
        "Pipfile",
        "MANIFEST.in",
        "setup.py",
        "requirements.txt",
        "constraints.txt",
        // Documentation
        "README",
        "CHANGELOG",
        "LICENSE",
        "LICENCE",
        "AUTHORS",
        "CONTRIBUTORS",
        "HISTORY",
        "TODO",
        "ROADMAP",
        "COPYING",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}
