//! Client for querying nearcore's JSON-RPC endpoint.
//!
//! This translates Bitcoin RPC queries into NEAR RPC calls and converts
//! the responses back into Bitcoin-compatible formats.

use crate::tx_translator::YOCTO_PER_SATOSHI;
use reqwest::Client;
use serde_json::json;

pub struct NearClient {
    client: Client,
    near_rpc_url: String,
}

impl NearClient {
    pub fn new(near_rpc_url: String) -> Self {
        NearClient {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            near_rpc_url,
        }
    }

    /// Generic NEAR RPC call helper (public for direct passthrough)
    pub async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "btcrpc",
            "method": method,
            "params": params
        });

        let resp = self
            .client
            .post(&self.near_rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("NEAR RPC request failed: {}", e))?;

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse NEAR RPC response: {}", e))?;

        if let Some(err) = json.get("error") {
            return Err(format!("NEAR RPC error: {}", err));
        }

        json.get("result")
            .cloned()
            .ok_or_else(|| "Missing 'result' in NEAR RPC response".to_string())
    }

    /// Query an account's state (balance, nonce, etc.)
    pub async fn view_account(&self, account_id: &str) -> Result<AccountView, String> {
        let result = self
            .call(
                "query",
                json!({
                    "request_type": "view_account",
                    "finality": "final",
                    "account_id": account_id
                }),
            )
            .await?;

        Ok(AccountView {
            amount: result
                .get("amount")
                .and_then(|v| v.as_str())
                .unwrap_or("0")
                .to_string(),
            locked: result
                .get("locked")
                .and_then(|v| v.as_str())
                .unwrap_or("0")
                .to_string(),
            block_height: result
                .get("block_height")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            block_hash: result
                .get("block_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }

    /// Get the latest block status
    pub async fn status(&self) -> Result<NodeStatus, String> {
        let result = self.call("status", json!([])).await?;
        let sync_info = result.get("sync_info").unwrap_or(&result);

        Ok(NodeStatus {
            chain_id: result
                .get("chain_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            latest_block_height: sync_info
                .get("latest_block_height")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            latest_block_hash: sync_info
                .get("latest_block_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            latest_block_time: sync_info
                .get("latest_block_time")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            syncing: sync_info
                .get("syncing")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            validator_account_id: result
                .get("validator_account_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    /// Send a signed transaction asynchronously (returns tx hash immediately)
    pub async fn send_tx_async(&self, signed_tx_base64: &str) -> Result<String, String> {
        let result = self
            .call("broadcast_tx_async", json!([signed_tx_base64]))
            .await?;
        result
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "Expected tx hash string".to_string())
    }

    /// Send a signed transaction and wait for it to complete
    pub async fn send_tx_commit(
        &self,
        signed_tx_base64: &str,
    ) -> Result<serde_json::Value, String> {
        self.call("broadcast_tx_commit", json!([signed_tx_base64]))
            .await
    }

    /// Get transaction status
    pub async fn tx_status(
        &self,
        tx_hash: &str,
        sender_id: &str,
    ) -> Result<serde_json::Value, String> {
        self.call("tx", json!([tx_hash, sender_id])).await
    }

    /// Get block by height
    pub async fn block_by_height(&self, height: u64) -> Result<serde_json::Value, String> {
        self.call("block", json!({"block_id": height})).await
    }

    /// Get block by hash
    pub async fn block_by_hash(&self, hash: &str) -> Result<serde_json::Value, String> {
        self.call("block", json!({"block_id": hash})).await
    }

    /// Get current gas price
    pub async fn gas_price(&self) -> Result<String, String> {
        let result = self.call("gas_price", json!([null])).await?;
        result
            .get("gas_price")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Missing gas_price field".to_string())
    }

    /// View an access key (returns nonce + permission)
    pub async fn view_access_key(
        &self,
        account_id: &str,
        public_key: &str,
    ) -> Result<serde_json::Value, String> {
        self.call(
            "query",
            json!({
                "request_type": "view_access_key",
                "finality": "final",
                "account_id": account_id,
                "public_key": public_key
            }),
        )
        .await
    }

    /// Validate a Bitcoin transaction via nearcore's broadcast_bitcoin_tx RPC
    pub async fn broadcast_bitcoin_tx(&self, tx_hex: &str) -> Result<serde_json::Value, String> {
        self.call("broadcast_bitcoin_tx", json!([tx_hex])).await
    }

    /// View all access keys for an account
    pub async fn view_access_key_list(
        &self,
        account_id: &str,
    ) -> Result<serde_json::Value, String> {
        self.call(
            "query",
            json!({
                "request_type": "view_access_key_list",
                "finality": "final",
                "account_id": account_id
            }),
        )
        .await
    }

    /// Call a view function on a contract (read-only, no gas cost)
    pub async fn call_function(
        &self,
        account_id: &str,
        method_name: &str,
        args_base64: &str,
    ) -> Result<serde_json::Value, String> {
        self.call(
            "query",
            json!({
                "request_type": "call_function",
                "finality": "final",
                "account_id": account_id,
                "method_name": method_name,
                "args_base64": args_base64
            }),
        )
        .await
    }

    /// View contract state (raw key-value storage)
    pub async fn view_state(
        &self,
        account_id: &str,
        prefix_base64: &str,
    ) -> Result<serde_json::Value, String> {
        self.call(
            "query",
            json!({
                "request_type": "view_state",
                "finality": "final",
                "account_id": account_id,
                "prefix_base64": prefix_base64
            }),
        )
        .await
    }

    /// View contract code (WASM bytecode)
    pub async fn view_code(&self, account_id: &str) -> Result<serde_json::Value, String> {
        self.call(
            "query",
            json!({
                "request_type": "view_code",
                "finality": "final",
                "account_id": account_id
            }),
        )
        .await
    }

    /// Get current validators
    pub async fn validators(&self) -> Result<serde_json::Value, String> {
        self.call("validators", json!([null])).await
    }

    /// Send transaction with configurable wait_until finality
    /// wait_until: "NONE", "INCLUDED", "EXECUTED_OPTIMISTIC", "INCLUDED_FINAL", "EXECUTED", "FINAL"
    pub async fn send_tx(
        &self,
        signed_tx_base64: &str,
        wait_until: &str,
    ) -> Result<serde_json::Value, String> {
        self.call(
            "send_tx",
            json!({
                "signed_tx_base64": signed_tx_base64,
                "wait_until": wait_until
            }),
        )
        .await
    }

    /// Get transaction status with full receipts (EXPERIMENTAL_tx_status)
    pub async fn tx_status_with_receipts(
        &self,
        tx_hash: &str,
        sender_id: &str,
    ) -> Result<serde_json::Value, String> {
        self.call(
            "EXPERIMENTAL_tx_status",
            json!({
                "tx_hash": tx_hash,
                "sender_account_id": sender_id,
                "wait_until": "EXECUTED"
            }),
        )
        .await
    }

    /// Get a chunk by chunk hash
    pub async fn chunk_by_hash(&self, chunk_hash: &str) -> Result<serde_json::Value, String> {
        self.call("chunk", json!({"chunk_id": chunk_hash})).await
    }

    /// Get a chunk by block hash + shard ID
    pub async fn chunk_by_block_shard(
        &self,
        block_id: serde_json::Value,
        shard_id: u64,
    ) -> Result<serde_json::Value, String> {
        self.call("chunk", json!({"block_id": block_id, "shard_id": shard_id}))
            .await
    }

    /// Get a receipt by receipt ID
    pub async fn receipt(&self, receipt_id: &str) -> Result<serde_json::Value, String> {
        self.call("EXPERIMENTAL_receipt", json!({"receipt_id": receipt_id}))
            .await
    }

    /// Get state changes in a block
    pub async fn changes_in_block(
        &self,
        block_reference: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.call("EXPERIMENTAL_changes_in_block", block_reference)
            .await
    }

    /// Get specific state changes by type
    pub async fn changes(&self, params: serde_json::Value) -> Result<serde_json::Value, String> {
        self.call("EXPERIMENTAL_changes", params).await
    }

    /// Get protocol config at a given block
    pub async fn protocol_config(
        &self,
        block_reference: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.call("EXPERIMENTAL_protocol_config", block_reference)
            .await
    }

    /// Get genesis config
    pub async fn genesis_config(&self) -> Result<serde_json::Value, String> {
        self.call("EXPERIMENTAL_genesis_config", json!({})).await
    }

    /// Node health check
    pub async fn health(&self) -> Result<serde_json::Value, String> {
        self.call("health", json!({})).await
    }

    /// Light client execution outcome proof
    pub async fn light_client_proof(
        &self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.call("EXPERIMENTAL_light_client_proof", params).await
    }

    /// Next light client block
    pub async fn next_light_client_block(
        &self,
        last_block_hash: &str,
    ) -> Result<serde_json::Value, String> {
        self.call(
            "next_light_client_block",
            json!({"last_block_hash": last_block_hash}),
        )
        .await
    }

    /// Validators ordered by stake
    pub async fn validators_ordered(
        &self,
        block_id: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        match block_id {
            Some(id) => {
                self.call("EXPERIMENTAL_validators_ordered", json!({"block_id": id}))
                    .await
            }
            None => {
                self.call("EXPERIMENTAL_validators_ordered", json!([null]))
                    .await
            }
        }
    }

    /// Congestion level for a shard
    pub async fn congestion_level(
        &self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.call("EXPERIMENTAL_congestion_level", params).await
    }

    /// Network info from nearcore
    pub async fn network_info(&self) -> Result<serde_json::Value, String> {
        self.call("network_info", json!({})).await
    }

    /// Client config
    pub async fn client_config(&self) -> Result<serde_json::Value, String> {
        self.call("client_config", json!({})).await
    }

    /// Gas price at a specific block
    pub async fn gas_price_at_block(&self, block_id: serde_json::Value) -> Result<String, String> {
        let result = self.call("gas_price", json!([block_id])).await?;
        result
            .get("gas_price")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Missing gas_price field".to_string())
    }

    /// Query with configurable block reference
    pub async fn query_at_block(
        &self,
        request_type: &str,
        params: serde_json::Value,
        block_ref: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let mut query_params = params.as_object().cloned().unwrap_or_default();
        query_params.insert("request_type".to_string(), json!(request_type));
        // Merge block reference
        if let Some(obj) = block_ref.as_object() {
            for (k, v) in obj {
                query_params.insert(k.clone(), v.clone());
            }
        }
        self.call("query", json!(query_params)).await
    }

    /// View gas key nonces
    pub async fn view_gas_key_nonces(
        &self,
        account_id: &str,
        public_key: &str,
    ) -> Result<serde_json::Value, String> {
        self.call(
            "query",
            json!({
                "request_type": "view_gas_key_nonces",
                "finality": "final",
                "account_id": account_id,
                "public_key": public_key
            }),
        )
        .await
    }

    /// Check if the nearcore node is reachable
    pub async fn is_connected(&self) -> bool {
        self.status().await.is_ok()
    }
}

pub struct AccountView {
    pub amount: String,
    pub locked: String,
    pub block_height: u64,
    pub block_hash: String,
}

impl AccountView {
    /// Convert yoctoBIT balance to BTC-like (satoshi) value.
    /// 1 satoshi = 10^16 yoctoBIT (from genesis_builder conversion)
    pub fn balance_as_btc(&self) -> f64 {
        let yocto: u128 = self.amount.parse().unwrap_or(0);
        let satoshis = yocto / YOCTO_PER_SATOSHI;
        satoshis as f64 / 100_000_000.0 // Convert satoshis to BTC
    }

    /// Get balance in satoshis
    pub fn balance_as_satoshis(&self) -> u64 {
        let yocto: u128 = self.amount.parse().unwrap_or(0);
        (yocto / YOCTO_PER_SATOSHI) as u64
    }

    /// Get locked (staked) balance in BTC
    pub fn locked_as_btc(&self) -> f64 {
        let yocto: u128 = self.locked.parse().unwrap_or(0);
        let satoshis = yocto / YOCTO_PER_SATOSHI;
        satoshis as f64 / 100_000_000.0
    }
}

pub struct NodeStatus {
    pub chain_id: String,
    pub latest_block_height: u64,
    pub latest_block_hash: String,
    pub latest_block_time: String,
    pub syncing: bool,
    pub validator_account_id: Option<String>,
}
