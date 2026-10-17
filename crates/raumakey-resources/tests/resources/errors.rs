//! `ResourceError` messages and how build-script helpers report them.

use std::collections::BTreeSet;
use std::error::Error as _;
use std::io;
use std::path::Path;

use raumakey_resources::{Artifact, ResourceError, emit_with_version};

const ALL: [ResourceError; 9] = [
    ResourceError::ZeroOrdinal,
    ResourceError::InvalidName,
    ResourceError::Duplicate,
    ResourceError::TooLarge,
    ResourceError::InvalidVersion,
    ResourceError::InvalidText,
    ResourceError::InvalidIconSize,
    ResourceError::EmptyIconGroup,
    ResourceError::IconIdsExhausted,
];

/// Fails to compile when a variant is added without extending [`ALL`].
fn listed(error: ResourceError) -> bool {
    match error {
        ResourceError::ZeroOrdinal
        | ResourceError::InvalidName
        | ResourceError::Duplicate
        | ResourceError::TooLarge
        | ResourceError::InvalidVersion
        | ResourceError::InvalidText
        | ResourceError::InvalidIconSize
        | ResourceError::EmptyIconGroup
        | ResourceError::IconIdsExhausted => ALL.contains(&error),
    }
}

#[test]
fn every_error_has_its_own_lower_case_message() {
    let messages: Vec<String> = ALL.iter().map(ToString::to_string).collect();
    for (error, message) in ALL.iter().zip(&messages) {
        assert!(listed(*error));
        assert!(
            message.starts_with(|c: char| c.is_ascii_lowercase()),
            "{error:?}: {message:?}"
        );
        assert!(!message.ends_with('.'), "{error:?}: {message:?}");
        assert!(error.source().is_none(), "{error:?}");
    }
    let distinct: BTreeSet<&String> = messages.iter().collect();
    assert_eq!(distinct.len(), ALL.len(), "messages are distinct");
}

#[test]
fn emit_wraps_the_resource_error_in_an_io_error() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("raumakey-resources-errors");
    let error = emit_with_version(&dir, Artifact::Tip, "1.2").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    let inner = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ResourceError>());
    assert_eq!(inner, Some(&ResourceError::InvalidVersion));
    assert_eq!(error.to_string(), ResourceError::InvalidVersion.to_string());
}
