#![no_main]

use libfuzzer_sys::fuzz_target;
use near_crypto::{KeyType, PublicKey, Secp256K1Signature, Signature};

fuzz_target!(|data: &[u8]| {
    if data.len() < 97 {
        return;
    }

    let mut signature_bytes = [0u8; 65];
    signature_bytes.copy_from_slice(&data[..65]);

    let mut message_hash = [0u8; 32];
    message_hash.copy_from_slice(&data[65..97]);

    if let Ok(signature) = Secp256K1Signature::try_from(&signature_bytes[..]) {
        let _ = signature.check_signature_values(true);
        let _ = signature.check_signature_values(false);
        let _ = signature.recover(message_hash);
    }

    if let Ok(signature) = Signature::from_parts(KeyType::SECP256K1, &signature_bytes) {
        let public_key = PublicKey::empty(KeyType::SECP256K1);
        let _ = signature.verify(&message_hash, &public_key);
    }
});
