//! The PostgreSQL the hub carries with it, actually started.
//!
//! The unit tests decide what to do about a data directory. This decides
//! nothing — it runs the thing: install the compiled-in archive, initialise a
//! data directory, start the server, run the real migrations, and come back
//! with a database the hub could serve from.
//!
//! That distinction is the whole point. `hub_isolation = 'schema'` shipped with
//! unit tests over the URL it built and nothing that had ever started a hub
//! behind one, and the failure mode was silent. "Zero prerequisites: download a
//! binary and run it" is a claim only an execution can make.

use std::path::PathBuf;

use wavvon_hub::embedded_pg;

/// A directory of its own per test, removed afterwards. Not a tempdir crate
/// call: PostgreSQL is still holding this directory open on Windows when the
/// test ends, so cleanup is best-effort by design.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "wavvon-embedded-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    Scratch(dir)
}

#[tokio::test]
async fn the_bundled_postgres_starts_and_takes_the_hub_schema() {
    let root = scratch("start");

    let pg = embedded_pg::start(&root.0)
        .await
        .expect("the compiled-in archive must install, initialise and start");

    // The data landed where the hub put it, not in a home directory or a
    // tempdir — the crate's own defaults are both, and a server in a tempdir
    // is a hub that loses everything on reboot.
    assert!(
        pg.data_dir().starts_with(&root.0),
        "data dir: {}",
        pg.data_dir().display()
    );
    assert!(pg.data_dir().join("PG_VERSION").exists());
    assert!(
        root.0.join("pg").exists(),
        "installations are version-scoped under <root>/pg"
    );

    // And it is a database the hub can actually use: the real migrations, not
    // a SELECT 1.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(pg.url())
        .await
        .expect("the URL handed out must connect");
    wavvon_hub::db::migrations::run(&pool)
        .await
        .expect("the hub's own migrations must apply to the embedded server");

    let channels: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM channels")
        .fetch_one(&pool)
        .await
        .expect("a migrated schema has a channels table");
    assert_eq!(channels, 0);

    pool.close().await;
    pg.stop().await.expect("stopping must be clean");
}

/// Restarting is the common case, and it must keep the data — which means
/// keeping the port and the password, because initdb set that password once
/// and a regenerated one would strand the directory it belongs to.
#[tokio::test]
async fn a_second_start_reuses_the_same_data_and_credentials() {
    let root = scratch("restart");

    let first = embedded_pg::start(&root.0).await.expect("first start");
    let first_url = first.url().to_string();
    {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&first_url)
            .await
            .expect("connect");
        sqlx::query("CREATE TABLE persistence_probe (id INT)")
            .execute(&pool)
            .await
            .expect("write something worth keeping");
        pool.close().await;
    }
    first.stop().await.expect("stop");

    let second = embedded_pg::start(&root.0).await.expect("second start");
    assert_eq!(
        second.url(),
        first_url,
        "a restart that moves port or password strands every tool pointed at the old one"
    );

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(second.url())
        .await
        .expect("connect after restart");
    let exists: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM information_schema.tables WHERE table_name = 'persistence_probe'",
    )
    .fetch_optional(&pool)
    .await
    .expect("query");
    assert_eq!(exists, Some(1), "the data must survive a restart");
    pool.close().await;
    second.stop().await.expect("stop");
}

/// A data directory from another major is refused, with the dump/restore path
/// named. Never a half-migration, and never a guess.
#[tokio::test]
async fn data_from_another_major_is_refused_rather_than_touched() {
    let root = scratch("major");
    let data_dir = root.0.join("pgdata");
    std::fs::create_dir_all(&data_dir).unwrap();
    let bundled = embedded_pg::bundled_major().expect("bundled major");
    std::fs::write(data_dir.join("PG_VERSION"), format!("{}\n", bundled - 1)).unwrap();

    let err = match embedded_pg::start(&root.0).await {
        Ok(_) => panic!("an older data directory must not be started"),
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("backup"),
        "the refusal must say what to do: {err}"
    );
    assert!(
        !data_dir.join("postgresql.conf").exists(),
        "refusing must leave the data directory untouched"
    );
}

/// A hub that was killed leaves its PostgreSQL running, and `pg_ctl start`
/// against a live data directory fails. Without adoption that means one crash
/// costs a manual hunt for a process the operator never knew existed — on the
/// install story whose whole promise is that there is nothing to administer.
#[tokio::test]
async fn a_server_left_running_is_adopted_rather_than_restarted() {
    let root = scratch("adopt");

    let first = embedded_pg::start(&root.0).await.expect("first start");
    let url = first.url().to_string();

    // Deliberately *not* stopping it: this is the crashed-hub shape.
    std::mem::forget(first);

    let second = embedded_pg::start(&root.0)
        .await
        .expect("a hub must come back up against its own running server");
    assert_eq!(second.url(), url, "adoption must reach the same database");

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(second.url())
        .await
        .expect("the adopted server must answer");
    pool.close().await;
    second.stop().await.expect("stop");
}

/// Backup against the embedded server, with the bundled `pg_dump`.
///
/// This is the case that would otherwise fail on exactly the install story the
/// bundling exists for: PostgreSQL was never installed, so `pg_dump` is not on
/// PATH and never will be. Starting the embedded server has to point the dump
/// path at the binaries it is running, or "download a binary and run it" ships
/// with a backup command that cannot work.
#[tokio::test]
async fn backup_and_restore_work_against_the_bundled_server() {
    let root = scratch("backup");
    // A previous test in this file may have set it; the value belongs to
    // whichever instance is running now.
    std::env::remove_var(wavvon_hub::db::dump::PG_BIN_DIR_ENV);

    let pg = embedded_pg::start(&root.0).await.expect("start");

    let bin_dir = pg.bin_dir();
    assert!(
        bin_dir.exists(),
        "the bundled install must carry its client tools: {}",
        bin_dir.display()
    );
    assert_eq!(
        std::env::var(wavvon_hub::db::dump::PG_BIN_DIR_ENV)
            .ok()
            .map(std::path::PathBuf::from),
        Some(bin_dir),
        "starting the embedded server must point the dump path at its own binaries"
    );

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(pg.url())
        .await
        .expect("connect");
    wavvon_hub::db::migrations::run(&pool)
        .await
        .expect("migrate");
    sqlx::query(
        "INSERT INTO users (public_key, first_seen_at, last_seen_at) VALUES ('owner', 0, 0)",
    )
    .execute(&pool)
    .await
    .expect("a user to own the channel");
    sqlx::query("INSERT INTO channels (id, name, created_by, created_at) VALUES ('c1', 'general', 'owner', 0)")
        .execute(&pool)
        .await
        .expect("something to back up");

    let archive = root.0.join("dump.pgc");
    let schema = wavvon_hub::db::dump::current_schema(&pool)
        .await
        .expect("schema");
    wavvon_hub::db::dump::dump(pg.url(), &archive, &schema).expect("pg_dump must run");
    assert!(archive.metadata().expect("archive").len() > 0);

    // And back: restore into an empty database and find the row again.
    sqlx::query("DROP TABLE channels CASCADE")
        .execute(&pool)
        .await
        .expect("clear the way for a restore");
    wavvon_hub::db::dump::restore(pg.url(), &archive).expect("pg_restore must run");

    let name: String = sqlx::query_scalar("SELECT name FROM channels WHERE id = 'c1'")
        .fetch_one(&pool)
        .await
        .expect("the restored row must be there");
    assert_eq!(name, "general");

    pool.close().await;
    pg.stop().await.expect("stop");
}
