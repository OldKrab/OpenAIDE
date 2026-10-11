use super::*;

fn invite() -> String {
    let mut bytes = iroh::SecretKey::from_bytes(&[7; 32])
        .public()
        .as_bytes()
        .to_vec();
    bytes.extend_from_slice(&[9; INVITE_SECRET_LEN]);
    format!("{INVITE_PREFIX}{}", BASE32_NOPAD.encode(&bytes))
}

#[test]
fn an_invite_survives_the_spacing_and_case_a_person_adds() {
    let code = invite();
    let typed = format!(" {}-{} ", code[..10].to_lowercase(), &code[10..]);
    let decoded = InviteCode::decode(&typed).unwrap();
    assert_eq!(decoded, InviteCode::decode(&code).unwrap());
    assert_eq!(decoded.secret, [9; INVITE_SECRET_LEN]);
}

#[test]
fn other_text_is_told_apart_from_a_damaged_invite() {
    assert_eq!(
        InviteCode::decode("hello"),
        Err(PairingCodeError::NotAPairingCode)
    );
    assert_eq!(
        InviteCode::decode("OAJ1ABCD"),
        Err(PairingCodeError::WrongKind)
    );
    let code = invite();
    assert_eq!(
        InviteCode::decode(&code[..code.len() - 4]),
        Err(PairingCodeError::Malformed)
    );
}

#[test]
fn a_label_is_cut_on_a_character_boundary() {
    let label = clamp_label(&"é".repeat(40));
    assert_eq!(label.len(), 64);
    assert_eq!(clamp_label("  a\u{7}b  "), "ab");
}
