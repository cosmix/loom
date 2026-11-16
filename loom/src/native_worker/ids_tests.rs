use super::*;

#[test]
fn unit_id_accepts_valid() {
    for id in ["T0a_1-x", "a", &"a".repeat(64)] {
        assert_eq!(validate_unit_id(id), Ok(()));
    }
}

#[test]
fn unit_id_rejects_empty() {
    assert_eq!(validate_unit_id(""), Err(IdError::Empty));
}

#[test]
fn unit_id_rejects_65_chars_as_too_long() {
    assert_eq!(validate_unit_id(&"a".repeat(65)), Err(IdError::TooLong));
}

#[test]
fn unit_id_rejects_dots_and_slashes_as_bad_character() {
    assert_eq!(validate_unit_id(".."), Err(IdError::BadCharacter('.')));
    assert_eq!(validate_unit_id("a/b"), Err(IdError::BadCharacter('/')));
}

#[test]
fn unit_id_rejects_leading_dash_as_bad_shape() {
    assert_eq!(validate_unit_id("-a"), Err(IdError::BadShape));
    assert_eq!(validate_unit_id("_a"), Err(IdError::BadShape));
}

#[test]
fn message_id_matches_unit_id_rules() {
    for id in ["abc_1-x", "", "..", "-x", "a/b", &"x".repeat(65)] {
        assert_eq!(validate_message_id(id), validate_unit_id(id));
    }
}

#[test]
fn invocation_id_accepts_inv_plus_32_lowercase_hex() {
    assert_eq!(
        validate_invocation_id("inv-0123456789abcdef0123456789abcdef"),
        Ok(())
    );
}

#[test]
fn invocation_id_rejects_uppercase_hex() {
    assert_eq!(
        validate_invocation_id("inv-0123456789abcdef0123456789abcdeF"),
        Err(IdError::BadCharacter('F'))
    );
}

#[test]
fn invocation_id_rejects_wrong_prefix() {
    assert_eq!(
        validate_invocation_id("bad-0123456789abcdef0123456789abcdef"),
        Err(IdError::BadShape)
    );
}

#[test]
fn invocation_id_rejects_short() {
    assert_eq!(validate_invocation_id("inv-123"), Err(IdError::BadShape));
    assert_eq!(validate_invocation_id(""), Err(IdError::Empty));
}

#[test]
fn invocation_id_rejects_long() {
    assert_eq!(
        validate_invocation_id(&format!("inv-{}", "a".repeat(33))),
        Err(IdError::TooLong)
    );
}

#[test]
fn mint_invocation_id_is_valid_and_distinct() {
    let a = mint_invocation_id().unwrap();
    let b = mint_invocation_id().unwrap();
    assert_eq!(validate_invocation_id(&a), Ok(()));
    assert_eq!(validate_invocation_id(&b), Ok(()));
    assert_ne!(a, b);
}
