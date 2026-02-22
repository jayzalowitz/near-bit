#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../src/patoshi.rs"]
#[allow(dead_code)]
mod patoshi;

fuzz_target!(|data: &[u8]| {
    if data.len() > 512 * 1024 {
        return;
    }

    let csv_reader = std::io::Cursor::new(data);
    let parsed = match patoshi::load_patoshi_addresses_from_reader(csv_reader) {
        Ok(addresses) => addresses,
        Err(_) => return,
    };

    let mut utxo_map = std::collections::BTreeMap::new();
    for (idx, address) in parsed.iter().take(512).enumerate() {
        utxo_map.insert(address.clone(), idx as u64);
    }

    let target = parsed
        .iter()
        .next()
        .cloned()
        .unwrap_or_else(|| "1PatoshiTargetAddress".to_string());
    let _ = patoshi::reassign_patoshi(&mut utxo_map, &parsed, &target);
});
