//! monitor-history integration tests: migrations, writer, rollups,
//! retention, bounded queries, corruption refusal, and a 1M-row load test
//! (ignored; run explicitly on native hardware).

use std::path::PathBuf;

use monitor_history::{
    apply_migrations, clear_samples, compute_rollups, get_history, retain, BatchWriter,
    HistoryQuery, HistoryStore, Migration, Resolution, RollupGranularity, WriteItem,
    ROLLUP_BUCKET_1M_MS,
};
use rusqlite::Connection;

fn temp_db(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "monitor-history-test-{}-{}-{}.sqlite3",
        std::process::id(),
        tag,
        uuid::Uuid::new_v4()
    ))
}

fn cleanup(path: &std::path::Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(p));
    }
}

fn insert_sample(conn: &Connection, ts_utc_ms: i64, value: f64, generation: &str) {
    conn.execute(
        "INSERT INTO samples_raw \
         (host_id, metric_id, entity_id, ts_utc_ms, value_real, source, generation, sequence) \
         VALUES ('host-1', 'cpu.total.usage', 'host', ?1, ?2, 'test', ?3, 1)",
        rusqlite::params![ts_utc_ms, value, generation],
    )
    .unwrap();
}

#[test]
fn fresh_database_reaches_latest_schema() {
    let conn = Connection::open_in_memory().unwrap();
    HistoryStore::with_connection(conn).expect("migrate fresh");
}

#[test]
fn checksum_drift_and_future_db_are_refused() {
    let path = temp_db("drift");
    {
        let store = HistoryStore::open(&path).unwrap();
        drop(store);
    }
    // Drifted migration list with same version but different SQL.
    let drifted = &[Migration {
        version: 1,
        name: monitor_history::MIGRATIONS[0].name,
        sql: "-- tampered",
    }];
    let mut conn = Connection::open(&path).unwrap();
    let err = apply_migrations(&mut conn, drifted).expect_err("drift refused");
    assert!(matches!(
        err,
        monitor_history::StoreError::ChecksumMismatch { .. }
    ));
    drop(conn);

    // Database from the future.
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (version, name, checksum, applied_at_utc) \
         VALUES (99, 'future', 'x', '2026-01-01T00:00:00.000Z')",
        [],
    )
    .unwrap();
    let err = HistoryStore::with_connection(conn).expect_err("future refused");
    assert!(matches!(
        err,
        monitor_history::StoreError::DatabaseTooNew { db: 99, app: 1 }
    ));
    cleanup(&path);
}

#[test]
fn batch_writer_persists_and_counts_drops() {
    let path = temp_db("writer");
    let conn = Connection::open(&path).unwrap();
    let store = HistoryStore::with_connection(conn).unwrap();

    // Hold the writer's connection out of the store so we can query after.
    let raw_conn = Connection::open(&path).unwrap();
    drop(store);

    let writer_conn = Connection::open(&path).unwrap();
    let writer = BatchWriter::spawn(writer_conn).expect("writer spawns with migrations");
    for i in 0..600 {
        writer.enqueue(WriteItem::Sample {
            host_id: "host-1".into(),
            metric_id: "cpu.total.usage".into(),
            entity_id: "host".into(),
            ts_utc_ms: 1_700_000_000_000 + i * 3_000,
            value_real: i as f64,
            source: "test".into(),
            generation: "g".into(),
            sequence: i as u64,
        });
    }
    let stats = writer.close_and_flush();
    assert_eq!(stats.written, 600, "all items flushed on close");
    assert_eq!(stats.dropped_full, 0);

    let count: i64 = raw_conn
        .query_row("SELECT COUNT(*) FROM samples_raw", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 600);
    cleanup(&path);
}

#[test]
fn rollups_are_deterministic_min_max_avg_last_count() {
    let conn = Connection::open_in_memory().unwrap();
    let store = HistoryStore::with_connection(conn).unwrap();
    let conn = store.connection();
    let base: i64 = 1_700_000_100_000; // lands inside a minute bucket
    let bucket = base.div_euclid(ROLLUP_BUCKET_1M_MS) * ROLLUP_BUCKET_1M_MS;
    // 4 samples in the same bucket, inserted out of order to prove the
    // `last` selection is by timestamp, not insertion order.
    insert_sample(conn, base + 30_000, 40.0, "g");
    insert_sample(conn, base + 1_000, 10.0, "g");
    insert_sample(conn, base + 20_000, 30.0, "g");
    insert_sample(conn, base + 10_000, 20.0, "g");

    let written = compute_rollups(
        conn,
        "host-1",
        RollupGranularity::OneMinute,
        bucket,
        bucket + ROLLUP_BUCKET_1M_MS,
    )
    .unwrap();
    assert_eq!(written, 1);

    let (count, ok_count, min, max, avg, last): (i64, i64, f64, f64, f64, f64) = conn
        .query_row(
            "SELECT count, ok_count, min, max, avg, last FROM samples_rollup_1m \
             WHERE host_id='host-1' AND metric_id='cpu.total.usage' AND entity_id='host'",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!((count, ok_count), (4, 4));
    assert_eq!((min, max), (10.0, 40.0));
    assert!((avg - 25.0).abs() < 1e-9);
    assert_eq!(last, 40.0, "last must be the newest timestamp's value");

    // Idempotent recompute: running twice does not double the counts.
    compute_rollups(
        conn,
        "host-1",
        RollupGranularity::OneMinute,
        bucket,
        bucket + ROLLUP_BUCKET_1M_MS,
    )
    .unwrap();
    let count_again: i64 = conn
        .query_row(
            "SELECT count FROM samples_rollup_1m \
             WHERE host_id='host-1' AND metric_id='cpu.total.usage' AND entity_id='host'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count_again, 4);
}

#[test]
fn retention_deletes_only_expired_tiers() {
    let conn = Connection::open_in_memory().unwrap();
    let store = HistoryStore::with_connection(conn).unwrap();
    let conn = store.connection();
    let now: i64 = 1_800_000_000_000;
    // raw: one fresh, one 25h old. 1m: one fresh, one 31d old. 1h likewise.
    insert_sample(conn, now - 1_000, 1.0, "g");
    insert_sample(conn, now - 25 * 3_600_000, 2.0, "g");
    conn.execute(
        "INSERT INTO samples_rollup_1m (host_id, metric_id, entity_id, bucket_utc_ms, count, ok_count) \
         VALUES ('host-1','m','e',?1,1,1),('host-1','m','e',?2,1,1)",
        rusqlite::params![now - 60_000, now - 31 * 86_400_000],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO samples_rollup_1h (host_id, metric_id, entity_id, bucket_utc_ms, count, ok_count) \
         VALUES ('host-1','m','e',?1,1,1),('host-1','m','e',?2,1,1)",
        rusqlite::params![now - 3_600_000, now - 366 * 86_400_000],
    )
    .unwrap();

    let summary = retain(conn, now, 24, 30, 365).unwrap();
    assert_eq!(summary.raw_deleted, 1);
    assert_eq!(summary.rollup_1m_deleted, 1);
    assert_eq!(summary.rollup_1h_deleted, 1);

    let raw_left: i64 = conn
        .query_row("SELECT COUNT(*) FROM samples_raw", [], |r| r.get(0))
        .unwrap();
    assert_eq!(raw_left, 1, "fresh row kept");
}

#[test]
fn clear_samples_wipes_series_but_keeps_schema() {
    let conn = Connection::open_in_memory().unwrap();
    let store = HistoryStore::with_connection(conn).unwrap();
    let conn = store.connection();
    insert_sample(conn, 1_700_000_000_000, 1.0, "g");
    let summary = clear_samples(conn).unwrap();
    assert_eq!(summary.raw_deleted, 1);
    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM samples_raw", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn get_history_is_bounded_reports_gaps_and_missing_ratio() {
    let conn = Connection::open_in_memory().unwrap();
    let store = HistoryStore::with_connection(conn).unwrap();
    let conn = store.connection();

    // Same-generation hole: 60 s of missing samples (MISSING_SAMPLES).
    let base: i64 = 1_700_000_000_000;
    for i in 0..80 {
        insert_sample(conn, base + i * 3_000, i as f64, "gen-1");
    }
    // 80..100 deliberately missing (60 s hole, same generation).
    for i in 100..160 {
        insert_sample(conn, base + i * 3_000, i as f64, "gen-1");
    }
    // Later restart with a new generation (GENERATION_CHANGE marker).
    for i in 200..220 {
        insert_sample(conn, base + i * 3_000, i as f64, "gen-2");
    }

    let query = HistoryQuery {
        host_id: "host-1".into(),
        metric_id: "cpu.total.usage".into(),
        entity_id: "host".into(),
        from_utc_ms: base,
        to_utc_ms: base + 220 * 3_000,
        cadence_ms: 3_000,
        max_points: 400,
    };
    let response = get_history(conn, &query).expect("query ok");
    assert_eq!(response.resolution, Resolution::Raw);
    assert!(
        response.points.len() <= 400,
        "bounded result, got {}",
        response.points.len()
    );
    assert!(
        response.gaps.iter().any(|g| g.reason == "MISSING_SAMPLES"),
        "the 60 s hole must appear as a gap: {:?}",
        response.gaps
    );
    assert!(
        response
            .gaps
            .iter()
            .any(|g| g.reason == "GENERATION_CHANGE"),
        "the generation change must appear as a gap"
    );
    assert!(response.missing_ratio > 0.0 && response.missing_ratio < 0.5);

    // Validation refusals.
    let too_wide = HistoryQuery {
        to_utc_ms: query.from_utc_ms + 40 * 86_400_000,
        ..query.clone()
    };
    assert!(matches!(
        get_history(conn, &too_wide),
        Err(monitor_history::QueryError::RangeTooLarge)
    ));
    let bad_points = HistoryQuery {
        max_points: 100_000,
        ..query.clone()
    };
    assert!(get_history(conn, &bad_points).is_err());
}

#[test]
fn rollup_resolution_engages_for_long_ranges() {
    let conn = Connection::open_in_memory().unwrap();
    let store = HistoryStore::with_connection(conn).unwrap();
    let conn = store.connection();
    let base: i64 = 1_700_000_000_000;
    let base_bucket = base.div_euclid(ROLLUP_BUCKET_1M_MS) * ROLLUP_BUCKET_1M_MS;
    conn.execute(
        "INSERT INTO samples_rollup_1m (host_id, metric_id, entity_id, bucket_utc_ms, count, ok_count, min, max, avg, last) \
         VALUES ('host-1','m','e',?1,4,4,1,4,2.5,4)",
        rusqlite::params![base_bucket],
    )
    .unwrap();
    let query = HistoryQuery {
        host_id: "host-1".into(),
        metric_id: "m".into(),
        entity_id: "e".into(),
        from_utc_ms: base_bucket,
        to_utc_ms: base_bucket + 25 * 3_600_000, // 25h span -> 1m rollups
        cadence_ms: 3_000,
        max_points: 400,
    };
    let response = get_history(conn, &query).expect("query ok");
    assert_eq!(response.resolution, Resolution::Rollup1m);
    assert_eq!(response.points.len(), 1);
    assert_eq!(response.points[0].count, Some(4));
}

/// Load gate (phase 03 §3): 1M synthetic points, bounded query. Ignored by
/// default; run on native hardware with:
///   cargo test -p monitor-history --test history -- --ignored --nocapture
#[test]
#[ignore = "1M-row load test; run explicitly"]
fn one_million_point_load_query_stays_bounded() {
    let conn = Connection::open_in_memory().unwrap();
    let store = HistoryStore::with_connection(conn).unwrap();
    let conn = store.connection();
    let base: i64 = 1_700_000_000_000;
    let started = std::time::Instant::now();
    const INSERT_SQL: &str = "INSERT INTO samples_raw \
         (host_id, metric_id, entity_id, ts_utc_ms, value_real, source, generation, sequence) \
         VALUES ('host-1','cpu.total.usage','host',?1,?2,'load','g',1)";
    for chunk_start in (0..1_000_000i64).step_by(100_000) {
        let tx = conn.unchecked_transaction().unwrap();
        {
            let mut stmt = tx.prepare_cached(INSERT_SQL).unwrap();
            for i in chunk_start..chunk_start + 100_000 {
                stmt.execute(rusqlite::params![base + i * 1_000, (i % 100) as f64])
                    .unwrap();
            }
        }
        tx.commit().unwrap();
    }
    let insert_elapsed = started.elapsed();

    // Raw resolution over a 30-minute window (1800 samples) so the stride
    // decimation actually runs on top of the 1M-row table.
    let query = HistoryQuery {
        host_id: "host-1".into(),
        metric_id: "cpu.total.usage".into(),
        entity_id: "host".into(),
        from_utc_ms: base,
        to_utc_ms: base + 1_800_000,
        cadence_ms: 1_000,
        max_points: 400,
    };
    let query_started = std::time::Instant::now();
    let response = get_history(conn, &query).expect("bounded load query");
    let query_elapsed = query_started.elapsed();
    println!(
        "1M rows: insert {:?}, query {:?}, points returned {}",
        insert_elapsed,
        query_elapsed,
        response.points.len()
    );
    assert_eq!(response.resolution, Resolution::Raw);
    assert!(
        response.points.len() <= 400 && response.points.len() >= 300,
        "1800 samples with stride ceil(1800/400)=5 must yield ~360 bounded points, got {}",
        response.points.len()
    );
    assert!(
        query_elapsed < std::time::Duration::from_millis(200),
        "bounded query must stay fast over 1M rows"
    );
}
