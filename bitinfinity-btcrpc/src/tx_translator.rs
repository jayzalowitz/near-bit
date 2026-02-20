//! Translate Bitcoin raw transactions to NEAR transfer actions.
//!
//! Parses standard Bitcoin transaction formats (P2PKH, P2WPKH, P2TR)
//! and extracts sender address, recipient outputs, and amounts.

use bitcoin::consensus::deserialize;
use bitcoin::script::Instruction;
use bitcoin::Transaction;
use sha2::{Digest, Sha256};

/// Conversion factor: 1 satoshi = 10^16 yoctoBIT.
/// This is set at genesis time: 1 BTC (10^8 sat) = 10^24 yoctoBIT.
pub const YOCTO_PER_SATOSHI: u128 = 10_000_000_000_000_000; // 10^16

/// A parsed Bitcoin transaction with extracted fields needed for NEAR translation.
pub struct ParsedBitcoinTx {
    /// Bitcoin txid (double SHA256, reversed hex)
    pub txid: String,
    /// Sender Bitcoin address derived from input pubkey (lowercased for NEAR account ID)
    pub sender_address: String,
    /// 33-byte compressed secp256k1 public key from sender
    pub sender_pubkey: Vec<u8>,
    /// Parsed inputs (vin) with Bitcoin Core compatible fields
    pub inputs: Vec<TxInput>,
    /// (address, satoshis) for each output
    pub outputs: Vec<TxOutput>,
    /// Original hex for caching
    pub raw_hex: String,
    /// Transaction version
    pub version: i32,
    /// Transaction locktime
    pub locktime: u32,
    /// Transaction weight (vsize calculation)
    pub weight: u64,
}

pub struct TxInput {
    /// Previous transaction hash
    pub txid: String,
    /// Previous output index
    pub vout: u32,
    /// ScriptSig hex
    pub script_sig_hex: String,
    /// ScriptSig asm (simplified)
    pub script_sig_asm: String,
    /// Witness items as hex strings
    pub txinwitness: Vec<String>,
    /// Sequence number
    pub sequence: u32,
}

pub struct TxOutput {
    /// Recipient Bitcoin address (lowercased)
    pub address: String,
    /// Amount in satoshis
    pub amount_satoshis: u64,
    /// Whether this is an OP_RETURN data output
    pub is_op_return: bool,
    /// OP_RETURN data payload (if is_op_return)
    pub op_return_data: Option<Vec<u8>>,
}

impl ParsedBitcoinTx {
    /// Parse a raw Bitcoin transaction from hex.
    /// Use `from_hex_with_hrp` for testnet/regtest bech32 HRP.
    pub fn from_hex(raw_hex: &str) -> Result<Self, String> {
        Self::from_hex_with_hrp(raw_hex, "bc")
    }

    /// Parse a raw Bitcoin transaction from hex with a custom bech32 HRP.
    pub fn from_hex_with_hrp(raw_hex: &str, bech32_hrp: &str) -> Result<Self, String> {
        let bytes = hex::decode(raw_hex).map_err(|e| format!("Invalid hex: {}", e))?;

        let tx: Transaction = deserialize(&bytes)
            .map_err(|e| format!("Failed to deserialize Bitcoin transaction: {}", e))?;

        let txid = tx.compute_txid().to_string();

        // Extract sender public key from the first input
        let (sender_pubkey, sender_address) = extract_sender_from_input(&tx, bech32_hrp)?;

        // Extract inputs with Bitcoin Core compatible fields
        let inputs = extract_inputs(&tx);

        // Extract outputs
        let outputs = extract_outputs(&tx, bech32_hrp)?;

        let version = tx.version.0;
        let locktime = tx.lock_time.to_consensus_u32();
        let weight = tx.weight().to_wu();

        Ok(ParsedBitcoinTx {
            txid,
            sender_address,
            sender_pubkey,
            inputs,
            outputs,
            raw_hex: raw_hex.to_string(),
            version,
            locktime,
            weight,
        })
    }

    /// Get the primary payment output (first non-change, non-OP_RETURN output).
    /// Change is identified as any output back to the sender address.
    pub fn payment_output(&self) -> Option<&TxOutput> {
        self.outputs
            .iter()
            .find(|o| !o.is_op_return && o.address != self.sender_address)
    }

    /// Get the total payment amount (all non-change, non-OP_RETURN outputs) in satoshis.
    pub fn total_payment_satoshis(&self) -> u64 {
        self.outputs
            .iter()
            .filter(|o| !o.is_op_return && o.address != self.sender_address)
            .map(|o| o.amount_satoshis)
            .sum()
    }

    /// Convert satoshis to yoctoBIT for NEAR transfer.
    /// 1 satoshi = 10^16 yoctoBIT (from genesis_builder conversion)
    pub fn satoshis_to_yocto(satoshis: u64) -> u128 {
        satoshis as u128 * YOCTO_PER_SATOSHI
    }

    /// Get OP_RETURN data if present (for NEAR function calls).
    pub fn op_return_data(&self) -> Option<&[u8]> {
        self.outputs
            .iter()
            .find(|o| o.is_op_return)
            .and_then(|o| o.op_return_data.as_deref())
    }

    /// Decode OP_RETURN NEAR function call.
    /// Format: "near:<contract_id>:<method_name>:<base64_args>"
    pub fn decode_near_function_call(&self) -> Option<NearFunctionCall> {
        let data = self.op_return_data()?;
        let text = std::str::from_utf8(data).ok()?;
        if !text.starts_with("near:") {
            return None;
        }
        let parts: Vec<&str> = text[5..].splitn(3, ':').collect();
        if parts.len() < 2 {
            return None;
        }
        Some(NearFunctionCall {
            contract_id: parts[0].to_string(),
            method_name: parts[1].to_string(),
            args_base64: parts.get(2).unwrap_or(&"").to_string(),
        })
    }
}

pub struct NearFunctionCall {
    pub contract_id: String,
    pub method_name: String,
    pub args_base64: String,
}

/// Extract all inputs with Bitcoin Core compatible fields.
fn extract_inputs(tx: &Transaction) -> Vec<TxInput> {
    tx.input
        .iter()
        .map(|inp| {
            let txid = inp.previous_output.txid.to_string();
            let vout = inp.previous_output.vout;
            let script_sig_hex = hex::encode(inp.script_sig.as_bytes());
            let script_sig_asm = if inp.script_sig.is_empty() {
                String::new()
            } else {
                // Simplified ASM: just show hex pushdata
                inp.script_sig
                    .instructions()
                    .filter_map(|inst| inst.ok())
                    .map(|inst| match inst {
                        Instruction::PushBytes(data) => hex::encode(data.as_bytes()),
                        Instruction::Op(op) => format!("{:?}", op),
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let txinwitness: Vec<String> = (0..inp.witness.len())
                .filter_map(|i| inp.witness.nth(i).map(|w| hex::encode(w)))
                .collect();
            let sequence = inp.sequence.0;

            TxInput {
                txid,
                vout,
                script_sig_hex,
                script_sig_asm,
                txinwitness,
                sequence,
            }
        })
        .collect()
}

/// Extract sender public key and address from the first transaction input.
fn extract_sender_from_input(
    tx: &Transaction,
    bech32_hrp: &str,
) -> Result<(Vec<u8>, String), String> {
    let first_input = tx.input.first().ok_or("Transaction has no inputs")?;

    // Try P2PKH: scriptSig contains [sig, pubkey]
    if !first_input.script_sig.is_empty() {
        let mut pushes: Vec<Vec<u8>> = Vec::new();
        for instruction in first_input.script_sig.instructions() {
            match instruction {
                Ok(Instruction::PushBytes(data)) => {
                    pushes.push(data.as_bytes().to_vec());
                }
                _ => {}
            }
        }

        // P2PKH: second push is the public key (33 or 65 bytes)
        let version_byte = p2pkh_version_byte(bech32_hrp);
        if pushes.len() >= 2 {
            let pubkey_bytes = &pushes[pushes.len() - 1];
            if pubkey_bytes.len() == 33 || pubkey_bytes.len() == 65 {
                let address = pubkey_to_p2pkh_address(pubkey_bytes, version_byte)?;
                return Ok((pubkey_bytes.clone(), address));
            }
        }

        // P2SH-P2WPKH: scriptSig has a single push (the witness script),
        // and witness contains [sig, pubkey]
        if !first_input.witness.is_empty() && first_input.witness.len() >= 2 {
            let pubkey_bytes = first_input
                .witness
                .nth(first_input.witness.len() - 1)
                .ok_or("Missing witness pubkey")?;
            if pubkey_bytes.len() == 33 || pubkey_bytes.len() == 65 {
                let address = pubkey_to_p2pkh_address(pubkey_bytes, version_byte)?;
                return Ok((pubkey_bytes.to_vec(), address));
            }
        }
    }

    // Try P2WPKH / P2TR: empty scriptSig, witness contains [sig, pubkey]
    if first_input.script_sig.is_empty() && !first_input.witness.is_empty() {
        if first_input.witness.len() >= 2 {
            // P2WPKH: witness = [sig, pubkey]
            let pubkey_bytes = first_input.witness.nth(1).ok_or("Missing witness pubkey")?;
            if pubkey_bytes.len() == 33 || pubkey_bytes.len() == 65 {
                let address = pubkey_to_p2wpkh_address(pubkey_bytes, bech32_hrp)?;
                return Ok((pubkey_bytes.to_vec(), address));
            }
        } else if first_input.witness.len() == 1 {
            // P2TR key-path spend: witness = [sig]
            // For taproot, we can't easily extract the pubkey from just the sig.
            // The pubkey would need to come from the UTXO being spent.
            return Err(
                "P2TR key-path spend: cannot extract pubkey from witness alone".to_string(),
            );
        }
    }

    Err("Could not extract sender public key from transaction inputs".to_string())
}

/// Derive a P2PKH address from a public key (lowercased for NEAR).
/// version_byte: 0x00 for mainnet, 0x6F for testnet/regtest
fn pubkey_to_p2pkh_address(pubkey: &[u8], version_byte: u8) -> Result<String, String> {
    // SHA256(pubkey)
    let sha256_hash = Sha256::digest(pubkey);

    // RIPEMD160(SHA256(pubkey))
    use ripemd::Ripemd160;
    let pubkey_hash = Ripemd160::digest(&sha256_hash);

    let mut payload = vec![version_byte];
    payload.extend_from_slice(&pubkey_hash);

    // Double SHA256 checksum
    let checksum_hash = Sha256::digest(&Sha256::digest(&payload));
    payload.extend_from_slice(&checksum_hash[..4]);

    // Base58 encode and lowercase
    Ok(bs58::encode(&payload).into_string().to_lowercase())
}

/// Get P2PKH version byte from bech32 HRP.
/// Mainnet ("bc") => 0x00, Testnet/Regtest ("tb"/"bcrt") => 0x6F
fn p2pkh_version_byte(bech32_hrp: &str) -> u8 {
    match bech32_hrp {
        "bc" => 0x00,
        _ => 0x6F, // testnet and regtest both use 0x6F
    }
}

/// Derive a P2WPKH (bech32) address from a public key.
fn pubkey_to_p2wpkh_address(pubkey: &[u8], bech32_hrp: &str) -> Result<String, String> {
    // SHA256(pubkey) then RIPEMD160
    let sha256_hash = Sha256::digest(pubkey);
    use ripemd::Ripemd160;
    let pubkey_hash = Ripemd160::digest(&sha256_hash);

    // Bech32 encode with witness version 0
    Ok(bech32_encode_local(bech32_hrp, 0, &pubkey_hash))
}

/// Bech32 encoding for Bitcoin witness addresses (BIP 173).
fn bech32_encode_local(hrp: &str, witness_version: u8, program: &[u8]) -> String {
    const CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    const GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];

    fn polymod(values: &[u8]) -> u32 {
        let mut chk: u32 = 1;
        for &v in values {
            let b = chk >> 25;
            chk = ((chk & 0x1ffffff) << 5) ^ (v as u32);
            for (i, g) in GEN.iter().enumerate() {
                if (b >> i) & 1 == 1 {
                    chk ^= g;
                }
            }
        }
        chk
    }

    fn hrp_expand(hrp: &str) -> Vec<u8> {
        let mut ret: Vec<u8> = hrp.as_bytes().iter().map(|&b| b >> 5).collect();
        ret.push(0);
        ret.extend(hrp.as_bytes().iter().map(|&b| b & 31));
        ret
    }

    let mut data5 = vec![witness_version];
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in program {
        acc = (acc << 8) | (byte as u32);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            data5.push(((acc >> bits) & 31) as u8);
        }
    }
    if bits > 0 {
        data5.push(((acc << (5 - bits)) & 31) as u8);
    }

    let mut values = hrp_expand(hrp);
    values.extend_from_slice(&data5);
    values.extend_from_slice(&[0u8; 6]);
    let polymod_val = polymod(&values) ^ 1;
    let checksum: Vec<u8> = (0..6)
        .map(|i| ((polymod_val >> (5 * (5 - i))) & 31) as u8)
        .collect();

    let mut result = String::from(hrp);
    result.push('1');
    for &d in data5.iter().chain(checksum.iter()) {
        result.push(CHARSET[d as usize] as char);
    }
    result
}

/// Extract outputs from a transaction.
fn extract_outputs(tx: &Transaction, bech32_hrp: &str) -> Result<Vec<TxOutput>, String> {
    let mut outputs = Vec::new();
    let network = match bech32_hrp {
        "bc" => bitcoin::Network::Bitcoin,
        "bcrt" => bitcoin::Network::Regtest,
        _ => bitcoin::Network::Testnet,
    };
    let params = bitcoin::params::Params::new(network);

    for txout in &tx.output {
        let script = &txout.script_pubkey;

        // Check for OP_RETURN
        if script.is_op_return() {
            let data = if script.len() > 2 {
                Some(script.as_bytes()[2..].to_vec())
            } else {
                None
            };
            outputs.push(TxOutput {
                address: String::new(),
                amount_satoshis: txout.value.to_sat(),
                is_op_return: true,
                op_return_data: data,
            });
            continue;
        }

        // Try to extract address
        let address = match bitcoin::Address::from_script(script, &params) {
            Ok(addr) => addr.to_string().to_lowercase(),
            Err(_) => {
                // Unknown script type — include with empty address
                String::new()
            }
        };

        outputs.push(TxOutput {
            address,
            amount_satoshis: txout.value.to_sat(),
            is_op_return: false,
            op_return_data: None,
        });
    }

    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_satoshis_to_yocto() {
        assert_eq!(ParsedBitcoinTx::satoshis_to_yocto(1), 10u128.pow(16));
        assert_eq!(
            ParsedBitcoinTx::satoshis_to_yocto(100_000_000),
            10u128.pow(24)
        ); // 1 BTC = 10^24 yoctoBIT
    }

    #[test]
    fn test_pubkey_to_p2pkh_address() {
        // Known test vector: compressed pubkey for Satoshi's address
        // This is a basic sanity check that the function produces a lowercased result
        let fake_pubkey = vec![0x02; 33]; // dummy compressed pubkey
        let result = pubkey_to_p2pkh_address(&fake_pubkey, 0x00);
        assert!(result.is_ok());
        let addr = result.unwrap();
        // Should start with '1' and be all lowercase
        assert!(addr.starts_with('1'));
        assert_eq!(addr, addr.to_lowercase());
    }
}
