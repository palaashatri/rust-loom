//! Directory walking: depth, entry limits, symlinks, unreadable directories,
//! and the persistent cache's change detection.

mod common;

use common::{inter, rename_family, write_family_variants, TempDir};
use loom_fonts::{FallbackPolicy, FontCatalog, ScanCache, ScanConfig, ScanLimits};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

fn config(dirs: Vec<std::path::PathBuf>) -> ScanConfig {
    let mut config = ScanConfig::with_dirs(dirs);
    config.include_bundled = false;
    config.policy = FallbackPolicy::new(Vec::<String>::new());
    config
}

fn write_font(path: &Path, family: &str) {
    fs::write(path, rename_family(&inter("Inter-Regular.ttf"), family)).unwrap();
}

#[test]
fn a_missing_directory_is_neither_an_issue_nor_a_truncation() {
    let catalog = FontCatalog::scan(&config(vec!["/definitely/not/here".into()]));
    assert!(catalog.report().issues.is_empty());
    assert!(!catalog.report().truncated);
}

#[test]
fn max_depth_limits_how_deep_the_scan_goes_and_says_so() {
    let dir = TempDir::new("depth");
    let deep = dir.path().join("a").join("b");
    fs::create_dir_all(&deep).unwrap();
    write_font(&dir.path().join("top.ttf"), "Topfa");
    write_font(&dir.path().join("a").join("one.ttf"), "Onefa");
    write_font(&deep.join("two.ttf"), "Twofa");

    let mut cfg = config(vec![dir.path().to_owned()]);
    cfg.limits = ScanLimits {
        max_depth: 1,
        ..ScanLimits::default()
    };
    let shallow = FontCatalog::scan(&cfg);
    assert!(shallow.has_family("Topfa") && shallow.has_family("Onefa"));
    assert!(!shallow.has_family("Twofa"), "depth 2 is past the limit");
    assert!(
        shallow.report().truncated,
        "a depth limit that skipped a directory makes the scan partial"
    );

    cfg.limits.max_depth = 0;
    let root_only = FontCatalog::scan(&cfg);
    assert!(root_only.has_family("Topfa") && !root_only.has_family("Onefa"));

    cfg.limits.max_depth = 2;
    let full = FontCatalog::scan(&cfg);
    assert!(full.has_family("Twofa"));
    assert!(!full.report().truncated);
}

#[test]
fn entries_are_streamed_and_counted_against_the_entry_limit() {
    let dir = TempDir::new("entries");
    for n in 0..200 {
        fs::write(dir.path().join(format!("note{n:03}.txt")), b"x").unwrap();
    }
    write_font(&dir.path().join("late.ttf"), "Latef");
    let mut cfg = config(vec![dir.path().to_owned()]);
    cfg.limits = ScanLimits {
        max_entries: 50,
        ..ScanLimits::default()
    };
    let catalog = FontCatalog::scan(&cfg);
    let report = catalog.report();
    assert!(report.truncated);
    assert_eq!(
        report.entries_examined, 50,
        "stopped after the limit, not after reading all 201 entries"
    );

    // Without the limit every entry is examined.
    cfg.limits = ScanLimits::default();
    let report = FontCatalog::scan(&cfg).report().clone();
    assert_eq!(report.entries_examined, 201);
    assert!(!report.truncated);
}

#[test]
fn the_font_file_limit_applies_inside_a_directory_listing() {
    let dir = TempDir::new("inside");
    write_family_variants(dir.path(), 40);
    let mut cfg = config(vec![dir.path().to_owned()]);
    cfg.limits = ScanLimits {
        max_files: 7,
        ..ScanLimits::default()
    };
    let catalog = FontCatalog::scan(&cfg);
    assert!(catalog.report().truncated);
    assert_eq!(catalog.report().files_seen, 7);
    assert!(
        catalog.report().entries_examined <= 8,
        "stops reading the directory once the limit is hit: {}",
        catalog.report().entries_examined
    );
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn symlinked_directories_are_followed_once_and_loops_terminate() {
        let root = TempDir::new("links");
        let outside = TempDir::new("links-outside");
        let real = root.path().join("real");
        fs::create_dir(&real).unwrap();
        write_font(&real.join("a.ttf"), "Realf");
        write_font(&outside.path().join("e.ttf"), "Outsf");
        // A link to a sibling directory, a link out of the tree, and a link
        // back to an ancestor (a cycle).
        symlink(&real, root.path().join("alias")).unwrap();
        symlink(outside.path(), root.path().join("elsewhere")).unwrap();
        symlink(root.path(), real.join("up")).unwrap();

        let catalog = FontCatalog::scan(&config(vec![root.path().to_owned()]));
        assert!(catalog.has_family("Realf"));
        assert!(
            catalog.has_family("Outsf"),
            "a link leaving the tree is followed"
        );
        let report = catalog.report();
        assert_eq!(
            report.files_seen, 2,
            "the aliased directory and the cycle are visited once: {report:?}"
        );
        assert!(!report.truncated);
    }

    #[test]
    fn a_symlinked_font_file_is_catalogued() {
        let root = TempDir::new("filelink");
        let elsewhere = TempDir::new("filelink-target");
        write_font(&elsewhere.path().join("target.ttf"), "Linkf");
        symlink(
            elsewhere.path().join("target.ttf"),
            root.path().join("linked.ttf"),
        )
        .unwrap();
        // A dangling link is skipped quietly.
        symlink(
            elsewhere.path().join("missing.ttf"),
            root.path().join("dangling.ttf"),
        )
        .unwrap();
        let catalog = FontCatalog::scan(&config(vec![root.path().to_owned()]));
        assert!(catalog.has_family("Linkf"));
        assert_eq!(catalog.report().files_seen, 1);
        assert!(catalog.report().issues.is_empty());
    }

    #[test]
    fn an_unreadable_directory_is_reported_not_silently_skipped() {
        let root = TempDir::new("denied");
        let locked = root.path().join("locked");
        fs::create_dir(&locked).unwrap();
        write_font(&root.path().join("ok.ttf"), "Okayf");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let readable = fs::read_dir(&locked).is_ok();
        let catalog = FontCatalog::scan(&config(vec![root.path().to_owned()]));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if readable {
            eprintln!("skipped: permissions are not enforced (running as root)");
            return;
        }
        assert!(catalog.has_family("Okayf"));
        let issues = &catalog.report().issues;
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].path, locked);
        assert!(!issues[0].reason.is_empty());
    }
}

#[test]
fn a_new_size_alone_invalidates_the_cached_entry() {
    let dir = TempDir::new("size");
    let path = dir.path().join("grow.ttf");
    write_font(&path, "Sizea");
    let stamp = fs::metadata(&path).unwrap().modified().unwrap();
    let mut cache = ScanCache::new();
    let cfg = config(vec![dir.path().to_owned()]);
    FontCatalog::scan_with_cache(&cfg, &mut cache);

    // Different length (trailing padding), same modification time.
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(&[0; 16]);
    fs::write(&path, bytes).unwrap();
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(stamp).unwrap();
    drop(file);
    let again = FontCatalog::scan_with_cache(&cfg, &mut cache);
    assert_eq!(again.report().files_parsed, 1);
    assert_eq!(again.report().cache_hits, 0);
}

#[test]
fn a_new_modification_time_alone_invalidates_the_cached_entry() {
    let dir = TempDir::new("mtime");
    let path = dir.path().join("same.ttf");
    write_font(&path, "First");
    let mut cache = ScanCache::new();
    let cfg = config(vec![dir.path().to_owned()]);
    let first = FontCatalog::scan_with_cache(&cfg, &mut cache);
    assert!(first.has_family("First"));

    // Same length, different content, different timestamp.
    write_font(&path, "Secnd");
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(3600))
        .unwrap();
    drop(file);
    let second = FontCatalog::scan_with_cache(&cfg, &mut cache);
    assert_eq!(second.report().files_parsed, 1);
    assert_eq!(second.report().cache_hits, 0);
    assert!(second.has_family("Secnd") && !second.has_family("First"));

    // Unchanged again: answered from the cache.
    let third = FontCatalog::scan_with_cache(&cfg, &mut cache);
    assert_eq!(third.report().cache_hits, 1);
    assert_eq!(third.report().files_parsed, 0);
}
