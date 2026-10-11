use std::path::{Path, PathBuf};

fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for directory in ["src", "tests", "examples"] {
        rust_files(&root.join(directory), &mut files);
    }
    files
}

#[test]
fn rust_files_use_snake_case_and_remain_within_the_module_size_limit() {
    for path in sources() {
        let stem = path.file_stem().unwrap().to_str().unwrap();
        assert!(
            stem.bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
            "non-standard Rust file name: {}",
            path.display()
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.lines().count() <= 350,
            "oversized module: {}",
            path.display()
        );
    }
}

#[test]
fn modules_do_not_suppress_naming_lints_at_crate_or_module_scope() {
    for path in sources() {
        let text = std::fs::read_to_string(&path).unwrap();
        for line in text.lines().map(str::trim) {
            assert!(
                !(line.starts_with("#![allow(")
                    && (line.contains("non_snake_case")
                        || line.contains("non_upper_case_globals"))),
                "blanket naming suppression: {}",
                path.display()
            );
        }
    }
}

#[test]
fn legacy_rust_aliases_preserve_qrc_results_and_diagnostic_entry_points() {
    use qqmusic::{self as plugin, qrc};
    let text = "[0,100]字(0,100)";
    assert_eq!(qrc::parseQrc(text).unwrap(), qrc::parse_qrc(text).unwrap());
    for (legacy, standard) in [
        (
            plugin::diagnoseFlymeAccount as *const (),
            plugin::diagnose_recommendation_account as *const (),
        ),
        (
            plugin::diagnoseRecommendations as *const (),
            plugin::diagnose_recommendations as *const (),
        ),
        (
            plugin::diagnoseSavedSession as *const (),
            plugin::diagnose_saved_session as *const (),
        ),
        (
            plugin::captureLyrics as *const (),
            plugin::capture_lyrics as *const (),
        ),
        (
            plugin::replayLyrics as *const (),
            plugin::replay_lyrics as *const (),
        ),
    ] {
        assert_eq!(legacy, standard);
    }
}
