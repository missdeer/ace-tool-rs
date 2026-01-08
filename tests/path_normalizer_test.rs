//! Tests for path_normalizer module

use std::path::{Path, PathBuf};

use ace_tool::utils::path_normalizer::{
    build_wsl_unc, is_wsl_mnt_path, is_wsl_unc_path, normalize_path, normalize_relative_path,
    parse_wsl_unc, win_to_wsl, wsl_to_win, NormalizedPath, RuntimeEnv, WslUncPath,
};

// ==================== RuntimeEnv Tests ====================

#[test]
fn test_runtime_env_detect_returns_valid_variant() {
    let env = RuntimeEnv::detect();
    // Should return one of the valid variants
    assert!(matches!(
        env,
        RuntimeEnv::Windows | RuntimeEnv::WslNative | RuntimeEnv::Unix
    ));
}

#[test]
fn test_runtime_env_is_copy() {
    let env = RuntimeEnv::Windows;
    let env2 = env;
    assert_eq!(env, env2);
}

#[test]
fn test_runtime_env_debug() {
    assert_eq!(format!("{:?}", RuntimeEnv::Windows), "Windows");
    assert_eq!(format!("{:?}", RuntimeEnv::WslNative), "WslNative");
    assert_eq!(format!("{:?}", RuntimeEnv::Unix), "Unix");
}

#[test]
fn test_runtime_env_equality() {
    assert_eq!(RuntimeEnv::Windows, RuntimeEnv::Windows);
    assert_eq!(RuntimeEnv::WslNative, RuntimeEnv::WslNative);
    assert_eq!(RuntimeEnv::Unix, RuntimeEnv::Unix);
    assert_ne!(RuntimeEnv::Windows, RuntimeEnv::Unix);
    assert_ne!(RuntimeEnv::Windows, RuntimeEnv::WslNative);
    assert_ne!(RuntimeEnv::Unix, RuntimeEnv::WslNative);
}

// ==================== win_to_wsl Tests ====================

#[test]
fn test_win_to_wsl_basic() {
    assert_eq!(
        win_to_wsl("C:\\Users\\foo"),
        Some("/mnt/c/Users/foo".to_string())
    );
    assert_eq!(
        win_to_wsl("D:\\Projects\\test"),
        Some("/mnt/d/Projects/test".to_string())
    );
}

#[test]
fn test_win_to_wsl_drive_only() {
    assert_eq!(win_to_wsl("C:"), Some("/mnt/c".to_string()));
    assert_eq!(win_to_wsl("D:"), Some("/mnt/d".to_string()));
    assert_eq!(win_to_wsl("Z:"), Some("/mnt/z".to_string()));
}

#[test]
fn test_win_to_wsl_drive_with_slash() {
    assert_eq!(win_to_wsl("C:\\"), Some("/mnt/c/".to_string()));
    assert_eq!(win_to_wsl("C:/"), Some("/mnt/c/".to_string()));
}

#[test]
fn test_win_to_wsl_uppercase_drive() {
    assert_eq!(win_to_wsl("C:\\Users"), Some("/mnt/c/Users".to_string()));
    assert_eq!(win_to_wsl("D:\\Data"), Some("/mnt/d/Data".to_string()));
}

#[test]
fn test_win_to_wsl_lowercase_drive() {
    assert_eq!(win_to_wsl("c:\\Users"), Some("/mnt/c/Users".to_string()));
    assert_eq!(win_to_wsl("d:\\Data"), Some("/mnt/d/Data".to_string()));
}

#[test]
fn test_win_to_wsl_forward_slashes() {
    assert_eq!(
        win_to_wsl("C:/Users/foo"),
        Some("/mnt/c/Users/foo".to_string())
    );
}

#[test]
fn test_win_to_wsl_mixed_slashes() {
    assert_eq!(
        win_to_wsl("C:\\Users/foo\\bar"),
        Some("/mnt/c/Users/foo/bar".to_string())
    );
}

#[test]
fn test_win_to_wsl_deep_path() {
    assert_eq!(
        win_to_wsl("C:\\a\\b\\c\\d\\e\\f\\g"),
        Some("/mnt/c/a/b/c/d/e/f/g".to_string())
    );
}

#[test]
fn test_win_to_wsl_with_spaces() {
    assert_eq!(
        win_to_wsl("C:\\Program Files\\App"),
        Some("/mnt/c/Program Files/App".to_string())
    );
}

#[test]
fn test_win_to_wsl_with_unicode() {
    assert_eq!(
        win_to_wsl("C:\\用户\\文档"),
        Some("/mnt/c/用户/文档".to_string())
    );
}

#[test]
fn test_win_to_wsl_invalid_paths() {
    assert_eq!(win_to_wsl("/home/user"), None);
    assert_eq!(win_to_wsl("/mnt/c/Users"), None);
    assert_eq!(win_to_wsl("relative/path"), None);
    assert_eq!(win_to_wsl(""), None);
    assert_eq!(win_to_wsl("C"), None);
    assert_eq!(win_to_wsl("C:foo"), None);
    assert_eq!(win_to_wsl("C:foo\\bar"), None);
}

#[test]
fn test_win_to_wsl_all_drive_letters() {
    for c in 'A'..='Z' {
        let path = format!("{}:\\test", c);
        let expected = format!("/mnt/{}/test", c.to_ascii_lowercase());
        assert_eq!(win_to_wsl(&path), Some(expected));
    }
}

// ==================== wsl_to_win Tests ====================

#[test]
fn test_wsl_to_win_basic() {
    assert_eq!(
        wsl_to_win("/mnt/c/Users/foo"),
        Some("C:\\Users\\foo".to_string())
    );
    assert_eq!(
        wsl_to_win("/mnt/d/Projects/test"),
        Some("D:\\Projects\\test".to_string())
    );
}

#[test]
fn test_wsl_to_win_drive_only() {
    assert_eq!(wsl_to_win("/mnt/c"), Some("C:".to_string()));
    assert_eq!(wsl_to_win("/mnt/d"), Some("D:".to_string()));
    assert_eq!(wsl_to_win("/mnt/z"), Some("Z:".to_string()));
}

#[test]
fn test_wsl_to_win_drive_with_slash() {
    assert_eq!(wsl_to_win("/mnt/c/"), Some("C:\\".to_string()));
}

#[test]
fn test_wsl_to_win_deep_path() {
    assert_eq!(
        wsl_to_win("/mnt/c/a/b/c/d/e"),
        Some("C:\\a\\b\\c\\d\\e".to_string())
    );
}

#[test]
fn test_wsl_to_win_with_spaces() {
    assert_eq!(
        wsl_to_win("/mnt/c/Program Files/App"),
        Some("C:\\Program Files\\App".to_string())
    );
}

#[test]
fn test_wsl_to_win_with_unicode() {
    assert_eq!(
        wsl_to_win("/mnt/c/用户/文档"),
        Some("C:\\用户\\文档".to_string())
    );
}

#[test]
fn test_wsl_to_win_invalid_paths() {
    assert_eq!(wsl_to_win("/home/user"), None);
    assert_eq!(wsl_to_win("/mnt/cache"), None); // "cache" is not a single letter
    assert_eq!(wsl_to_win("/mnt/cache/data"), None);
    assert_eq!(wsl_to_win("/mnt/"), None);
    assert_eq!(wsl_to_win("/mnt"), None);
    assert_eq!(wsl_to_win(""), None);
    assert_eq!(wsl_to_win("C:\\Users"), None);
}

#[test]
fn test_wsl_to_win_not_mnt_prefix() {
    assert_eq!(wsl_to_win("/var/log"), None);
    assert_eq!(wsl_to_win("/usr/bin"), None);
    assert_eq!(wsl_to_win("/etc/hosts"), None);
}

#[test]
fn test_wsl_to_win_all_drive_letters() {
    for c in 'a'..='z' {
        let path = format!("/mnt/{}/test", c);
        let expected = format!("{}:\\test", c.to_ascii_uppercase());
        assert_eq!(wsl_to_win(&path), Some(expected));
    }
}

// ==================== Roundtrip Tests ====================

#[test]
fn test_win_to_wsl_to_win_roundtrip() {
    let original = "C:\\Users\\foo\\bar";
    let wsl = win_to_wsl(original).unwrap();
    let back = wsl_to_win(&wsl).unwrap();
    assert_eq!(back, original);
}

#[test]
fn test_wsl_to_win_to_wsl_roundtrip() {
    let original = "/mnt/c/Users/foo/bar";
    let win = wsl_to_win(original).unwrap();
    let back = win_to_wsl(&win).unwrap();
    assert_eq!(back, original);
}

// ==================== parse_wsl_unc Tests ====================

#[test]
fn test_parse_wsl_unc_wsl_dollar() {
    let unc = parse_wsl_unc("\\\\wsl$\\Ubuntu\\home\\user");
    assert!(unc.is_some());
    let unc = unc.unwrap();
    assert_eq!(unc.distro, "Ubuntu");
    assert_eq!(unc.inner_path, "/home/user");
}

#[test]
fn test_parse_wsl_unc_wsl_localhost() {
    let unc = parse_wsl_unc("\\\\wsl.localhost\\Debian\\var\\log");
    assert!(unc.is_some());
    let unc = unc.unwrap();
    assert_eq!(unc.distro, "Debian");
    assert_eq!(unc.inner_path, "/var/log");
}

#[test]
fn test_parse_wsl_unc_root_path() {
    let unc = parse_wsl_unc("\\\\wsl$\\Ubuntu\\");
    assert!(unc.is_some());
    let unc = unc.unwrap();
    assert_eq!(unc.distro, "Ubuntu");
    assert_eq!(unc.inner_path, "/");
}

#[test]
fn test_parse_wsl_unc_distro_only() {
    let unc = parse_wsl_unc("\\\\wsl$\\Ubuntu");
    assert!(unc.is_some());
    let unc = unc.unwrap();
    assert_eq!(unc.distro, "Ubuntu");
    assert_eq!(unc.inner_path, "/");
}

#[test]
fn test_parse_wsl_unc_with_forward_slashes() {
    let unc = parse_wsl_unc("//wsl$/Ubuntu/home/user");
    assert!(unc.is_some());
    let unc = unc.unwrap();
    assert_eq!(unc.distro, "Ubuntu");
    assert_eq!(unc.inner_path, "/home/user");
}

#[test]
fn test_parse_wsl_unc_case_insensitive() {
    let unc = parse_wsl_unc("\\\\WSL$\\Ubuntu\\home\\user");
    assert!(unc.is_some());
    let unc = unc.unwrap();
    assert_eq!(unc.distro, "Ubuntu");
    assert_eq!(unc.inner_path, "/home/user");
}

#[test]
fn test_parse_wsl_unc_deep_path() {
    let unc = parse_wsl_unc("\\\\wsl$\\Ubuntu\\a\\b\\c\\d\\e");
    assert!(unc.is_some());
    let unc = unc.unwrap();
    assert_eq!(unc.distro, "Ubuntu");
    assert_eq!(unc.inner_path, "/a/b/c/d/e");
}

#[test]
fn test_parse_wsl_unc_with_spaces() {
    let unc = parse_wsl_unc("\\\\wsl$\\Ubuntu\\home\\my user\\documents");
    assert!(unc.is_some());
    let unc = unc.unwrap();
    assert_eq!(unc.distro, "Ubuntu");
    assert_eq!(unc.inner_path, "/home/my user/documents");
}

#[test]
fn test_parse_wsl_unc_various_distros() {
    let distros = [
        "Ubuntu",
        "Debian",
        "Alpine",
        "openSUSE-Leap-15.2",
        "Ubuntu-20.04",
    ];
    for distro in distros {
        let path = format!("\\\\wsl$\\{}\\home", distro);
        let unc = parse_wsl_unc(&path);
        assert!(unc.is_some(), "Failed for distro: {}", distro);
        assert_eq!(unc.unwrap().distro, distro);
    }
}

#[test]
fn test_parse_wsl_unc_invalid_paths() {
    assert!(parse_wsl_unc("C:\\Users\\foo").is_none());
    assert!(parse_wsl_unc("/home/user").is_none());
    assert!(parse_wsl_unc("\\\\server\\share").is_none());
    assert!(parse_wsl_unc("").is_none());
    assert!(parse_wsl_unc("\\\\wsl$\\").is_none());
    assert!(parse_wsl_unc("\\\\wsl$").is_none());
    assert!(parse_wsl_unc("\\\\wsl$\\\\home").is_none());
}

// ==================== build_wsl_unc Tests ====================

#[test]
fn test_build_wsl_unc_basic() {
    assert_eq!(
        build_wsl_unc("Ubuntu", "/home/user"),
        "\\\\wsl.localhost\\Ubuntu\\home\\user"
    );
}

#[test]
fn test_build_wsl_unc_root() {
    assert_eq!(build_wsl_unc("Ubuntu", "/"), "\\\\wsl.localhost\\Ubuntu\\");
}

#[test]
fn test_build_wsl_unc_deep_path() {
    assert_eq!(
        build_wsl_unc("Debian", "/var/log/syslog"),
        "\\\\wsl.localhost\\Debian\\var\\log\\syslog"
    );
}

#[test]
fn test_build_wsl_unc_relative_path() {
    assert_eq!(
        build_wsl_unc("Ubuntu", "home/user"),
        "\\\\wsl.localhost\\Ubuntu\\home\\user"
    );
}

#[test]
fn test_build_wsl_unc_with_spaces() {
    assert_eq!(
        build_wsl_unc("Ubuntu", "/home/my user/docs"),
        "\\\\wsl.localhost\\Ubuntu\\home\\my user\\docs"
    );
}

#[test]
fn test_build_wsl_unc_roundtrip() {
    let distro = "Ubuntu";
    let path = "/home/user/project";
    let unc = build_wsl_unc(distro, path);
    let parsed = parse_wsl_unc(&unc).unwrap();
    assert_eq!(parsed.distro, distro);
    assert_eq!(parsed.inner_path, path);
}

// ==================== is_wsl_unc_path Tests ====================

#[test]
fn test_is_wsl_unc_path_wsl_dollar_backslash() {
    assert!(is_wsl_unc_path("\\\\wsl$\\Ubuntu\\home"));
    assert!(is_wsl_unc_path("\\\\wsl$\\Debian"));
}

#[test]
fn test_is_wsl_unc_path_wsl_localhost_backslash() {
    assert!(is_wsl_unc_path("\\\\wsl.localhost\\Ubuntu\\home"));
    assert!(is_wsl_unc_path("\\\\wsl.localhost\\Debian"));
}

#[test]
fn test_is_wsl_unc_path_forward_slashes() {
    assert!(is_wsl_unc_path("//wsl$/Ubuntu/home"));
    assert!(is_wsl_unc_path("//wsl.localhost/Ubuntu/home"));
}

#[test]
fn test_is_wsl_unc_path_case_insensitive() {
    assert!(is_wsl_unc_path("\\\\WSL$\\Ubuntu"));
    assert!(is_wsl_unc_path("\\\\WsL.LoCaLhOsT\\Ubuntu"));
}

#[test]
fn test_is_wsl_unc_path_invalid() {
    assert!(!is_wsl_unc_path("C:\\Users\\foo"));
    assert!(!is_wsl_unc_path("/home/user"));
    assert!(!is_wsl_unc_path("\\\\server\\share"));
    assert!(!is_wsl_unc_path(""));
}

// ==================== is_wsl_mnt_path Tests ====================

#[test]
fn test_is_wsl_mnt_path_valid() {
    assert!(is_wsl_mnt_path("/mnt/c"));
    assert!(is_wsl_mnt_path("/mnt/c/"));
    assert!(is_wsl_mnt_path("/mnt/c/Users"));
    assert!(is_wsl_mnt_path("/mnt/d/Projects"));
    assert!(is_wsl_mnt_path("/mnt/z"));
}

#[test]
fn test_is_wsl_mnt_path_all_drives() {
    for c in 'a'..='z' {
        let path = format!("/mnt/{}", c);
        assert!(is_wsl_mnt_path(&path), "Failed for drive: {}", c);
        let path_with_subdir = format!("/mnt/{}/test", c);
        assert!(
            is_wsl_mnt_path(&path_with_subdir),
            "Failed for drive: {}",
            c
        );
    }
}

#[test]
fn test_is_wsl_mnt_path_invalid_multi_letter() {
    assert!(!is_wsl_mnt_path("/mnt/cache"));
    assert!(!is_wsl_mnt_path("/mnt/cache/data"));
    assert!(!is_wsl_mnt_path("/mnt/wsl"));
    assert!(!is_wsl_mnt_path("/mnt/ab"));
}

#[test]
fn test_is_wsl_mnt_path_invalid_format() {
    assert!(!is_wsl_mnt_path("/home/user"));
    assert!(!is_wsl_mnt_path("/mnt/"));
    assert!(!is_wsl_mnt_path("/mnt"));
    assert!(!is_wsl_mnt_path(""));
    assert!(!is_wsl_mnt_path("/mnt/c2"));
    assert!(!is_wsl_mnt_path("C:\\Users"));
}

#[test]
fn test_is_wsl_mnt_path_edge_cases() {
    assert!(!is_wsl_mnt_path("/mnt/1")); // digit, not letter
    assert!(is_wsl_mnt_path("/mnt/C")); // uppercase is also valid (is_ascii_alphabetic)
    assert!(is_wsl_mnt_path("/mnt/c/a/b/c/d/e/f")); // deep path
}

// ==================== normalize_path Tests ====================

#[test]
fn test_normalize_path_windows_regular() {
    let path = Path::new("C:\\Users\\foo\\bar");
    let result = normalize_path(path, RuntimeEnv::Windows);
    assert_eq!(result.canonical, "C:/Users/foo/bar");
    assert_eq!(result.local, path);
}

#[test]
fn test_normalize_path_windows_unc() {
    let path = Path::new("\\\\wsl$\\Ubuntu\\home\\user");
    let result = normalize_path(path, RuntimeEnv::Windows);
    assert_eq!(result.canonical, "/home/user");
    assert_eq!(result.local, path);
}

#[test]
fn test_normalize_path_wsl_mnt() {
    let path = Path::new("/mnt/c/Users/foo");
    let result = normalize_path(path, RuntimeEnv::WslNative);
    assert_eq!(result.canonical, "/mnt/c/Users/foo");
    assert_eq!(result.local, path);
}

#[test]
fn test_normalize_path_wsl_native() {
    let path = Path::new("/home/user/project");
    let result = normalize_path(path, RuntimeEnv::WslNative);
    assert_eq!(result.canonical, "/home/user/project");
    assert_eq!(result.local, path);
}

#[test]
fn test_normalize_path_wsl_native_from_windows_path() {
    let path = Path::new("C:\\Users\\foo");
    let result = normalize_path(path, RuntimeEnv::WslNative);
    assert_eq!(result.canonical, "/mnt/c/Users/foo");
    assert_eq!(result.local, PathBuf::from("/mnt/c/Users/foo"));
}

#[test]
fn test_normalize_path_unix() {
    let path = Path::new("/home/user/project");
    let result = normalize_path(path, RuntimeEnv::Unix);
    assert_eq!(result.canonical, "/home/user/project");
    assert_eq!(result.local, path);
}

#[test]
fn test_normalize_path_unix_with_backslash() {
    // Edge case: Unix path somehow contains backslash
    let path = Path::new("/home/user\\project");
    let result = normalize_path(path, RuntimeEnv::Unix);
    assert_eq!(result.canonical, "/home/user/project");
}

// ==================== normalize_relative_path Tests ====================

#[test]
fn test_normalize_relative_path_backslashes() {
    assert_eq!(normalize_relative_path("src\\main.rs"), "src/main.rs");
    assert_eq!(normalize_relative_path("a\\b\\c\\d"), "a/b/c/d");
}

#[test]
fn test_normalize_relative_path_forward_slashes() {
    assert_eq!(normalize_relative_path("src/main.rs"), "src/main.rs");
    assert_eq!(normalize_relative_path("a/b/c/d"), "a/b/c/d");
}

#[test]
fn test_normalize_relative_path_mixed() {
    assert_eq!(normalize_relative_path("src\\lib/mod.rs"), "src/lib/mod.rs");
    assert_eq!(normalize_relative_path("a/b\\c/d\\e"), "a/b/c/d/e");
}

#[test]
fn test_normalize_relative_path_empty() {
    assert_eq!(normalize_relative_path(""), "");
}

#[test]
fn test_normalize_relative_path_no_slashes() {
    assert_eq!(normalize_relative_path("file.txt"), "file.txt");
}

#[test]
fn test_normalize_relative_path_with_spaces() {
    assert_eq!(
        normalize_relative_path("my folder\\my file.txt"),
        "my folder/my file.txt"
    );
}

#[test]
fn test_normalize_relative_path_with_unicode() {
    assert_eq!(
        normalize_relative_path("文件夹\\文件.txt"),
        "文件夹/文件.txt"
    );
}

#[test]
fn test_normalize_relative_path_consecutive_slashes() {
    // Should preserve consecutive slashes (just convert type)
    assert_eq!(normalize_relative_path("a\\\\b"), "a//b");
    assert_eq!(normalize_relative_path("a//b"), "a//b");
}

// ==================== NormalizedPath Tests ====================

#[test]
fn test_normalized_path_clone() {
    let np = NormalizedPath {
        canonical: "/home/user".to_string(),
        local: PathBuf::from("/home/user"),
    };
    let cloned = np.clone();
    assert_eq!(np.canonical, cloned.canonical);
    assert_eq!(np.local, cloned.local);
}

#[test]
fn test_normalized_path_debug() {
    let np = NormalizedPath {
        canonical: "/home/user".to_string(),
        local: PathBuf::from("/home/user"),
    };
    let debug = format!("{:?}", np);
    assert!(debug.contains("NormalizedPath"));
    assert!(debug.contains("canonical"));
    assert!(debug.contains("local"));
}

// ==================== WslUncPath Tests ====================

#[test]
fn test_wsl_unc_path_clone() {
    let unc = WslUncPath {
        distro: "Ubuntu".to_string(),
        inner_path: "/home/user".to_string(),
    };
    let cloned = unc.clone();
    assert_eq!(unc.distro, cloned.distro);
    assert_eq!(unc.inner_path, cloned.inner_path);
}

#[test]
fn test_wsl_unc_path_debug() {
    let unc = WslUncPath {
        distro: "Ubuntu".to_string(),
        inner_path: "/home/user".to_string(),
    };
    let debug = format!("{:?}", unc);
    assert!(debug.contains("WslUncPath"));
    assert!(debug.contains("Ubuntu"));
    assert!(debug.contains("/home/user"));
}

// ==================== Edge Cases and Special Characters ====================

#[test]
fn test_paths_with_dots() {
    assert_eq!(
        win_to_wsl("C:\\Users\\..\\foo"),
        Some("/mnt/c/Users/../foo".to_string())
    );
    assert_eq!(
        wsl_to_win("/mnt/c/Users/../foo"),
        Some("C:\\Users\\..\\foo".to_string())
    );
}

#[test]
fn test_paths_with_special_chars() {
    assert_eq!(
        win_to_wsl("C:\\Users\\foo@bar#baz"),
        Some("/mnt/c/Users/foo@bar#baz".to_string())
    );
}

#[test]
fn test_very_long_path() {
    let long_segment = "a".repeat(100);
    let win_path = format!("C:\\{}\\{}\\{}", long_segment, long_segment, long_segment);
    let result = win_to_wsl(&win_path);
    assert!(result.is_some());
    assert!(result.unwrap().starts_with("/mnt/c/"));
}
