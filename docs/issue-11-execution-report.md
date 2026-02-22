# Issue #11 Execution Report (2026-02-20)

This report documents what was executed in this workspace toward:
- Issue #11: Launch plan hardening tasks
- Issue #1: Core architecture/UX goals

## Incremental Update (2026-02-21)

Additional logical commits were pushed on `infinitoshi/btc-near-fork-plan` for RPC compatibility hardening and validation depth:

- `e9abbf283`
  - `listunspent` address-filter parameter validation tightened (array type + string entry checks).
  - E2E invalid-address filter check added (`-32602`).
- `5a486709d`
  - `createrawtransaction` now rejects malformed/non-positive output amounts and empty destination sets.
  - `signrawtransactionwithwallet` intent parser now rejects missing address and non-positive/non-numeric amounts.
  - E2E invalid-path checks added for both handlers.
- `209c08a29`
  - Shared PSBT output parsing (`createpsbt` + `walletcreatefundedpsbt`) hardened:
    - rejects missing outputs, invalid format, non-object array entries, non-positive/non-numeric amounts, and empty destination sets.
  - E2E invalid-output checks added for both methods (`-32602`).
- `86e5bd5e0`
  - E2E invalid-path checks added for `decodepsbt`, `analyzepsbt`, `finalizepsbt`, and `walletprocesspsbt` invalid-base64 paths (`-22`).
- `044df1c45`
  - Unit tests added for `createpsbt` invalid output payload handling.
- `92974de55`
  - Unit tests added for `createrawtransaction` invalid output payload handling.
- `4dd56ace2`
  - `generatetodescriptor` standardized to `-32601` not-supported code for consistency with related PoS mining stubs.
  - E2E now asserts intentional-stub behavior for:
    - `getmininginfo` PoS fields
    - `getblocktemplate`, `generate`, `generatetoaddress`, `generatetodescriptor`
    - `addnode`, `disconnectnode`, `onetry`
- `4c1640a71`
  - Tier-3 E2E coverage added for:
    - `getblock` (verbosity `0/1/2`)
    - `getblockstats` (valid + invalid path)
    - `getchaintips`
    - `getrawmempool` (verbose/non-verbose)
    - `getmempoolentry` unknown tx path
    - `getmempoolancestors` / `getmempooldescendants`
- `2236cd54e`
  - Auth-depth E2E expanded to include Tier-2 `createpsbt` method:
    - no-auth -> HTTP 401
    - wrong auth -> HTTP 401
    - correct auth -> success

Verification reruns for this incremental set:

- `cargo test -q -p bitinfinity-tools`
- `cargo test -q -p bitinfinity-neard`
- `cargo test -q -p bitinfinity-btcrpc`
- `cargo test -q -p node-runtime --manifest-path nearcore/Cargo.toml patoshi`
- `cargo test -q -p node-runtime --manifest-path nearcore/Cargo.toml test_maybe_auto_register_bitcoin_access_key_non_bitcoin_signer_noop`
- `cargo test -q -p node-runtime --manifest-path nearcore/Cargo.toml test_maybe_auto_register_bitcoin_access_key_rejects_invalid_signature`
- `./scripts/e2e_testnet.sh`

All passed.

## Incremental Update (2026-02-21, continued)

Additional logical commits pushed after the above update:

- `d19fca98f`
  - Added E2E coverage for quantum-key RPC skeleton methods:
    - `addquantumkey`, `removequantumkey`, `listquantumkeys`
    - duplicate key rejection, invalid keytype rejection, max-keys rejection, remove-missing rejection.
- `94b1a0c0d`
  - Added `quantum_enforcement_active: false` to `getblockchaininfo` response (connected + fallback paths).
  - E2E now asserts presence/value of this field.
- `3dac363ce`
  - Added E2E coverage for `getblockhash` valid path and out-of-range error path (`-8`).
  - Added E2E unknown-hash error assertion for `getblock` (`-5`).
- `a2a00acfe`
  - Hardened quantum-key RPCs to validate Bitcoin address parameters:
    - `addquantumkey`, `removequantumkey`, and `listquantumkeys` now return `-5` on invalid addresses.
  - Added matching E2E invalid-address assertions for all three methods.
- `cd485ce2c`
  - Added E2E assertion that `addquantumkey` rejects invalid `pubkey_hex` with `-32602`.

Verification reruns (this continued increment):

- `cargo test -q -p bitinfinity-btcrpc`
- `./scripts/e2e_testnet.sh`

All passed.

## Incremental Update (2026-02-21, continued again)

Additional logical commits pushed after the previous continuation:

- `72e55693b`
  - Added persistent quantum key registry (`~/.bitinfinity/quantum_keys.json`):
    - load on btcrpc startup
    - save on `addquantumkey` / `removequantumkey`.
  - Added E2E assertion that registry file exists with expected key count.
- `34d83994f`
  - Hardened `removequantumkey` input validation:
    - invalid keytype -> `-32602`
    - invalid pubkey hex -> `-32602`.
  - Added matching E2E checks.
- `322294971`
  - Added btcrpc unit tests for `removequantumkey` invalid keytype/hex rejection paths.
- `55890728b`
  - Extended E2E to restart btcrpc mid-run and verify quantum keys persist/reload after restart.
- `e692dc447`
  - Added `getblock` bool verbosity compatibility:
    - `false` maps to verbosity `0`
    - `true` maps to verbosity `1`.
  - Added E2E checks for bool verbosity paths.

Verification reruns (this continuation):

- `cargo test -q -p bitinfinity-btcrpc`
- `./scripts/e2e_testnet.sh`

All passed.

## Incremental Update (2026-02-21, alias compatibility hardening)

Additional logical commits pushed after the previous continuation:

- `6488c2fb1`
  - Quantum-key RPC state now aliases canonical mixed-case and legacy lowercase Base58 address forms:
    - `addquantumkey`, `removequantumkey`, and `listquantumkeys` now merge/sync case-variant entries for the same P2PKH/P2SH identity.
  - Added unit tests:
    - `test_listquantumkeys_supports_legacy_lowercase_alias`
    - `test_removequantumkey_supports_legacy_lowercase_alias`
- `0da9a31d0`
  - Extended E2E quantum coverage for lowercase alias behavior:
    - `listquantumkeys(lowercase_alias)` returns canonical key set.
    - `removequantumkey(lowercase_alias, ...)` removes canonical registrations.
  - Kept script portability on macOS default Bash by using `tr` for lowercase conversion.

Verification reruns (this continuation):

- `cargo test -q -p bitinfinity-btcrpc` (44 passing)
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-tools`
- `cargo test -q -p bitinfinity-neard`
- `cargo test -q -p node-runtime --manifest-path nearcore/Cargo.toml test_maybe_auto_register_bitcoin_access_key_non_bitcoin_signer_noop`
- `cargo test -q -p node-runtime --manifest-path nearcore/Cargo.toml test_maybe_auto_register_bitcoin_access_key_rejects_invalid_signature`
- `cargo test -q -p node-runtime --manifest-path nearcore/Cargo.toml patoshi_unlock`

All passed.

## Incremental Update (2026-02-21, continued once more)

Additional logical commits pushed after the previous continuation:

- `1fb3b04d7`
  - Added Tier-2 PSBT invalid-path coverage in E2E:
    - `combinepsbt` malformed/non-base64 candidate set now explicitly asserted to return `-22`.
  - Added `psbt_combine_invalid_error_code` to E2E summary output.
- `f8623b2ea`
  - Extended quantum-key restart validation in E2E:
    - after btcrpc restart, `listquantumkeys` is now checked for both canonical and lowercase alias forms, and counts must match.
  - Added `quantum_after_restart_alias_count` to E2E summary output.

Verification reruns (this continuation):

- `./scripts/e2e_testnet.sh`

Passed.

## Incremental Update (2026-02-21, continued auth and alias depth)

Additional logical commits pushed after the previous continuation:

- `7ce8e3817`
  - Added unit test coverage for reverse quantum alias flow:
    - canonical lookup after lowercase-alias registration.
- `bf6fc4694`
  - Extended E2E quantum flow to register first key via lowercase alias and assert canonical visibility immediately.
  - Added `quantum_after_alias_add_count` to summary.
- `88d5e60bb`
  - Extended auth-depth E2E checks to include `sendrawtransaction` write path:
    - no-auth -> `401`
    - wrong auth -> `401`
    - correct auth -> `200`
  - Added `auth_sendraw_*` summary fields.
- `12660f9a7`
  - Added E2E empty-destination invalid-path assertions (`-32602`) for:
    - `createrawtransaction`
    - `createpsbt`
    - `walletcreatefundedpsbt`
  - Added corresponding summary fields for these rejection paths.

Verification reruns (this continuation):

- `cargo test -q -p bitinfinity-btcrpc` (45 passing)
- `./scripts/e2e_testnet.sh`

All passed.

## Incremental Update (2026-02-21, continued auth/registry hardening)

Additional logical commits pushed after the previous continuation:

- `68dec9d25`
  - Extended auth-depth E2E checks for `sendtoaddress`:
    - no-auth -> `401`
    - wrong auth -> `401`
    - correct auth -> `200` with structured JSON-RPC response.
  - Added `auth_sendtoaddress_*` summary fields.
- `6ab6a35df`
  - Added E2E on-disk quantum registry parity assertion:
    - lowercase alias entry count in `~/.bitinfinity/quantum_keys.json` must match canonical entry count.
  - Added `quantum_registry_alias_count` summary field.
- `b198cfe55`
  - Extended auth-depth E2E checks for `walletprocesspsbt`:
    - no-auth -> `401`
    - wrong auth -> `401`
    - correct auth -> `200` with structured JSON-RPC response.
  - Added `auth_walletprocesspsbt_*` summary fields.

Verification reruns (this continuation):

- `./scripts/e2e_testnet.sh`

Passed.

## Incremental Update (2026-02-21, continued auth-depth expansion)

Additional logical commit pushed after the previous continuation:

- `02ee5d869`
  - Extended auth-depth E2E checks to `addquantumkey`:
    - no-auth -> `401`
    - wrong auth -> `401`
    - correct auth -> `200` with structured JSON-RPC response.
  - Added summary fields:
    - `auth_addquantumkey_noauth_http_code`
    - `auth_addquantumkey_wrong_http_code`
    - `auth_addquantumkey_ok_http_code`

Verification rerun (this continuation):

- `./scripts/e2e_testnet.sh`

Passed.

## Incremental Update (2026-02-21, continued quantum auth-depth coverage)

Additional logical commit pushed after the previous continuation:

- `2021b1a86`
  - Extended auth-depth E2E checks for quantum RPC methods:
    - `removequantumkey`: no-auth `401`, wrong auth `401`, correct auth `200`
    - `listquantumkeys`: no-auth `401`, wrong auth `401`, correct auth `200`
  - Added summary fields:
    - `auth_removequantumkey_*`
    - `auth_listquantumkeys_*`

Verification rerun (this continuation):

- `./scripts/e2e_testnet.sh`

Passed.

## Incremental Update (2026-02-21, continued quantum alias unit coverage)

Additional logical commit pushed after the previous continuation:

- `84267032a`
  - Added btcrpc unit coverage for duplicate detection across canonical/lowercase alias forms:
    - register key via lowercase alias
    - re-register same key via canonical address
    - assert rejection with `-32602`.
  - New test:
    - `test_addquantumkey_rejects_duplicate_across_canonical_and_lowercase_alias`

Verification rerun (this continuation):

- `cargo test -q -p bitinfinity-btcrpc` (46 passing)

Passed.

## Incremental Update (2026-02-21, continued coin-control auth-depth)

Additional logical commit pushed after the previous continuation:

- `a6edfe43b`
  - Extended auth-depth E2E checks for `lockunspent` coin-control write method:
    - no-auth -> `401`
    - wrong auth -> `401`
    - correct auth -> `200` with structured JSON-RPC response.
  - Added summary fields:
    - `auth_lockunspent_noauth_http_code`
    - `auth_lockunspent_wrong_http_code`
    - `auth_lockunspent_ok_http_code`

Verification rerun (this continuation):

- `./scripts/e2e_testnet.sh`

Passed.

## Incremental Update (2026-02-21, continued alias de-dup unit coverage)

Additional logical commit pushed after the previous continuation:

- `56825e913`
  - Added btcrpc unit test coverage for `listquantumkeys` de-dup behavior when canonical and lowercase alias entries coexist in storage:
    - overlapping key tuples are returned once
    - unique tuples across both entries are preserved.
  - New test:
    - `test_listquantumkeys_deduplicates_alias_storage_entries`

Verification rerun (this continuation):

- `cargo test -q -p bitinfinity-btcrpc` (47 passing)

Passed.

## Incremental Update (2026-02-21, continued signing auth-depth)

Additional logical commit pushed after the previous continuation:

- `6618541b2`
  - Extended auth-depth E2E checks for `signrawtransactionwithwallet`:
    - no-auth -> `401`
    - wrong auth -> `401`
    - correct auth -> `200` with structured JSON-RPC response.
  - Added summary fields:
    - `auth_signraw_noauth_http_code`
    - `auth_signraw_wrong_http_code`
    - `auth_signraw_ok_http_code`

Verification rerun (this continuation):

- `./scripts/e2e_testnet.sh`

Passed.

## What was implemented in this change set

### 1) Canonical Bitcoin address support (Issue #1 critical)

Implemented:
- Canonical Base58Check address casing is now preserved end-to-end.
- Legacy lowercased Base58 account IDs remain accepted for backward compatibility.
- Address checksum validation is now enforced in account-id parsing (Base58Check + SegWit decode).

Files:
- `near-account-id/src/validation.rs`
- `near-account-id/src/account_id_ref.rs`
- `near-account-id/src/lib.rs`
- `bitinfinity-tools/src/genesis_builder.rs`
- `bitinfinity-btcrpc/src/main.rs`
- `nearcore/core/crypto/src/bitcoin_utils.rs`
- `nearcore/chain/jsonrpc/src/lib.rs`
- `nearcore/runtime/runtime/src/bitcoin_tx.rs`

### 2) Tier-1 RPC behavior alignment (Issue #11 / 0.5 support)

Implemented:
- Removed forced address lowercasing in wallet/address RPC paths to avoid Base58 corruption.
- Added compatibility aliases where keys are stored, so older lowercased addresses still function.
- Hardened `getaddressinfo` fields (`iswatch`, `iswatchonly`, `witness_version`, `witness_program`) for wallet compatibility.

Primary file:
- `bitinfinity-btcrpc/src/main.rs`

### 3) Parser hardening (Issue #11 / 0.2 support)

Implemented:
- Account ID parser now validates canonical Bitcoin address checksums and SegWit formatting.
- Added tests for canonical valid addresses and invalid-checksum rejection.

Primary file:
- `near-account-id/src/validation.rs`

### 4) Intentional-stub compliance for PoS chain semantics (Issue #11 / 0.5 support)

Implemented:
- `generate` / `generatetoaddress` now return explicit not-supported errors (no CPU mining on PoS).
- `getblocktemplate` now returns explicit not-supported error for PoW template flow.
- `addnode` / `disconnectnode` / `onetry` now return explicit not-supported errors for peer-management operations handled by nearcore.
- `getmininginfo` now reports PoS-oriented fields and zeroed PoW-specific metrics.

Primary file:
- `bitinfinity-btcrpc/src/main.rs`

### 5) Runtime Patoshi floor enforcement (Issue #11 / 0.3, Issue #10 dependency)

Implemented:
- Added `PatoshiRecord` runtime state (Borsh) under account contract-data key `bitinfinity:patoshi:v1`.
- Added signer-side Patoshi guard in nearcore transaction processing:
  - locked accounts may only `Stake` (self-receiver) or `Transfer` (foundation-only),
  - all other action types are rejected pre-receipt,
  - post-charge balance floor (`>= genesis_balance`) is enforced.
- Added receipt-time `DelegateAction` rejection for locked Patoshi accounts.
- Added unit tests for lock state transitions and guarded action/floor checks.
- Added runtime unlock scheduling path:
  - canonical unlock trigger is a single zero-value transfer to foundation,
  - schedules `unlock_epoch = current_epoch + 14` once and persists state on-chain.
- Added epoch-boundary Patoshi sweep path:
  - reads Patoshi account index from state,
  - computes excess over genesis floor from total balance,
  - sweeps available excess from liquid balance to foundation each epoch,
  - logs locked unswept remainder for visibility.

Primary files:
- `nearcore/runtime/runtime/src/bitcoin_tx.rs`
- `nearcore/runtime/runtime/src/lib.rs`
- `nearcore/runtime/runtime/src/actions.rs`

### 6) Patoshi unlock flow wired end-to-end (Issue #10 / Issue #11 0.3)

Implemented:
- `patoshiunlock` now:
  - builds challenge message `bitcoin-infinity-unlock:<genesis_block_hash>`,
  - verifies Bitcoin message signature against address/challenge,
  - submits canonical unlock-trigger transaction on-chain (zero-value transfer to foundation),
  - returns submitted NEAR tx hash + timelock metadata.
- Runtime consumes the unlock trigger and writes `unlock_epoch`, activating delayed unlock semantics.
- Added btcrpc unit tests for message-signature verification path (valid signature + wrong message + wrong address).

Primary files:
- `bitinfinity-btcrpc/src/main.rs`
- `nearcore/runtime/runtime/src/bitcoin_tx.rs`
- `nearcore/runtime/runtime/src/lib.rs`

### 7) Patoshi genesis registry + key flow hardening (Issue #1 + #11)

Implemented:
- `bitinfinity-tools generate-genesis` now auto-generates a dedicated secp256k1 Bitcoin keypair for Patoshi reassignment (instead of reassigning to validator account).
- Generated keypair is written to `patoshi-keypair.txt` in output directory (unix perms tightened to `0600`).
- Genesis writer now supports `StateRecord::Data` and writes:
  - per-account Patoshi lock records (`PatoshiRecord`, Borsh/base64),
  - global Patoshi account index (`bitinfinity:patoshi:index:v1`) for epoch sweeps.
- Added tests verifying Patoshi data records are present and decode correctly.

Primary files:
- `bitinfinity-tools/src/main.rs`
- `bitinfinity-tools/src/genesis_builder.rs`
- `bitinfinity-tools/Cargo.toml`

### 8) TPS claim qualification + benchmark methodology scaffold (Issue #11 / 0.6 partial)

Implemented:
- Qualified website throughput wording from unbounded `>1,000,000 TPS` language to a per-shard scaling statement.
- Added benchmark methodology doc with:
  - measurement definitions,
  - required profiles (1k/10k/50k),
  - required raw artifact publication rules,
  - explicit wording guidelines to avoid unmeasured aggregate TPS claims.
- Executed and published pilot benchmark runs (February 20, 2026) with artifact references:
  - baseline pilot (`1000 TPS`, `60s`) achieved `avg_tps_from_log=857.586`, `peak_tps_from_log=1035.614`
  - native-TPS multi-profile pilot (`20s/profile`) achieved:
    - baseline target `1000`: `avg=620.265`, `peak=886.428`
    - stress target `10000`: `avg=6143.507`, `peak=8844.634`
    - peak target `50000`: `avg=8935.701`, `peak=12648.842`

Primary files:
- `docs/index.html`
- `docs/benchmark-methodology.md`
- `README.md`

### 9) Fuzz harness hardening for parser/tx paths (Issue #11 / 0.2 partial)

Implemented:
- Fixed `bitinfinity-btcrpc/fuzz` workspace configuration so fuzz crate builds standalone.
- Expanded account-id fuzz target to exercise:
  - UTF-8 lossy inputs, null-byte injection, redundant separators,
  - casing transforms, trim behavior, and bounded long-string variants,
  - `AccountId::validate`, `AccountId::from_str`, and `AccountIdRef::new`.
- Expanded RPC parser fuzz target to exercise typed JSON-RPC decode:
  - single-request and batch request decoding,
  - positional parameter extraction (`str`/`u64`) and object key extraction.
- Expanded raw-tx hex fuzz target to exercise odd-length/truncated variants and decode attempts from both raw bytes and decoded hex payloads.
- Added dedicated `tx_translator` fuzz target to exercise:
  - `ParsedBitcoinTx::from_hex` / `from_hex_with_hrp` across `bc|tb|bcrt`,
  - sender/output extraction, payment aggregation, OP_RETURN decode path.
- Added starter corpora for account-id, rpc-parse, tx-hex, and tx-translator targets.

Primary files:
- `near-account-id/fuzz/fuzz_targets/fuzz_account_id_parse.rs`
- `bitinfinity-btcrpc/fuzz/Cargo.toml`
- `bitinfinity-btcrpc/fuzz/fuzz_targets/fuzz_rpc_parse.rs`
- `bitinfinity-btcrpc/fuzz/fuzz_targets/fuzz_tx_hex.rs`
- `bitinfinity-btcrpc/fuzz/fuzz_targets/fuzz_tx_translator.rs`
- `near-account-id/fuzz/corpus/...`
- `bitinfinity-btcrpc/fuzz/corpus/...`

### 10) CI enforcement for fuzz smoke + scheduled nightly matrix (Issue #11 / 0.2 partial)

Implemented:
- Expanded CI smoke fuzz job to run all active targets:
  - `fuzz_account_id_parse`
  - `fuzz_rpc_parse`
  - `fuzz_tx_hex`
  - `fuzz_tx_translator`
- Added dedicated scheduled nightly workflow (`.github/workflows/nightly-fuzz.yml`) with matrix execution across all fuzz targets.
- Added manual dispatch input `max_total_time` to control per-target nightly runtime without code changes.

Primary files:
- `.github/workflows/ci.yml`
- `.github/workflows/nightly-fuzz.yml`
- `README.md`

### 11) Benchmark execution runner + artifact contract (Issue #11 / 0.6 progress)

Implemented:
- Added executable benchmark profile runner:
  - `scripts/benchmark/run_tps_profiles.sh`
  - supports `baseline|stress|peak|all` profiles (1k/10k/50k TPS),
  - supports build-skipping and dry-run planning (`--skip-build`, `--dry-run`),
  - captures per-profile config/genesis/logs/metrics and emits aggregate `summary.json`, `summary.csv`, `summary.md`.
- Hardened runner portability for macOS default Bash by removing associative arrays.
- Added CI benchmark runner dry-run smoke job to prevent script regressions on PRs.
- Documented runner usage and artifact schema in benchmark docs and README.
- Added benchmark artifact ignore rule for generated outputs.
- Added cross-platform log parsing (macOS/GNU compatible) for observed TPS extraction:
  - `avg_tps_from_log`
  - `peak_tps_from_log`
  - final included/failed counts from tx-generator log output.
- Added explicit shutdown diagnostics in benchmark summaries:
  - `schedule_completed_from_log`
  - `signal_11_from_log`
- Added strict benchmark exit enforcement:
  - script now exits non-zero when any profile has non-zero `effective_run_status`,
  - `--allow-nonzero-run-status` preserves zero script exit for exploratory runs while retaining diagnostics.
- Added controller-mode stabilization and timeout correctness:
  - controller-enabled schedules are now default (`--disable-controller` to opt out),
  - runner terminates benchmark processes without killing itself (child-tree termination instead of process-group kill),
  - added startup timeout (`--startup-timeout`, default `900s`) so setup time is not charged against profile runtime,
  - introduced `effective_run_status` for pass/fail accounting (`run_status=143` after schedule-complete graceful stop is normalized to `0`).

Primary files:
- `scripts/benchmark/run_tps_profiles.sh`
- `.github/workflows/ci.yml`
- `docs/benchmark-methodology.md`
- `README.md`
- `.gitignore`

## Verification run

Passed:
- `cargo test --manifest-path near-account-id/Cargo.toml`
- `cargo test -p bitinfinity-tools`
- `cargo test -p bitinfinity-btcrpc`
- `cargo check -p near-crypto -p near-jsonrpc --manifest-path nearcore/Cargo.toml`
- `cargo check -p node-runtime --manifest-path nearcore/Cargo.toml --lib`
- `cargo test -p node-runtime --manifest-path nearcore/Cargo.toml patoshi_unlock`
- `cargo test -p node-runtime --manifest-path nearcore/Cargo.toml --features test_features bitcoin_tx::tests`
- `cargo check --manifest-path near-account-id/fuzz/Cargo.toml`
- `cargo check --manifest-path bitinfinity-btcrpc/fuzz/Cargo.toml`
- `cargo +nightly fuzz run fuzz_account_id_parse -- -runs=100`
- `cargo +nightly fuzz run fuzz_rpc_parse -- -runs=100`
- `cargo +nightly fuzz run fuzz_tx_hex -- -runs=100`
- `cargo +nightly fuzz run fuzz_tx_translator -- -runs=100`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml")'`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/nightly-fuzz.yml")'`
- `bash -n scripts/benchmark/run_tps_profiles.sh`
- `./scripts/benchmark/run_tps_profiles.sh --dry-run --skip-build --profile baseline --metrics-interval 1`
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 120 --duration-override 10 --run-grace 20 --num-accounts 50 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/live-smoke-verify2-20260220T175708Z`
- `./scripts/benchmark/run_tps_profiles.sh --profile all --tps-override 120 --duration-override 8 --run-grace 20 --num-accounts 50 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/live-smoke-all-20260220T175814Z`
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 1000 --duration-override 60 --run-grace 45 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/pilot-baseline-1000-20260220T180109Z`
- `./scripts/benchmark/run_tps_profiles.sh --profile all --duration-override 20 --run-grace 45 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/pilot-all-native-tps-20260220T180236Z`
- `./scripts/benchmark/run_tps_profiles.sh --profile stress --tps-override 10000 --duration-override 60 --run-grace 60 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/pilot-stress-10000-60s-20260220T181011Z`
- `./scripts/benchmark/run_tps_profiles.sh --profile peak --tps-override 50000 --duration-override 60 --run-grace 60 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/pilot-peak-50000-60s-20260220T181137Z`
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 200 --duration-override 6 --run-grace 20 --num-accounts 50 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/smoke-diagnostics-20260220T181345Z`
- `./scripts/benchmark/run_tps_profiles.sh --profile stress --tps-override 10000 --duration-override 60 --run-grace 60 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/strict-check-stress60-20260220T181723Z` (expected strict failure, observed exit code `2`)
- `./scripts/benchmark/run_tps_profiles.sh --profile stress --tps-override 10000 --duration-override 60 --run-grace 60 --num-accounts 500 --metrics-interval 1 --skip-build --allow-nonzero-run-status --out-dir artifacts/benchmarks/strict-check-stress60-allow-20260220T181852Z` (override path, observed exit code `0`)
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 50 --duration-override 2 --run-grace 5 --startup-timeout 300 --num-accounts 10 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/post-fix2-short-20260220T183753Z`
- `./scripts/benchmark/run_tps_profiles.sh --profile stress --tps-override 10000 --duration-override 60 --run-grace 120 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/post-fix-stress60-20260220T183159Z`
- `./scripts/benchmark/run_tps_profiles.sh --profile peak --tps-override 50000 --duration-override 60 --run-grace 120 --startup-timeout 900 --num-accounts 500 --metrics-interval 1 --skip-build --out-dir artifacts/benchmarks/post-fix2-peak60-20260220T183819Z`

Benchmark smoke artifact evidence:
- `artifacts/benchmarks/live-smoke-verify2-20260220T175708Z/summary.json`
- `artifacts/benchmarks/live-smoke-verify2-20260220T175708Z/summary.csv`
- `artifacts/benchmarks/live-smoke-verify2-20260220T175708Z/summary.md`
- `artifacts/benchmarks/live-smoke-verify2-20260220T175708Z/baseline/summary.json`
- `artifacts/benchmarks/live-smoke-all-20260220T175814Z/summary.json`
- `artifacts/benchmarks/live-smoke-all-20260220T175814Z/summary.csv`
- `artifacts/benchmarks/live-smoke-all-20260220T175814Z/summary.md`
- `artifacts/benchmarks/pilot-baseline-1000-20260220T180109Z/summary.json`
- `artifacts/benchmarks/pilot-baseline-1000-20260220T180109Z/summary.csv`
- `artifacts/benchmarks/pilot-baseline-1000-20260220T180109Z/summary.md`
- `artifacts/benchmarks/pilot-all-native-tps-20260220T180236Z/summary.json`
- `artifacts/benchmarks/pilot-all-native-tps-20260220T180236Z/summary.csv`
- `artifacts/benchmarks/pilot-all-native-tps-20260220T180236Z/summary.md`
- `artifacts/benchmarks/pilot-stress-10000-60s-20260220T181011Z/summary.json`
- `artifacts/benchmarks/pilot-stress-10000-60s-20260220T181011Z/summary.csv`
- `artifacts/benchmarks/pilot-stress-10000-60s-20260220T181011Z/summary.md`
- `artifacts/benchmarks/pilot-peak-50000-60s-20260220T181137Z/summary.json`
- `artifacts/benchmarks/pilot-peak-50000-60s-20260220T181137Z/summary.csv`
- `artifacts/benchmarks/pilot-peak-50000-60s-20260220T181137Z/summary.md`
- `artifacts/benchmarks/smoke-diagnostics-20260220T181345Z/summary.json`
- `artifacts/benchmarks/strict-check-stress60-20260220T181723Z/summary.json`
- `artifacts/benchmarks/strict-check-stress60-allow-20260220T181852Z/summary.json`
- `artifacts/benchmarks/post-fix2-short-20260220T183753Z/summary.json`
- `artifacts/benchmarks/post-fix2-short-20260220T183753Z/summary.csv`
- `artifacts/benchmarks/post-fix2-short-20260220T183753Z/summary.md`
- `artifacts/benchmarks/post-fix-stress60-20260220T183159Z/summary.json`
- `artifacts/benchmarks/post-fix-stress60-20260220T183159Z/summary.csv`
- `artifacts/benchmarks/post-fix-stress60-20260220T183159Z/summary.md`
- `artifacts/benchmarks/post-fix2-peak60-20260220T183819Z/summary.json`
- `artifacts/benchmarks/post-fix2-peak60-20260220T183819Z/summary.csv`
- `artifacts/benchmarks/post-fix2-peak60-20260220T183819Z/summary.md`

Observed (smoke profile):
- target: `120 TPS` for `10s`
- run status: `0` (completed)
- timed_out: `0`
- avg TPS from log: `52.654`
- peak TPS from log: `78.968`
- final success metric: `1040`
- final failed metric: `0`

Observed (`--profile all` smoke):
- profiles emitted: `3` (`baseline`, `stress`, `peak`)
- each profile run status: `0`
- each profile timed_out: `0`
- observed TPS fields populated (non-null) in all per-profile summaries

Observed (baseline pilot @ `1000 TPS`, `60s`):
- run status: `0` (completed)
- timed_out: `0`
- avg TPS from log: `857.586`
- peak TPS from log: `1035.614`
- final success metric: `58856`
- final failed metric: `0`

Observed (native-TPS multi-profile pilot @ `20s/profile`):
- baseline (`1000` target): `avg=620.265`, `peak=886.428`, `failed_metric=0`
- stress (`10000` target): `avg=6143.507`, `peak=8844.634`, `failed_metric=0`
- peak (`50000` target): `avg=8935.701`, `peak=12648.842`, `failed_metric=0`

Observed (extended 60s high-load pilots, pre-mitigation historical runs):
- stress (`10000` target): `avg=8551.340`, `peak=10328.730`, `final_success_metric=595345`, `run_status=139`, `schedule_completed_from_log=1`, `signal_11_from_log=1`
- peak (`50000` target): `avg=12353.138`, `peak=14985.768`, `final_success_metric=843928`, `run_status=139`, `schedule_completed_from_log=1`, `signal_11_from_log=1`
- log evidence includes: `error: Recipe \`run-localnet\` was terminated on line 19 by signal 11`
- strict-exit validation:
  - strict mode run returned exit code `2` when `nonzero_profile_count=1`
  - `--allow-nonzero-run-status` run returned exit code `0` with the same diagnostics preserved

Observed (post-mitigation controller-mode runs):
- short profile sanity (`50 TPS`, `2s`): `run_status=143`, `effective_run_status=0`, `timed_out=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`
- stress (`10000` target, `60s`): `avg=8331.414`, `peak=11059.288`, `final_success_metric=566478`, `run_status=143`, `effective_run_status=0`, `timed_out=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`
- peak (`50000` target, `60s`): `avg=8798.048`, `peak=11576.004`, `final_success_metric=619419`, `run_status=143`, `effective_run_status=0`, `timed_out=0`, `schedule_completed_from_log=1`, `signal_11_from_log=0`
- strict mode now passes for these runs because `nonzero_profile_count` is derived from `effective_run_status`.

Notes:
- Installed `rust-src` for the active toolchain, unblocking wasm `-Zbuild-std` test dependencies.
- Runtime Bitcoin address detection tests now use checksum-valid canonical vectors (Taproot + P2SH), and `bitcoin_tx::tests` is green.
- Installed `cargo-fuzz` and nightly toolchain to execute sanitizer-backed fuzz smoke runs.

## Continuation (2026-02-21): walletcreatefundedpsbt auth-gating coverage

Implemented:
- Extended `scripts/e2e_testnet.sh` auth-depth coverage to include `walletcreatefundedpsbt` with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - correct-credential request returns HTTP `200` and preserves JSON-RPC `id`.
- Added corresponding summary exports for this method:
  - `auth_walletcreatefundedpsbt_noauth_http_code`
  - `auth_walletcreatefundedpsbt_wrong_http_code`
  - `auth_walletcreatefundedpsbt_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): listlockunspent auth-gating coverage

Implemented:
- Extended `scripts/e2e_testnet.sh` auth-depth checks to include `listlockunspent` with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - correct-credential request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports for the new checks:
  - `auth_listlockunspent_noauth_http_code`
  - `auth_listlockunspent_wrong_http_code`
  - `auth_listlockunspent_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `./scripts/e2e_testnet.sh` (one initial run hit transient `AddrInUse` on `3030/24567`; clean rerun passed)
- `HOME="$(mktemp -d)" RUSTUP_HOME="/Users/infinitoshi/.rustup" CARGO_HOME="/Users/infinitoshi/.cargo" cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): stabilize quantum-key unit test isolation

Implemented:
- Hardened `bitinfinity-btcrpc` quantum-key unit tests against shared file-state interference:
  - added a test helper that acquires a process-local lock for quantum registry tests,
  - removes the on-disk quantum registry file before and after each covered test,
  - runs all alias/duplicate/removal quantum tests through this isolated helper.
- This removes nondeterministic failures caused by concurrent tests sharing the same persisted registry.

Primary file:
- `bitinfinity-btcrpc/src/main.rs`

Verification rerun:
- `cargo test -q -p bitinfinity-btcrpc`
- `cargo test -q -p bitinfinity-btcrpc` (second consecutive pass to confirm flake mitigation)

## Continuation (2026-02-21): E2E fail-fast port-collision diagnostics

Implemented:
- Hardened `scripts/e2e_testnet.sh` startup checks to fail fast on occupied listener ports with actionable diagnostics instead of crashing deeper in runtime startup.
- Added preflight validation for:
  - `NEAR_RPC_URL` port
  - `NEAR_NETWORK_PORT` (default `24567`)
  - `BTC_RPC_ADDR` port
  - `BTC_RPC_AUTH_ADDR` port
- Added port-number validation (`1..65535`) and explicit `lsof` listener dump on conflicts.
- Added `lsof` to required command checks.

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh` (initial run intentionally demonstrated fail-fast collision output on occupied port `3030`)
- `./scripts/e2e_testnet.sh` (clean rerun after removing stale process passed)
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): walletlock auth-gating coverage

Implemented:
- Extended auth-depth checks in `scripts/e2e_testnet.sh` to cover `walletlock` with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_walletlock_noauth_http_code`
  - `auth_walletlock_wrong_http_code`
  - `auth_walletlock_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh` (one initial preflight-fail on occupied port `3030`; clean rerun passed)
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): reliable E2E daemon cleanup (no stale listeners)

Implemented:
- Removed subshell-wrapped daemon launches in `scripts/e2e_testnet.sh` for:
  - `bitinfinity-neard`
  - `bitinfinity-btcrpc` (initial + restart)
  - auth-mode `bitinfinity-btcrpc`
- Daemons now launch directly in background so tracked PIDs correspond to actual long-lived processes, allowing `cleanup()` to terminate them reliably.
- This eliminates stale process leakage that previously left ports (`3030/24567/18332/18333`) occupied across runs.

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `lsof -iTCP:3030 -sTCP:LISTEN -n -P || true`
- `lsof -iTCP:24567 -sTCP:LISTEN -n -P || true`
- `lsof -iTCP:18332 -sTCP:LISTEN -n -P || true`
- `lsof -iTCP:18333 -sTCP:LISTEN -n -P || true`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): walletpassphrase auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include `walletpassphrase` with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_walletpassphrase_noauth_http_code`
  - `auth_walletpassphrase_wrong_http_code`
  - `auth_walletpassphrase_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): walletpassphrasechange auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include `walletpassphrasechange` with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Used a deliberately invalid old passphrase in the authenticated request to avoid mutating wallet encryption state while still exercising auth gating.
- Added summary exports:
  - `auth_walletpassphrasechange_noauth_http_code`
  - `auth_walletpassphrasechange_wrong_http_code`
  - `auth_walletpassphrasechange_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): encryptwallet auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include `encryptwallet` with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Used an empty-params payload for the authenticated probe to avoid mutating wallet encryption state while still verifying auth middleware behavior.
- Added summary exports:
  - `auth_encryptwallet_noauth_http_code`
  - `auth_encryptwallet_wrong_http_code`
  - `auth_encryptwallet_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): createwallet auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include `createwallet` with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_createwallet_noauth_http_code`
  - `auth_createwallet_wrong_http_code`
  - `auth_createwallet_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): loadwallet/unloadwallet auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include wallet management RPC methods:
  - `loadwallet`
  - `unloadwallet`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_loadwallet_noauth_http_code`
  - `auth_loadwallet_wrong_http_code`
  - `auth_loadwallet_ok_http_code`
  - `auth_unloadwallet_noauth_http_code`
  - `auth_unloadwallet_wrong_http_code`
  - `auth_unloadwallet_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): dumpprivkey auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include `dumpprivkey` (sensitive wallet export path) with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_dumpprivkey_noauth_http_code`
  - `auth_dumpprivkey_wrong_http_code`
  - `auth_dumpprivkey_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): importprivkey auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include `importprivkey` with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Used a deliberately invalid WIF payload for the authenticated probe to avoid mutating wallet key state while still verifying auth middleware enforcement.
- Added summary exports:
  - `auth_importprivkey_noauth_http_code`
  - `auth_importprivkey_wrong_http_code`
  - `auth_importprivkey_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): importaddress/backupwallet auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include wallet watch/backup management RPC methods:
  - `importaddress`
  - `backupwallet`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Used empty-params authenticated probes so these checks remain side-effect free while still validating auth middleware behavior.
- Added summary exports:
  - `auth_importaddress_noauth_http_code`
  - `auth_importaddress_wrong_http_code`
  - `auth_importaddress_ok_http_code`
  - `auth_backupwallet_noauth_http_code`
  - `auth_backupwallet_wrong_http_code`
  - `auth_backupwallet_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): settxfee/keypoolrefill auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include wallet settings RPC methods:
  - `settxfee`
  - `keypoolrefill`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_settxfee_noauth_http_code`
  - `auth_settxfee_wrong_http_code`
  - `auth_settxfee_ok_http_code`
  - `auth_keypoolrefill_noauth_http_code`
  - `auth_keypoolrefill_wrong_http_code`
  - `auth_keypoolrefill_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): signmessage auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include `signmessage` (wallet signing path) with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_signmessage_noauth_http_code`
  - `auth_signmessage_wrong_http_code`
  - `auth_signmessage_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): verifymessage auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include `verifymessage` (message-verification path) with explicit:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_verifymessage_noauth_http_code`
  - `auth_verifymessage_wrong_http_code`
  - `auth_verifymessage_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): getnewaddress/setlabel auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include wallet address/label management RPC methods:
  - `getnewaddress`
  - `setlabel`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getnewaddress_noauth_http_code`
  - `auth_getnewaddress_wrong_http_code`
  - `auth_getnewaddress_ok_http_code`
  - `auth_setlabel_noauth_http_code`
  - `auth_setlabel_wrong_http_code`
  - `auth_setlabel_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): getrawchangeaddress/listlabels auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include wallet address/label read paths:
  - `getrawchangeaddress`
  - `listlabels`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getrawchangeaddress_noauth_http_code`
  - `auth_getrawchangeaddress_wrong_http_code`
  - `auth_getrawchangeaddress_ok_http_code`
  - `auth_listlabels_noauth_http_code`
  - `auth_listlabels_wrong_http_code`
  - `auth_listlabels_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Issue #1 goal check

Status:
- Bitcoin addresses as account IDs: **Achieved** (canonical + backward-compatible legacy)
- No-claim first-transaction signature recovery flow: **Achieved** (already present; preserved)
- User keys secp256k1 / validator keys ed25519 split: **Achieved** (already present; unchanged)
- Patoshi reassignment tooling: **Achieved** (dedicated auto-generated Bitcoin keypair output + reassignment path)

## Issue #11 remaining high-priority gaps (not completed here)

Still open and required for full #11 closure:
- Optional upstream follow-up: investigate the self-exit (`controller: null`) shutdown path that previously produced `signal 11` in pre-mitigation pilots.
- External audit/bounty/legal/governance/infra phases.
- Full launch-gate completion across all #11 phases.

This change set advances Phase 0 hardening and removes a key blocker against Issue #1’s zero-friction address goal.
