use iroh::SecretKey;

use super::*;

/// A join-request code exactly as an OpenAIDE before ADR-0081 showed it.
const OLD_JOIN_CODE: &str = "OAJ15VESRRRI2HBMN2XJAM4JAWMVMEUVSJZ2LRR7SNRWYFDBJLEHG7IQMVDBMJWGK5AA";

fn key(seed: u8) -> PublicKey {
    SecretKey::from_bytes(&[seed; 32]).public()
}

fn invite(secret: u8) -> InviteCode {
    InviteCode {
        server: key(1),
        secret: [secret; INVITE_SECRET_LEN],
        relay: None,
    }
}

#[test]
fn invite_round_trips_and_tolerates_pasted_formatting() {
    let invite = invite(7);
    let code = invite.encode();

    assert!(code.starts_with("OAI1"));
    assert_eq!(InviteCode::decode(&code), Ok(invite.clone()));
    let pasted = format!(" {}-{} \n", code[..10].to_lowercase(), &code[10..]);
    assert_eq!(InviteCode::decode(&pasted), Ok(invite));
}

#[test]
fn a_code_from_an_older_openaide_is_told_apart_from_other_text() {
    assert_eq!(
        InviteCode::decode(OLD_JOIN_CODE),
        Err(PairingCodeError::FromOlderOpenAide)
    );
    assert_eq!(
        InviteCode::decode(&OLD_JOIN_CODE.to_lowercase()),
        Err(PairingCodeError::FromOlderOpenAide)
    );
    assert_eq!(
        InviteCode::decode("https://example.test"),
        Err(PairingCodeError::NotAPairingCode)
    );
}

#[test]
fn a_damaged_invite_is_named_as_damaged() {
    let code = invite(9).encode();

    assert_eq!(
        InviteCode::decode(&code[..code.len() - 4]),
        Err(PairingCodeError::Malformed)
    );
    assert_eq!(
        InviteCode::decode("OAI1!!!"),
        Err(PairingCodeError::Malformed)
    );
}

#[test]
fn a_code_is_recognized_by_the_key_of_the_app_server_that_made_it() {
    let invite = invite(5);

    assert!(invite.is_from(&key(1).to_string()));
    assert!(!invite.is_from(&key(2).to_string()));
    assert!(!invite.is_from("not a key"));
}

#[test]
fn labels_are_clamped_on_a_character_boundary_without_control_characters() {
    let long = "é".repeat(60);

    let clamped = clamp_label(&format!("  {long}\n"));

    assert_eq!(clamped.len(), MAX_LABEL_BYTES);
    assert_eq!(clamp_label("a\u{7}b"), "ab");
}

#[test]
fn the_secret_travels_as_base32_like_the_code() {
    let invite = invite(9);
    let request = crate::InviteRequest {
        secret: invite.secret_text().to_lowercase(),
        name: "Phone".to_string(),
        model: None,
    };

    assert_eq!(request.secret_bytes(), Some(invite.secret.to_vec()));
}

#[test]
fn an_invite_with_an_own_relay_carries_the_relay_and_its_password() {
    let relay = OwnRelay::parse("https://relay.example.com:4443/?token=friend-password").unwrap();
    let with_relay = InviteCode {
        relay: Some(relay.clone()),
        ..invite(9)
    };
    let code = with_relay.encode();

    assert!(code.starts_with("OAI2"), "{code}");
    let decoded = InviteCode::decode(&code).unwrap();
    assert_eq!(decoded, with_relay);
    assert_eq!(decoded.relay.unwrap().line(), relay.line());
    // An invite without a relay stays the code every earlier version reads.
    assert!(invite(9).encode().starts_with("OAI1"));
}

#[test]
fn a_relay_invite_with_a_damaged_relay_is_malformed() {
    let mut bytes = key(1).as_bytes().to_vec();
    bytes.extend_from_slice(&[3; INVITE_SECRET_LEN]);
    let without_relay = format!("OAI2{}", BASE32_NOPAD.encode(&bytes));
    bytes.extend_from_slice(b"https://relay.example.com/");
    let without_password = format!("OAI2{}", BASE32_NOPAD.encode(&bytes));

    assert_eq!(
        InviteCode::decode(&without_relay),
        Err(PairingCodeError::Malformed)
    );
    assert_eq!(
        InviteCode::decode(&without_password),
        Err(PairingCodeError::Malformed)
    );
}
