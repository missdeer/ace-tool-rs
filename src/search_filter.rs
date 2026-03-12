//! Search filtering options for dynamic document exclusion

use std::collections::HashSet;

use globset::{Glob, GlobSetBuilder};

/// Default document file extensions to exclude when `exclude_document_files` is true
pub const DEFAULT_DOCUMENT_EXTENSIONS: &[&str] = &[
    ".md", ".mdx", ".txt", ".csv", ".tsv", ".rst", ".adoc", ".tex", ".org",
];

/// Default document filenames (without extension) to exclude when `exclude_document_files` is true
pub const DEFAULT_DOCUMENT_FILENAMES: &[&str] = &[
    "README", "CHANGELOG", "TODO", "ROADMAP",
    "LICENSE", "LICENCE", "AUTHORS", "CONTRIBUTORS",
    "HISTORY", "COPYING", "NEWS", "CHANGES",
];

/// Search filter options for excluding entries from search results
#[derive(Debug, Clone, Default)]
pub struct SearchFilterOptions {
    /// Whether to exclude document files (md, txt, etc.)
    pub exclude_document_files: bool,
    /// Extensions to exclude (normalized to lowercase with leading dot)
    pub exclude_extensions: HashSet<String>,
    /// Filenames without extension to exclude (e.g., README, CHANGELOG)
    pub exclude_filenames: HashSet<String>,
    /// Glob patterns to exclude
    pub exclude_globs: Vec<String>,
    /// Compiled glob matcher (lazy initialization)
    compiled_globset: Option<globset::GlobSet>,
}

impl SearchFilterOptions {
    /// Create filter options from MCP tool arguments
    pub fn from_args(args: &crate::tools::search_context::SearchContextArgs) -> Self {
        let exclude_document_files = args.exclude_document_files.unwrap_or(false);

        let mut exclude_extensions = HashSet::new();
        let mut exclude_filenames = HashSet::new();

        // Handle exclude_extensions - normalize to lowercase with leading dot
        if let Some(ref exts) = args.exclude_extensions {
            for ext in exts {
                let normalized = normalize_extension(ext);
                if !normalized.is_empty() {
                    exclude_extensions.insert(normalized);
                }
            }
        }

        // Add default document extensions and filenames if exclude_document_files is true
        if exclude_document_files {
            for ext in DEFAULT_DOCUMENT_EXTENSIONS {
                exclude_extensions.insert(ext.to_string());
            }
            for name in DEFAULT_DOCUMENT_FILENAMES {
                exclude_filenames.insert(name.to_lowercase());
            }
        }

        Self {
            exclude_document_files,
            exclude_extensions,
            exclude_filenames,
            exclude_globs: args.exclude_globs.clone().unwrap_or_default(),
            compiled_globset: None,
        }
    }

    /// Compile glob patterns into a matcher (call once before filtering)
    pub fn compile_globs(&mut self) -> Result<(), globset::Error> {
        if self.exclude_globs.is_empty() {
            self.compiled_globset = None;
            return Ok(());
        }

        let mut builder = GlobSetBuilder::new();
        for pattern in &self.exclude_globs {
            builder.add(Glob::new(pattern)?);
        }

        self.compiled_globset = Some(builder.build()?);
        Ok(())
    }

    /// Ensure glob patterns are compiled exactly once before filtering.
    pub fn ensure_compiled_globs(&mut self) -> Result<(), globset::Error> {
        if self.exclude_globs.is_empty() {
            self.compiled_globset = None;
            return Ok(());
        }

        if self.compiled_globset.is_some() {
            return Ok(());
        }

        self.compile_globs()
    }

    /// Check if a relative path should be excluded from search
    pub fn should_exclude(&self, rel_path: &str) -> bool {
        // Check extension exclusion
        if !self.exclude_extensions.is_empty() {
            if let Some(ext) = get_extension(rel_path) {
                if self.exclude_extensions.contains(&ext) {
                    return true;
                }
            }
        }

        // Check filename exclusion (for files without extension like README)
        if !self.exclude_filenames.is_empty() {
            if let Some(filename) = get_filename(rel_path) {
                if self.exclude_filenames.contains(&filename) {
                    return true;
                }
            }
        }

        // Check glob pattern exclusion
        if let Some(ref globset) = self.compiled_globset {
            if globset.is_match(rel_path) {
                return true;
            }
        }

        false
    }

    /// Check if any filtering is active
    pub fn is_active(&self) -> bool {
        self.exclude_document_files
            || !self.exclude_extensions.is_empty()
            || !self.exclude_filenames.is_empty()
            || !self.exclude_globs.is_empty()
    }
}

/// Normalize extension to lowercase with leading dot
fn normalize_extension(ext: &str) -> String {
    let trimmed = ext.trim().to_lowercase();
    if trimmed.starts_with('.') {
        trimmed
    } else if !trimmed.is_empty() {
        format!(".{}", trimmed)
    } else {
        trimmed
    }
}

/// Extract extension from path (lowercase, with leading dot)
fn get_extension(path: &str) -> Option<String> {
    let path_lower = path.to_lowercase();
    let idx = path_lower.rfind('.')?;
    // Ensure the dot is not part of a directory name (no slash after the dot)
    if path_lower[idx..].contains('/') || path_lower[idx..].contains('\\') {
        return None;
    }
    // Ensure it's not a hidden file (dot at the beginning with no extension)
    // Hidden files like ".hidden" should return None
    if idx == 0 {
        return None;
    }
    Some(path_lower[idx..].to_string())
}

/// Extract filename (without extension) from path
fn get_filename(path: &str) -> Option<String> {
    let path_lower = path.to_lowercase();
    // Get the last component of the path
    let filename = path_lower.rsplit('/').next()?;
    // If it has an extension, remove it
    if let Some(dot_idx) = filename.rfind('.') {
        // Make sure it's not a hidden file like ".gitignore"
        if dot_idx > 0 {
            return Some(filename[..dot_idx].to_string());
        }
    }
    Some(filename.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_filter_options_default() {
        let filter = SearchFilterOptions::default();
        assert!(!filter.exclude_document_files);
        assert!(filter.exclude_extensions.is_empty());
        assert!(filter.exclude_filenames.is_empty());
        assert!(filter.exclude_globs.is_empty());
        assert!(!filter.is_active());
    }

    #[test]
    fn test_normalize_extension() {
        assert_eq!(normalize_extension("md"), ".md");
        assert_eq!(normalize_extension(".md"), ".md");
        assert_eq!(normalize_extension(" .TXT "), ".txt");
        assert_eq!(normalize_extension(""), "");
        assert_eq!(normalize_extension("  "), "");
    }

    #[test]
    fn test_get_extension() {
        assert_eq!(get_extension("README.md"), Some(".md".to_string()));
        assert_eq!(get_extension("src/main.rs"), Some(".rs".to_string()));
        assert_eq!(get_extension("docs/guide.MD"), Some(".md".to_string()));
        assert_eq!(get_extension("notes.TxT"), Some(".txt".to_string()));
        assert_eq!(get_extension("noextension"), None);
        assert_eq!(get_extension(".hidden"), None);
    }

    #[test]
    fn test_should_exclude_by_extension() {
        let mut filter = SearchFilterOptions::default();
        filter.exclude_extensions.insert(".md".to_string());

        assert!(filter.should_exclude("README.md"));
        assert!(filter.should_exclude("docs/guide.MD")); // Case insensitive
        assert!(!filter.should_exclude("src/main.rs"));
        assert!(!filter.should_exclude("config.yaml"));
    }

    #[test]
    fn test_should_exclude_by_glob() {
        let mut filter = SearchFilterOptions {
            exclude_globs: vec!["docs/**".to_string(), "**/README*".to_string()],
            ..Default::default()
        };
        filter.compile_globs().unwrap();

        assert!(filter.should_exclude("docs/guide.md"));
        assert!(filter.should_exclude("docs/api/reference.rs"));
        assert!(filter.should_exclude("README.md"));
        assert!(filter.should_exclude("subdir/README-zh-CN.md"));
        assert!(!filter.should_exclude("src/main.rs"));
        assert!(!filter.should_exclude("config/app.yaml"));
    }

    #[test]
    fn test_ensure_compiled_globs_compiles_on_demand() {
        let mut filter = SearchFilterOptions {
            exclude_globs: vec!["docs/**".to_string()],
            ..Default::default()
        };

        // 未显式 compile 前，glob 规则尚未生效
        assert!(!filter.should_exclude("docs/guide.md"));

        filter.ensure_compiled_globs().unwrap();

        assert!(filter.should_exclude("docs/guide.md"));
        assert!(!filter.should_exclude("src/main.rs"));
    }

    #[test]
    fn test_exclude_document_files() {
        let mut filter = SearchFilterOptions {
            exclude_document_files: true,
            ..Default::default()
        };
        // from_args would populate exclude_extensions with default doc extensions

        // Manually populate for this test
        for ext in DEFAULT_DOCUMENT_EXTENSIONS {
            filter.exclude_extensions.insert(ext.to_string());
        }

        assert!(filter.should_exclude("README.md"));
        assert!(filter.should_exclude("notes.txt"));
        assert!(filter.should_exclude("data.csv"));
        assert!(!filter.should_exclude("src/main.rs"));
        assert!(!filter.should_exclude("config.yaml"));
    }

    #[test]
    fn test_combined_filters_union() {
        let mut filter = SearchFilterOptions {
            exclude_document_files: true,
            exclude_globs: vec!["docs/**".to_string()],
            ..Default::default()
        };
        for ext in DEFAULT_DOCUMENT_EXTENSIONS {
            filter.exclude_extensions.insert(ext.to_string());
        }
        filter.exclude_extensions.insert(".rs".to_string());
        filter.compile_globs().unwrap();

        // Excluded by extension (.md from document files)
        assert!(filter.should_exclude("README.md"));
        // Excluded by extension (.rs from exclude_extensions)
        assert!(filter.should_exclude("src/main.rs"));
        // Excluded by glob (docs/**)
        assert!(filter.should_exclude("docs/config.yaml"));
        // Not excluded
        assert!(!filter.should_exclude("config/app.yaml"));
    }

    #[test]
    fn test_filter_is_active() {
        let filter1 = SearchFilterOptions {
            exclude_document_files: true,
            ..Default::default()
        };
        assert!(filter1.is_active());

        let mut filter2 = SearchFilterOptions::default();
        filter2.exclude_extensions.insert(".md".to_string());
        assert!(filter2.is_active());

        let filter3 = SearchFilterOptions {
            exclude_globs: vec!["docs/**".to_string()],
            ..Default::default()
        };
        assert!(filter3.is_active());

        let filter4 = SearchFilterOptions::default();
        assert!(!filter4.is_active());
    }

    #[test]
    fn test_get_filename() {
        assert_eq!(get_filename("README"), Some("readme".to_string()));
        assert_eq!(get_filename("src/README"), Some("readme".to_string()));
        assert_eq!(get_filename("docs/CHANGELOG"), Some("changelog".to_string()));
        assert_eq!(get_filename("README.md"), Some("readme".to_string()));
        assert_eq!(get_filename("src/main.rs"), Some("main".to_string()));
        assert_eq!(get_filename(".gitignore"), Some(".gitignore".to_string()));
    }

    #[test]
    fn test_exclude_document_filenames() {
        let mut filter = SearchFilterOptions {
            exclude_document_files: true,
            ..Default::default()
        };
        // Manually populate for this test
        for name in DEFAULT_DOCUMENT_FILENAMES {
            filter.exclude_filenames.insert(name.to_lowercase());
        }

        // 无扩展名文档文件应该被排除
        assert!(filter.should_exclude("README"));
        assert!(filter.should_exclude("docs/README"));
        assert!(filter.should_exclude("CHANGELOG"));
        assert!(filter.should_exclude("TODO"));
        assert!(filter.should_exclude("ROADMAP"));

        // 有扩展名的文档文件也应该被排除（通过扩展名）
        for ext in DEFAULT_DOCUMENT_EXTENSIONS {
            filter.exclude_extensions.insert(ext.to_string());
        }
        assert!(filter.should_exclude("README.md"));
        assert!(filter.should_exclude("docs/guide.txt"));

        // 普通源码文件不应被排除
        assert!(!filter.should_exclude("src/main.rs"));
        assert!(!filter.should_exclude("lib/controller.py"));
    }

    #[test]
    fn test_from_args_populates_filenames() {
        use crate::tools::search_context::SearchContextArgs;

        let args = SearchContextArgs {
            project_root_path: Some("/path".to_string()),
            query: Some("test".to_string()),
            exclude_document_files: Some(true),
            exclude_extensions: None,
            exclude_globs: None,
        };

        let filter = SearchFilterOptions::from_args(&args);

        // 验证默认文件名被注入
        assert!(filter.exclude_filenames.contains("readme"));
        assert!(filter.exclude_filenames.contains("changelog"));
        assert!(filter.exclude_filenames.contains("todo"));
        assert!(filter.exclude_filenames.contains("roadmap"));
    }
}
