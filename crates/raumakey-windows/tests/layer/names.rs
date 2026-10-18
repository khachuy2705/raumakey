//! `names`: SID hash, named-object and pipe names (pure).

use raumakey_model::identity::NAMED_OBJECT_PREFIX;
use raumakey_windows::names::{
    NameError, SidHash, fnv1a_64, is_valid_component, message_name, object_name, pipe_name,
    sid_hash16,
};

#[test]
fn fnv1a_64_matches_the_published_vectors() {
    assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn sid_hash_is_sixteen_lowercase_hex_digits() {
    let hash = sid_hash16("S-1-5-21-1004336348-1177238915-682003330-512");
    assert_eq!(hash.as_str(), "bb4b9cece699243d");
    assert_eq!(sid_hash16("").as_str(), "cbf29ce484222325");
    assert_eq!(sid_hash16("a").as_str(), "af63dc4c8601ec8c");
    // Leading zeros are kept.
    for sid in ["S-1-5-18", "S-1-5-19", "S-1-12-1-1-2-3-4"] {
        let hash = sid_hash16(sid);
        assert_eq!(hash.as_str().len(), 16);
        assert!(
            hash.as_str()
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }
}

#[test]
fn sid_hash_parses_only_canonical_hashes() {
    assert!(SidHash::parse("0123456789abcdef").is_ok());
    for bad in [
        "",
        "0123456789abcde",
        "0123456789abcdef0",
        "0123456789ABCDEF",
        "0123456789abcdeg",
        "0123456789abcde.",
    ] {
        assert_eq!(
            SidHash::parse(bad),
            Err(NameError::InvalidSidHash),
            "{bad:?}"
        );
    }
}

#[test]
fn object_names_are_user_qualified_and_prefixed() {
    let hash = SidHash::parse("0123456789abcdef").unwrap();
    let name = object_name(&hash, "controller").unwrap();
    assert_eq!(name, "Local\\Raumakey.0123456789abcdef.controller");
    assert!(name.starts_with(NAMED_OBJECT_PREFIX));
    assert_eq!(
        object_name(&hash, "legacy-shm_1").unwrap(),
        "Local\\Raumakey.0123456789abcdef.legacy-shm_1"
    );
}

#[test]
fn pipe_names_carry_session_and_user() {
    let hash = SidHash::parse("0123456789abcdef").unwrap();
    assert_eq!(
        pipe_name(1, &hash),
        "\\\\.\\pipe\\Raumakey.1.0123456789abcdef.control"
    );
    assert_eq!(
        pipe_name(4_294_967_295, &hash),
        "\\\\.\\pipe\\Raumakey.4294967295.0123456789abcdef.control"
    );
}

/// Registered window messages live in the session-wide atom table, so their names carry the
/// user's SID hash, like the names of kernel objects (but without `Local\`).
#[test]
fn message_names_are_user_qualified_and_prefixed() {
    let hash = SidHash::parse("0123456789abcdef").unwrap();
    assert_eq!(
        message_name(&hash, "show-settings").unwrap(),
        "Raumakey.0123456789abcdef.show-settings"
    );
    for bad in ["", "a.b", "a b", "ế", "a\0"] {
        assert_eq!(
            message_name(&hash, bad),
            Err(NameError::InvalidComponent),
            "{bad:?}"
        );
    }
}

#[test]
fn components_are_restricted_to_ascii_word_characters() {
    for good in ["a", "controller", "A-b_9", &"x".repeat(64)] {
        assert!(is_valid_component(good), "{good:?}");
    }
    for bad in [
        "",
        ".",
        "a.b",
        "a\\b",
        "a/b",
        "a b",
        "a:b",
        "..",
        "ä",
        "a\0",
        &"x".repeat(65),
    ] {
        assert!(!is_valid_component(bad), "{bad:?}");
        let hash = SidHash::parse("0123456789abcdef").unwrap();
        assert_eq!(object_name(&hash, bad), Err(NameError::InvalidComponent));
    }
}

#[test]
fn errors_display_without_the_rejected_text() {
    let text = NameError::InvalidComponent.to_string();
    assert!(!text.is_empty());
    assert!(!NameError::InvalidSidHash.to_string().is_empty());
}
