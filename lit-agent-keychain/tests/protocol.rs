use lit_agent_keychain::{actions, crypto, models::Signed};
use serde_json::Value;
#[test]
fn javascript_receipt_and_multichunk_cids_match_rust() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/protocol.json")).unwrap();
    let signed: Signed = serde_json::from_value(fixture["signed"].clone()).unwrap();
    let key = fixture["publicKey"].as_str().unwrap();
    let vault = signed.receipt.payload.vault_id.clone();
    crypto::verify_signed(&signed, key, &vault).unwrap();
    let mut forged = signed.clone();
    forged.document["expiresAt"] = Value::from(2000000001u64);
    assert!(crypto::verify_signed(&forged, key, &vault).is_err());
    assert!(crypto::verify_signed(&signed, key, &"00".repeat(32)).is_err());
    for case in fixture["cids"].as_array().unwrap() {
        let code = case["pattern"]
            .as_str()
            .unwrap()
            .repeat(case["repeat"].as_u64().unwrap() as usize);
        assert_eq!(actions::cid(&code), case["cid"].as_str().unwrap());
    }
}
