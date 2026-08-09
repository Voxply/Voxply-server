//! Minimum supported PostgreSQL server version.
//!
//! The hub runs its own schema migrations but has no control over the server
//! an operator points it at. Without this check, a server older than the SQL
//! in `migrations.rs` requires fails partway through applying the schema, and
//! the operator sees a syntax error from a `CREATE TABLE` rather than a
//! sentence telling them their PostgreSQL is too old.
//!
//! The floor rises only when a feature genuinely needs it — never on a
//! schedule. See decisions.md ("The hub bundles PostgreSQL, and never touches
//! one it did not create") and the version table in `hosting.md`.

use sqlx::PgPool;

/// Minimum PostgreSQL this Wavvon release supports, in `server_version_num`
/// form (major * 10000 + minor).
///
/// **14** as of 2026-08: the oldest release still receiving upstream security
/// patches. The code's own floor is lower — PostgreSQL 12, set by the
/// `tsvector GENERATED ALWAYS AS (...) STORED` column on `posts` — so this
/// costs no operator anything today while ruling out servers that are already
/// unsupported upstream.
pub const MIN_SERVER_VERSION_NUM: i32 = 140_000;

/// Renders `server_version_num` back to something an operator recognises:
/// `160004` → `"16.4"`.
fn human(version_num: i32) -> String {
    format!("{}.{}", version_num / 10_000, version_num % 10_000)
}

/// Pure half of the check, so the comparison and the message are testable
/// without a server of every vintage to hand.
fn check_version_num(found: i32) -> Result<(), String> {
    if found >= MIN_SERVER_VERSION_NUM {
        return Ok(());
    }
    Err(format!(
        "PostgreSQL {} is too old: wavvon-hub {} requires {} or newer. \
         Upgrade the server, or point WAVVON_DATABASE_URL at a newer one. \
         See https://github.com/Wavvon/Wavvon-docs — hosting.md, \"Providing PostgreSQL\".",
        human(found),
        env!("CARGO_PKG_VERSION"),
        human(MIN_SERVER_VERSION_NUM),
    ))
}

/// Fails with an operator-readable message when the server is below the floor.
///
/// Call before running migrations — the whole point is to say why *before*
/// leaving a half-applied schema behind.
pub async fn ensure_supported(pool: &PgPool) -> Result<(), String> {
    // `server_version_num` is an integer setting exposed as text by SHOW.
    let raw: String = sqlx::query_scalar("SHOW server_version_num")
        .fetch_one(pool)
        .await
        .map_err(|e| format!("could not read the PostgreSQL server version: {e}"))?;

    let found: i32 = raw
        .trim()
        .parse()
        .map_err(|_| format!("PostgreSQL reported an unparseable version: {raw:?}"))?;

    check_version_num(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_floor_and_anything_newer() {
        assert!(check_version_num(MIN_SERVER_VERSION_NUM).is_ok());
        assert!(check_version_num(160_004).is_ok(), "16.4");
        assert!(check_version_num(180_000).is_ok(), "18.0");
    }

    #[test]
    fn rejects_below_the_floor() {
        assert!(check_version_num(130_099).is_err(), "13.99 is still 13");
        assert!(check_version_num(120_000).is_err());
    }

    /// The message has to be usable by someone who does not know what
    /// `server_version_num` is — it carries both numbers in human form and
    /// says what to do.
    #[test]
    fn rejection_names_both_versions_readably() {
        let err = check_version_num(120_004).unwrap_err();
        assert!(err.contains("12.4"), "found version, got: {err}");
        assert!(err.contains("14.0"), "required version, got: {err}");
        assert!(err.contains("WAVVON_DATABASE_URL"), "remedy, got: {err}");
    }

    #[test]
    fn human_renders_major_and_minor() {
        assert_eq!(human(160_004), "16.4");
        assert_eq!(human(140_000), "14.0");
    }
}
