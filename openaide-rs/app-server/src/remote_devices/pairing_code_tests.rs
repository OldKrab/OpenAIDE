use iroh::SecretKey;

use super::*;

fn key(seed: u8) -> PublicKey {
    SecretKey::from_bytes(&[seed; 32]).public()
}

#[test]
fn invite_round_trips_and_tolerates_pasted_formatting() {
    let invite = InviteCode {
        server: key(1),
        secret: [7; INVITE_SECRET_LEN],
    };
    let code = invite.encode();

    assert!(code.starts_with("OAI1"));
    assert_eq!(InviteCode::decode(&code), Ok(invite.clone()));
    let pasted = format!(" {}-{} \n", code[..10].to_lowercase(), &code[10..]);
    assert_eq!(InviteCode::decode(&pasted), Ok(invite));
}

#[test]
fn join_request_round_trips_labels_and_omits_an_empty_model() {
    let join = JoinCode {
        device: key(2),
        name: "Kitchen tablet".to_string(),
        model: Some("Tab S9".to_string()),
    };
    assert_eq!(JoinCode::decode(&join.encode()), Ok(join));

    let unnamed_model = JoinCode {
        device: key(2),
        name: "Phone".to_string(),
        model: None,
    };
    assert_eq!(JoinCode::decode(&unnamed_model.encode()), Ok(unnamed_model));
}

#[test]
fn decoding_names_the_wrong_kind_of_code() {
    let invite = InviteCode {
        server: key(1),
        secret: [0; INVITE_SECRET_LEN],
    }
    .encode();

    assert_eq!(JoinCode::decode(&invite), Err(PairingCodeError::WrongKind));
    assert_eq!(
        JoinCode::decode("https://example.test"),
        Err(PairingCodeError::NotAPairingCode)
    );
}

#[test]
fn decoding_rejects_damaged_and_unnamed_codes() {
    let join = JoinCode {
        device: key(2),
        name: "Phone".to_string(),
        model: None,
    }
    .encode();

    assert_eq!(
        JoinCode::decode(&join[..join.len() - 3]),
        Err(PairingCodeError::Malformed)
    );
    assert_eq!(
        JoinCode::decode("OAJ1!!!"),
        Err(PairingCodeError::Malformed)
    );
    let unnamed = JoinCode {
        device: key(2),
        name: " ".to_string(),
        model: None,
    }
    .encode();
    assert_eq!(JoinCode::decode(&unnamed), Err(PairingCodeError::Malformed));
}

#[test]
fn labels_are_clamped_on_a_character_boundary_without_control_characters() {
    let long = "é".repeat(60);

    let clamped = clamp_label(&format!("  {long}\n"));

    assert_eq!(clamped.len(), MAX_LABEL_BYTES);
    assert_eq!(clamp_label("a\u{7}b"), "ab");
}

/// Other test files paste this code as a literal; it must keep decoding.
#[test]
fn the_join_code_fixture_used_by_gateway_tests_stays_valid() {
    let join = JoinCode {
        device: key(3),
        name: "Tablet".to_string(),
        model: None,
    };

    assert_eq!(
        join.encode(),
        "OAJ15VESRRRI2HBMN2XJAM4JAWMVMEUVSJZ2LRR7SNRWYFDBJLEHG7IQMVDBMJWGK5AA"
    );
}
