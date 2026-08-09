//! The dump/restore round trip that `wavvon-hub backup` / `restore` is built
//! on (decisions.md, "One mechanism moves the data").
//!
//! The unit tests in `db::dump` cover the pure rules — direction, row-count
//! comparison. What only a real server can answer is whether the flag vector
//! actually round-trips *this* schema, which is not plain DDL: it carries a
//! `tsvector GENERATED ALWAYS AS (...) STORED` column, and `--no-owner
//! --no-acl` has to hold across a change of role.
//!
//! Needs `pg_dump`/`pg_restore` on PATH (or `WAVVON_PG_BIN_DIR`). When they
//! are absent the test says so and passes — a hard failure would make the
//! whole suite unrunnable on a machine without the client tools, which is
//! most Windows dev boxes. But a test that skips itself is a test that can
//! stop running without anyone noticing, so CI sets
//! `WAVVON_REQUIRE_PG_TOOLS=1` and the skip becomes a failure there.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use wavvon_hub::db;

fn base_db_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432".to_string())
}

fn have_pg_tools() -> bool {
    // Cheapest honest probe: ask pg_dump its version. Covers "not on PATH"
    // and "WAVVON_PG_BIN_DIR points somewhere wrong" identically.
    let bin = match std::env::var("WAVVON_PG_BIN_DIR") {
        Ok(dir) if !dir.is_empty() => std::path::Path::new(&dir).join("pg_dump"),
        _ => std::path::PathBuf::from("pg_dump"),
    };
    std::process::Command::new(bin)
        .arg("--version")
        .output()
        .is_ok()
}

/// A named database with migrations applied, dropped first so a killed run
/// does not poison the next one.
async fn fresh_db(name: &str) -> (PgPool, String) {
    let base = base_db_url();
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&format!("{base}/postgres"))
        .await
        .expect("connect to the postgres maintenance database");

    sqlx::query(&format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))
        .execute(&admin)
        .await
        .expect("drop leftover test database");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&admin)
        .await
        .expect("create test database");

    let url = format!("{base}/{name}");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect to test database");
    (pool, url)
}

async fn drop_db(name: &str) {
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&format!("{}/postgres", base_db_url()))
        .await
        .expect("connect to the postgres maintenance database");
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))
        .execute(&admin)
        .await;
}

#[tokio::test]
async fn dump_restores_every_row_into_an_empty_database() {
    if !have_pg_tools() {
        assert!(
            std::env::var("WAVVON_REQUIRE_PG_TOOLS").is_err(),
            "pg_dump not found, and WAVVON_REQUIRE_PG_TOOLS is set — this environment \
             is supposed to have the PostgreSQL client tools installed"
        );
        eprintln!(
            "SKIP backup_flow: pg_dump not found \
             (set WAVVON_PG_BIN_DIR or install postgresql-client)"
        );
        return;
    }

    let src_name = "wavvon_dumptest_src";
    let dst_name = "wavvon_dumptest_dst";

    let (src, src_url) = fresh_db(src_name).await;
    db::migrations::run(&src).await.expect("migrate source");

    // Something in a table, so a restore that produces an empty-but-correct
    // schema cannot pass. hub_settings is populated by the migrations
    // themselves and needs no fixture scaffolding.
    let seeded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM hub_settings")
        .fetch_one(&src)
        .await
        .expect("count hub_settings");
    assert!(seeded > 0, "migrations should seed hub_settings");

    let expected = db::dump::row_counts(&src).await.expect("source row counts");
    assert!(
        expected.len() > 50,
        "expected a real schema, got {} tables",
        expected.len()
    );

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("database.dump");
    let schema = db::dump::current_schema(&src)
        .await
        .expect("current schema");
    db::dump::dump(&src_url, &archive, &schema).expect("pg_dump");
    assert!(archive.metadata().expect("dump file").len() > 0);

    let (dst, dst_url) = fresh_db(dst_name).await;
    assert!(
        db::dump::is_empty(&dst).await.expect("emptiness check"),
        "a freshly created database has no tables"
    );

    db::dump::restore(&dst_url, &archive).expect("pg_restore");

    let actual = db::dump::row_counts(&dst)
        .await
        .expect("restored row counts");
    db::dump::compare_row_counts(&expected, &actual).expect("every table must come back whole");

    // The one piece of schema that is not plain DDL. If `--no-owner --no-acl`
    // or the custom format ever stopped carrying it, search would silently
    // come back dead on every restored hub.
    let generated: Option<String> = sqlx::query_scalar(
        "SELECT is_generated FROM information_schema.columns \
         WHERE table_name = 'posts' AND generation_expression IS NOT NULL LIMIT 1",
    )
    .fetch_optional(&dst)
    .await
    .expect("inspect generated columns");
    assert_eq!(
        generated.as_deref(),
        Some("ALWAYS"),
        "the posts tsvector column must survive the round trip"
    );

    assert!(
        !db::dump::is_empty(&dst).await.expect("emptiness check"),
        "the restored database is no longer empty — this is what --force exists to override"
    );

    drop(src);
    drop(dst);
    drop_db(src_name).await;
    drop_db(dst_name).await;
}

/// The same round trip for a hub whose tables live in its own schema rather
/// than in `public` — the layout a farm uses when its PostgreSQL grants one
/// database and no `CREATEDB`.
///
/// `dump.rs` used to hardcode `schemaname = 'public'`. Left that way, a
/// schema-isolated hub would have counted **zero tables, dumped nothing, and
/// reported a successful backup** — an operator's archive quietly empty until
/// the day they needed it. Worth its own test precisely because the failure
/// looks like success.
#[tokio::test]
async fn a_hub_in_its_own_schema_backs_up_that_schema() {
    let name = "wavvon_dumptest_schema";
    let (admin_pool, base) = {
        let base = base_db_url();
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&format!("{base}/postgres"))
            .await
            .expect("connect to the postgres maintenance database");
        (pool, base)
    };
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))
        .execute(&admin_pool)
        .await;
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&admin_pool)
        .await
        .expect("create test database");

    // A hub confined to `hub_x` by search_path, exactly as the farm hands it
    // over in schema-isolation mode.
    let plain = format!("{base}/{name}");
    let setup = PgPoolOptions::new()
        .max_connections(1)
        .connect(&plain)
        .await
        .unwrap();
    sqlx::query("CREATE SCHEMA hub_x")
        .execute(&setup)
        .await
        .unwrap();

    let scoped_url = format!("{plain}?options=-c%20search_path%3Dhub_x,public");
    let scoped = PgPoolOptions::new()
        .max_connections(2)
        .connect(&scoped_url)
        .await
        .expect("connect with a search_path");
    db::migrations::run(&scoped)
        .await
        .expect("migrate into hub_x");

    assert_eq!(
        db::dump::current_schema(&scoped).await.unwrap(),
        "hub_x",
        "the connection must actually be scoped, or this test proves nothing"
    );

    let expected = db::dump::row_counts(&scoped).await.expect("row counts");
    assert!(
        expected.len() > 50,
        "the hub's tables must be found in its own schema, got {}",
        expected.len()
    );

    // Everything above needs no external binary, and it is the half that would
    // have caught the bug: a schema-blind `row_counts` finds zero tables here
    // and a backup built on it reports success over nothing. Only the dump
    // itself needs pg_dump, so only the dump is gated — otherwise the whole
    // check would vanish on any machine without the client tools, which is
    // most of them.
    if have_pg_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("database.dump");
        db::dump::dump(&scoped_url, &archive, "hub_x").expect("pg_dump");
        assert!(
            archive.metadata().unwrap().len() > 0,
            "an empty archive is the failure this test exists for"
        );
    } else {
        assert!(
            std::env::var("WAVVON_REQUIRE_PG_TOOLS").is_err(),
            "pg_dump not found, and WAVVON_REQUIRE_PG_TOOLS is set"
        );
        eprintln!("SKIP the pg_dump half: pg_dump not found");
    }

    drop(scoped);
    drop(setup);
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))
        .execute(&admin_pool)
        .await;
}
