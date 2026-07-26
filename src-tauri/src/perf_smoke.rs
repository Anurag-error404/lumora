//! One-shot performance smoke harness (not a CI gate).
//! Run: `cargo test --manifest-path src-tauri/Cargo.toml perf_smoke -- --nocapture --ignored`

use std::time::Instant;

use image::{Rgb, RgbImage};
use rusqlite::params;
use tempfile::tempdir;

use crate::db;
use crate::duplicates;
use crate::indexer;
use crate::search;
use crate::thumbnails;
use crate::views;

#[test]
#[ignore = "manual perf smoke — run with --ignored --nocapture"]
fn perf_smoke_measure_and_print() {
    let dir = tempdir().unwrap();
    let media = dir.path().join("media");
    let thumbs = dir.path().join("thumbs");
    std::fs::create_dir_all(&media).unwrap();
    std::fs::create_dir_all(&thumbs).unwrap();

    const N: usize = 100;
    for i in 0..N {
        let img = RgbImage::from_fn(480, 320, |x, y| {
            Rgb([
                ((x + i as u32) % 256) as u8,
                ((y + i as u32 * 3) % 256) as u8,
                ((x * y + i as u32) % 256) as u8,
            ])
        });
        img.save(media.join(format!("photo{i:04}.png"))).unwrap();
    }

    // Thumbnail throughput (decode + resize JPEG write)
    let thumb_start = Instant::now();
    for i in 0..N {
        let src = media.join(format!("photo{i:04}.png"));
        thumbnails::generate_thumbnail(&src, &thumbs, &format!("hash{i}")).unwrap();
    }
    let thumb_secs = thumb_start.elapsed().as_secs_f64();
    let thumbs_per_min = (N as f64 / thumb_secs) * 60.0;

    // Real import path seeds FTS the same way production does
    let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
    indexer::import_folder_with_progress(&conn, &media, &thumbs, |_, _, _| {}).unwrap();

    // Tag a few for richer FTS
    for i in (0..N).step_by(10) {
        let path = media.join(format!("photo{i:04}.png"));
        let path_str = path.to_string_lossy().to_string();
        conn.execute(
            "UPDATE assets SET camera = 'Canon EOS' WHERE path = ?1",
            params![path_str],
        )
        .unwrap();
    }
    // Refresh FTS for camera updates
    let ids: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT id FROM assets WHERE deleted_at IS NULL")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };
    for id in &ids {
        let _ = indexer::refresh_fts(&conn, id);
    }

    // Warm + measure FTS text search (camera token is refreshed into FTS above)
    let sample = search::search_assets(&conn, "Canon", 50, 0).unwrap();
    let search_start = Instant::now();
    let mut hit_count = 0usize;
    for _ in 0..20 {
        let rows = search::search_assets(&conn, "Canon", 50, 0).unwrap();
        hit_count = rows.len();
    }
    let search_ms = search_start.elapsed().as_secs_f64() * 1000.0 / 20.0;

    let filter_start = Instant::now();
    for _ in 0..20 {
        let _ = search::search_assets(&conn, "camera:Canon", 50, 0).unwrap();
    }
    let filter_ms = filter_start.elapsed().as_secs_f64() * 1000.0 / 20.0;

    // Near-dup clustering on imported perceptual hashes
    let near_start = Instant::now();
    let groups = duplicates::find_near_duplicates(&conn).unwrap();
    let near_ms = near_start.elapsed().as_secs_f64() * 1000.0;

    if let Some(first) = ids.first() {
        views::record_view(&conn, first).unwrap();
        let recent = views::list_recently_viewed(&conn, 10, 0).unwrap();
        assert_eq!(recent[0].id, *first);
    }

    println!("PERF_SMOKE library_size={N}");
    println!("PERF_SMOKE search_hits={hit_count}");
    println!(
        "PERF_SMOKE sample_first={}",
        sample.first().map(|a| a.path.clone()).unwrap_or_default()
    );
    println!("PERF_SMOKE thumbs_per_min={thumbs_per_min:.1}");
    println!("PERF_SMOKE search_ms={search_ms:.2}");
    println!("PERF_SMOKE filter_ms={filter_ms:.2}");
    println!("PERF_SMOKE near_dup_ms={near_ms:.2}");
    println!("PERF_SMOKE near_dup_groups={}", groups.len());
    println!("PERF_SMOKE thumb_wall_secs={thumb_secs:.2}");

    assert!(
        thumbs_per_min >= 100.0,
        "thumb throughput {thumbs_per_min:.1}/min below goal of 100/min"
    );
    assert!(
        search_ms < 100.0,
        "FTS search {search_ms:.2}ms above goal of 100ms"
    );
    assert!(hit_count > 0, "expected FTS hits for Canon");
}
