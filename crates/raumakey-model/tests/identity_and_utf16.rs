use proptest::prelude::*;
use raumakey_model::identity::{
    Guid, INSTALLER_UPGRADE_CODE, LANGID_VIETNAMESE, NAMED_OBJECT_PREFIX, PRODUCT_NAME,
    RAUMAKEY_INJECTED_TAG, REGISTRY_ROOT, TIP_CLSID, TIP_PROFILE_GUID,
};
use raumakey_model::utf16;

#[test]
fn product_identities_use_registry_format() {
    assert_eq!(PRODUCT_NAME, "Raumakey");
    assert_eq!(
        TIP_CLSID.to_string(),
        "{0FC48F2B-4F91-44A0-B500-DE512E801432}"
    );
    assert_eq!(
        TIP_PROFILE_GUID.to_string(),
        "{DB875058-BBF9-407C-85A8-950BE7D14C22}"
    );
    assert_eq!(
        INSTALLER_UPGRADE_CODE.to_string(),
        "{BF73893F-FCEB-4BAE-AE1C-E81D7107ECE5}"
    );
    assert_eq!(NAMED_OBJECT_PREFIX, "Local\\Raumakey.");
    assert_eq!(REGISTRY_ROOT, "Software\\Raumakey");
    assert_eq!(LANGID_VIETNAMESE, 0x042A);
}

#[test]
fn guid_exposes_com_fields() {
    assert_eq!(TIP_CLSID.data1(), 0x0FC4_8F2B);
    assert_eq!(TIP_CLSID.data2(), 0x4F91);
    assert_eq!(TIP_CLSID.data3(), 0x44A0);
    assert_eq!(
        TIP_CLSID.data4(),
        [0xB5, 0x00, 0xDE, 0x51, 0x2E, 0x80, 0x14, 0x32]
    );
}

#[test]
fn guid_parse_accepts_braces_and_case() {
    assert_eq!(
        Guid::parse("{db875058-bbf9-407c-85a8-950be7d14c22}").unwrap(),
        TIP_PROFILE_GUID
    );
    assert_eq!(
        Guid::parse("DB875058-BBF9-407C-85A8-950BE7D14C22").unwrap(),
        TIP_PROFILE_GUID
    );
    assert!(Guid::parse("{DB875058-BBF9-407C-85A8-950BE7D14C2}").is_err());
    assert!(Guid::parse("{DB875058BBF9-407C-85A8-950BE7D14C22X}").is_err());
    assert!(Guid::parse("").is_err());
}

#[test]
fn identities_differ_from_the_legacy_product() {
    let legacy_clsid = Guid::parse("{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}").unwrap();
    let legacy_profile = Guid::parse("{F74F81AF-6F78-42C7-B006-2630C950B9A2}").unwrap();
    assert_ne!(TIP_CLSID, legacy_clsid);
    assert_ne!(TIP_PROFILE_GUID, legacy_profile);
    assert_ne!(RAUMAKEY_INJECTED_TAG, 0xEBCA_1234);
}

#[test]
fn injected_tag_fits_a_32_bit_extra_info() {
    assert!(RAUMAKEY_INJECTED_TAG <= u64::from(u32::MAX));
    assert_ne!(RAUMAKEY_INJECTED_TAG, 0);
}

#[test]
fn utf16_validation_rejects_lone_surrogates() {
    assert!(utf16::validate(&[0x61, 0x62]).is_ok());
    assert!(utf16::validate(&[0xD83D, 0xDE00]).is_ok());
    assert_eq!(utf16::validate(&[0x61, 0xD83D]).unwrap_err().index, 1);
    assert_eq!(utf16::validate(&[0xDE00, 0x61]).unwrap_err().index, 0);
    assert_eq!(utf16::validate(&[0xD83D, 0x61]).unwrap_err().index, 0);
    assert!(utf16::is_well_formed(&[]));
}

#[test]
fn utf16_scalar_boundaries() {
    let text = [0x61, 0xD83D, 0xDE00, 0x62];
    assert!(utf16::is_scalar_boundary(&text, 0));
    assert!(utf16::is_scalar_boundary(&text, 1));
    assert!(!utf16::is_scalar_boundary(&text, 2));
    assert!(utf16::is_scalar_boundary(&text, 3));
    assert!(utf16::is_scalar_boundary(&text, 4));
    assert!(!utf16::is_scalar_boundary(&text, 5));
    assert_eq!(utf16::scalar_count(&text), 3);
    assert_eq!(utf16::last_scalar_len(&text), 1);
    assert_eq!(utf16::last_scalar_len(&text[..3]), 2);
    assert_eq!(utf16::last_scalar_len(&[]), 0);
}

#[test]
fn utf16_single_scalar_and_round_trip() {
    assert_eq!(utf16::single_scalar(&[0x61]), Some('a'));
    assert_eq!(utf16::single_scalar(&[0xD83D, 0xDE00]), Some('😀'));
    assert_eq!(utf16::single_scalar(&[0x61, 0x62]), None);
    assert_eq!(utf16::single_scalar(&[0xD83D]), None);
    assert_eq!(utf16::single_scalar(&[]), None);
    let units = utf16::encode("Tiếng Việt 😀");
    assert_eq!(utf16::decode(&units).unwrap(), "Tiếng Việt 😀");
    assert!(utf16::decode(&[0xDC00]).is_err());
}

proptest! {
    #[test]
    fn encoded_strings_are_well_formed(s in ".*") {
        let units = utf16::encode(&s);
        prop_assert!(utf16::validate(&units).is_ok());
        prop_assert_eq!(utf16::decode(&units).unwrap(), s.clone());
        prop_assert_eq!(utf16::scalar_count(&units), s.chars().count());
    }

    #[test]
    fn validation_agrees_with_std(units in proptest::collection::vec(any::<u16>(), 0..32)) {
        prop_assert_eq!(utf16::is_well_formed(&units), String::from_utf16(&units).is_ok());
    }
}
