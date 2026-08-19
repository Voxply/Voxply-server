//! Hub slugs — the human-readable half of a hub's address.
//!
//! A hub is reachable two ways on a farm:
//!
//! ```text
//! https://farm.example/hub/MangiaDaPippo      <- slug, chosen by the owner
//! https://farm.example/hub/<64 hex chars>     <- pubkey, the hub's identity
//! ```
//!
//! **The slug is an alias. The pubkey is the identity.** That distinction is
//! the whole design, and it buys one property nothing else does: a client can
//! compare the key it expects against `/info.public_key` and notice if a name
//! now points somewhere else. If the slug *were* the identity there would be
//! nothing to compare against, and the farm's mapping would have to be taken
//! on faith — which a self-hosted federated product cannot ask for.
//!
//! It is deliberately **not** a slugification of the hub's display name: the
//! name is presentation ("Osteria di Pippo", spaces and accents and emoji
//! welcome) and changing it must never disturb a URL somebody bookmarked.
//!
//! Slugs are released, not deleted: see `hub_slugs.released_at`. A released
//! slug stops resolving, frees a quota slot, and is held for a cooling-off
//! period during which only the hub that gave it up may reclaim it. After that
//! it returns to the pool. Names come back — but never the instant somebody
//! lets one go, which is when inheriting their inbound links is worth most.

/// Longest a slug may be. Long enough for a real name, short enough to dictate.
pub const MAX_LEN: usize = 32;
/// Shortest. Two characters are a landgrab, not a name.
pub const MIN_LEN: usize = 3;

/// Held back for routes we may want directly under `/hub/`, plus a handful
/// that would be actively confusing to let someone own.
const RESERVED: &[&str] = &[
    "admin", "api", "new", "join", "info", "health", "hub", "farm", "auth", "ws", "static",
    "assets", "about", "help", "support", "settings",
];

#[derive(Debug, PartialEq, Eq)]
pub enum SlugError {
    TooShort,
    TooLong,
    BadCharacter,
    Reserved,
    LooksLikeAPubkey,
}

impl SlugError {
    /// Operator-facing text. These reach a person typing a name into a form,
    /// so each one says what to do rather than what went wrong.
    pub fn message(&self) -> String {
        match self {
            SlugError::TooShort => format!("must be at least {MIN_LEN} characters"),
            SlugError::TooLong => format!("must be at most {MAX_LEN} characters"),
            SlugError::BadCharacter => {
                "may only contain letters a-z, digits, hyphen and underscore".to_string()
            }
            SlugError::Reserved => "that name is reserved".to_string(),
            SlugError::LooksLikeAPubkey => {
                "64 hex characters is the shape of a hub key — pick something else".to_string()
            }
        }
    }
}

/// Validate and normalise a slug, returning the form to store and match on.
///
/// **Comparison is case-insensitive**, and that is a security property, not a
/// convenience. Without it `MangiaDaPippo` and `mangiadapippo` are two owners,
/// and the second one inherits the first one's traffic from anybody who
/// mistypes the capitals. This risk did not exist while the address was a
/// pubkey; it arrives the moment a person picks the name.
///
/// **ASCII only**, for the same reason one step further out: `MangiaDaPippo`
/// written with a Cyrillic "а" is a different string that renders identically.
/// Refusing non-ASCII closes that at the door. Full homoglyph analysis would
/// not be worth the complexity — the hub's real name travels in its title,
/// not its URL.
pub fn normalize(raw: &str) -> Result<String, SlugError> {
    let trimmed = raw.trim();
    // Count chars, not bytes: a multi-byte input should fail on BadCharacter
    // below with a useful message, not on a length check that looks arbitrary.
    let len = trimmed.chars().count();
    if len < MIN_LEN {
        return Err(SlugError::TooShort);
    }
    if len > MAX_LEN {
        return Err(SlugError::TooLong);
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(SlugError::BadCharacter);
    }

    let lowered = trimmed.to_ascii_lowercase();

    if RESERVED.contains(&lowered.as_str()) {
        return Err(SlugError::Reserved);
    }
    // The proxy accepts a slug or a pubkey in the same path segment, so a slug
    // shaped like a key would be ambiguous. 64 hex chars can't pass MAX_LEN
    // anyway today, but this must not silently become reachable if MAX_LEN
    // ever grows.
    if lowered.len() == 64 && lowered.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(SlugError::LooksLikeAPubkey);
    }

    Ok(lowered)
}

/// True when this path segment is a hub pubkey rather than a slug.
///
/// Used by the proxy to pick which column to resolve against. Kept here beside
/// `normalize`, because the two definitions must agree: anything this accepts
/// must be something `normalize` rejects, or one address would mean two hubs.
pub fn looks_like_pubkey(segment: &str) -> bool {
    segment.len() == 64 && segment.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_normal_name_and_lowercases_it() {
        assert_eq!(normalize("MangiaDaPippo").unwrap(), "mangiadapippo");
        assert_eq!(normalize("  OsteriaPippo  ").unwrap(), "osteriapippo");
        assert_eq!(normalize("il-bar_di-pippo2").unwrap(), "il-bar_di-pippo2");
    }

    /// The impersonation this prevents: two owners, one visible name.
    #[test]
    fn case_variants_are_the_same_slug() {
        assert_eq!(
            normalize("MangiaDaPippo").unwrap(),
            normalize("mangiadapippo").unwrap()
        );
        assert_eq!(
            normalize("MANGIADAPIPPO").unwrap(),
            normalize("mangiaDApippo").unwrap()
        );
    }

    /// A Cyrillic "а" renders identically to the Latin one. Refused at the
    /// door rather than analysed.
    #[test]
    fn non_ascii_is_refused() {
        assert_eq!(
            normalize("Mangi\u{0430}DaPippo"),
            Err(SlugError::BadCharacter)
        );
        assert_eq!(normalize("osteria-café"), Err(SlugError::BadCharacter));
        assert_eq!(normalize("pippo🍻bar"), Err(SlugError::BadCharacter));
    }

    #[test]
    fn spaces_and_punctuation_are_refused() {
        assert_eq!(normalize("Osteria di Pippo"), Err(SlugError::BadCharacter));
        assert_eq!(normalize("pippo.bar"), Err(SlugError::BadCharacter));
        assert_eq!(normalize("pippo/bar"), Err(SlugError::BadCharacter));
    }

    #[test]
    fn length_bounds() {
        assert_eq!(normalize("ab"), Err(SlugError::TooShort));
        assert_eq!(normalize(""), Err(SlugError::TooShort));
        assert!(normalize("abc").is_ok());
        assert!(normalize(&"a".repeat(MAX_LEN)).is_ok());
        assert_eq!(normalize(&"a".repeat(MAX_LEN + 1)), Err(SlugError::TooLong));
    }

    #[test]
    fn reserved_names_are_refused_case_insensitively() {
        assert_eq!(normalize("admin"), Err(SlugError::Reserved));
        assert_eq!(normalize("Admin"), Err(SlugError::Reserved));
        assert_eq!(normalize("JOIN"), Err(SlugError::Reserved));
    }

    /// The two functions must never both accept the same string, or one
    /// address would resolve to two different hubs.
    #[test]
    fn a_slug_and_a_pubkey_can_never_be_the_same_string() {
        let key = "a".repeat(64);
        assert!(looks_like_pubkey(&key));
        assert!(
            normalize(&key).is_err(),
            "normalize must reject anything looks_like_pubkey accepts"
        );
    }

    #[test]
    fn pubkey_detection() {
        assert!(looks_like_pubkey(&"0123456789abcdef".repeat(4)));
        assert!(!looks_like_pubkey("mangiadapippo"));
        assert!(!looks_like_pubkey(&"g".repeat(64)), "not hex");
        assert!(!looks_like_pubkey(&"a".repeat(63)), "wrong length");
    }
}
