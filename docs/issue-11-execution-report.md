# Issue #11 Execution Report (2026-02-20)

This report documents what was executed in this workspace toward:
- Issue #11: Launch plan hardening tasks
- Issue #1: Core architecture/UX goals

## Incremental Update (2026-02-21)

Additional logical commits were pushed on `jayzalowitz/btc-near-fork-plan` for RPC compatibility hardening and validation depth:

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
- `HOME="$(mktemp -d)" RUSTUP_HOME="/Users/jayzalowitz/.rustup" CARGO_HOME="/Users/jayzalowitz/.cargo" cargo test -q -p bitinfinity-btcrpc`

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

## Continuation (2026-02-21): label-query auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include wallet label-query methods:
  - `getaddressesbylabel`
  - `getreceivedbylabel`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getaddressesbylabel_noauth_http_code`
  - `auth_getaddressesbylabel_wrong_http_code`
  - `auth_getaddressesbylabel_ok_http_code`
  - `auth_getreceivedbylabel_noauth_http_code`
  - `auth_getreceivedbylabel_wrong_http_code`
  - `auth_getreceivedbylabel_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): getwalletinfo/listaddressgroupings auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include wallet introspection methods:
  - `getwalletinfo`
  - `listaddressgroupings`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getwalletinfo_noauth_http_code`
  - `auth_getwalletinfo_wrong_http_code`
  - `auth_getwalletinfo_ok_http_code`
  - `auth_listaddressgroupings_noauth_http_code`
  - `auth_listaddressgroupings_wrong_http_code`
  - `auth_listaddressgroupings_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): listreceivedbyaddress/listunspent auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include wallet receive/UTXO listing methods:
  - `listreceivedbyaddress`
  - `listunspent`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_listreceivedbyaddress_noauth_http_code`
  - `auth_listreceivedbyaddress_wrong_http_code`
  - `auth_listreceivedbyaddress_ok_http_code`
  - `auth_listunspent_noauth_http_code`
  - `auth_listunspent_wrong_http_code`
  - `auth_listunspent_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): getbalance/getbalances auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include wallet balance queries:
  - `getbalance`
  - `getbalances`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getbalance_noauth_http_code`
  - `auth_getbalance_wrong_http_code`
  - `auth_getbalance_ok_http_code`
  - `auth_getbalances_noauth_http_code`
  - `auth_getbalances_wrong_http_code`
  - `auth_getbalances_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): tx-query auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include transaction-query methods:
  - `gettransaction`
  - `getrawtransaction`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_gettransaction_noauth_http_code`
  - `auth_gettransaction_wrong_http_code`
  - `auth_gettransaction_ok_http_code`
  - `auth_getrawtransaction_noauth_http_code`
  - `auth_getrawtransaction_wrong_http_code`
  - `auth_getrawtransaction_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-21): listtransactions/listsinceblock auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include transaction listing methods:
  - `listtransactions`
  - `listsinceblock`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_listtransactions_noauth_http_code`
  - `auth_listtransactions_wrong_http_code`
  - `auth_listtransactions_ok_http_code`
  - `auth_listsinceblock_noauth_http_code`
  - `auth_listsinceblock_wrong_http_code`
  - `auth_listsinceblock_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): decodepsbt/analyzepsbt auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include PSBT inspection methods:
  - `decodepsbt`
  - `analyzepsbt`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_decodepsbt_noauth_http_code`
  - `auth_decodepsbt_wrong_http_code`
  - `auth_decodepsbt_ok_http_code`
  - `auth_analyzepsbt_noauth_http_code`
  - `auth_analyzepsbt_wrong_http_code`
  - `auth_analyzepsbt_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): finalizepsbt/utxoupdatepsbt auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include PSBT finalize/update methods:
  - `finalizepsbt`
  - `utxoupdatepsbt`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_finalizepsbt_noauth_http_code`
  - `auth_finalizepsbt_wrong_http_code`
  - `auth_finalizepsbt_ok_http_code`
  - `auth_utxoupdatepsbt_noauth_http_code`
  - `auth_utxoupdatepsbt_wrong_http_code`
  - `auth_utxoupdatepsbt_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): combinepsbt auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include PSBT combination method:
  - `combinepsbt`
- Added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_combinepsbt_noauth_http_code`
  - `auth_combinepsbt_wrong_http_code`
  - `auth_combinepsbt_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): getblockheader/getaddressinfo auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include Tier-1 wallet/read methods:
  - `getblockheader`
  - `getaddressinfo`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getblockheader_noauth_http_code`
  - `auth_getblockheader_wrong_http_code`
  - `auth_getblockheader_ok_http_code`
  - `auth_getaddressinfo_noauth_http_code`
  - `auth_getaddressinfo_wrong_http_code`
  - `auth_getaddressinfo_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): scantxoutset/createrawtransaction auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include remaining Tier-1 balance/build methods:
  - `scantxoutset`
  - `createrawtransaction`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_scantxoutset_noauth_http_code`
  - `auth_scantxoutset_wrong_http_code`
  - `auth_scantxoutset_ok_http_code`
  - `auth_createrawtransaction_noauth_http_code`
  - `auth_createrawtransaction_wrong_http_code`
  - `auth_createrawtransaction_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): getblock/getblockstats auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include Tier-3 block-read methods:
  - `getblock`
  - `getblockstats`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getblock_noauth_http_code`
  - `auth_getblock_wrong_http_code`
  - `auth_getblock_ok_http_code`
  - `auth_getblockstats_noauth_http_code`
  - `auth_getblockstats_wrong_http_code`
  - `auth_getblockstats_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): getchaintips/getrawmempool auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include additional Tier-3 mempool/chain-tip read methods:
  - `getchaintips`
  - `getrawmempool`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getchaintips_noauth_http_code`
  - `auth_getchaintips_wrong_http_code`
  - `auth_getchaintips_ok_http_code`
  - `auth_getrawmempool_noauth_http_code`
  - `auth_getrawmempool_wrong_http_code`
  - `auth_getrawmempool_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): mempool-entry auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include additional Tier-3 mempool entry methods:
  - `getmempoolentry`
  - `getmempoolancestors`
  - `getmempooldescendants`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getmempoolentry_noauth_http_code`
  - `auth_getmempoolentry_wrong_http_code`
  - `auth_getmempoolentry_ok_http_code`
  - `auth_getmempoolancestors_noauth_http_code`
  - `auth_getmempoolancestors_wrong_http_code`
  - `auth_getmempoolancestors_ok_http_code`
  - `auth_getmempooldescendants_noauth_http_code`
  - `auth_getmempooldescendants_wrong_http_code`
  - `auth_getmempooldescendants_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): block-hash/blockchaininfo auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include additional chain-read methods:
  - `getbestblockhash`
  - `getblockhash`
  - `getblockchaininfo`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getbestblockhash_noauth_http_code`
  - `auth_getbestblockhash_wrong_http_code`
  - `auth_getbestblockhash_ok_http_code`
  - `auth_getblockhash_noauth_http_code`
  - `auth_getblockhash_wrong_http_code`
  - `auth_getblockhash_ok_http_code`
  - `auth_getblockchaininfo_noauth_http_code`
  - `auth_getblockchaininfo_wrong_http_code`
  - `auth_getblockchaininfo_ok_http_code`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): mining/stub auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include PoS/mining-stub endpoints:
  - `getmininginfo`
  - `getblocktemplate`
  - `generate`
  - `generatetoaddress`
  - `generatetodescriptor`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_getmininginfo_*`
  - `auth_getblocktemplate_*`
  - `auth_generate_*`
  - `auth_generatetoaddress_*`
  - `auth_generatetodescriptor_*`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): p2p-stub auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include P2P stub methods:
  - `addnode`
  - `disconnectnode`
  - `onetry`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_addnode_*`
  - `auth_disconnectnode_*`
  - `auth_onetry_*`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): validateaddress/joinpsbts auth-gating coverage

Implemented:
- Extended auth-depth E2E checks to include remaining RPC compatibility methods:
  - `validateaddress`
  - `joinpsbts`
- For each method, added explicit auth triad assertions:
  - unauthenticated request returns HTTP `401`,
  - wrong-credential request returns HTTP `401`,
  - authenticated request returns HTTP `200` with matching JSON-RPC `id`.
- Added summary exports:
  - `auth_validateaddress_*`
  - `auth_joinpsbts_*`

Primary file:
- `scripts/e2e_testnet.sh`

Verification rerun:
- `bash -n scripts/e2e_testnet.sh`
- `./scripts/e2e_testnet.sh`
- `cargo test -q -p bitinfinity-btcrpc`

## Continuation (2026-02-22): CI auth-coverage drift guard

Implemented:
- Added executable guard script:
  - `scripts/check_auth_coverage.sh`
- Script behavior:
  - extracts all RPC `method` tokens in `scripts/e2e_testnet.sh`,
  - extracts `method` tokens inside the auth-verification block,
  - fails when any btcrpc method used in the E2E flow is missing from auth coverage (with `query` explicitly ignored as NEAR JSON-RPC).
- Wired guard into CI test job:
  - new step `Verify E2E auth coverage matrix` in `.github/workflows/ci.yml`.

Primary files:
- `scripts/check_auth_coverage.sh`
- `.github/workflows/ci.yml`

Verification rerun:
- `bash -n scripts/check_auth_coverage.sh`
- `./scripts/check_auth_coverage.sh`
- `ruby -e 'require \"yaml\"; YAML.load_file(\".github/workflows/ci.yml\")'`

## Continuation (2026-02-22): controller-null benchmark startup fallback

Implemented:
- Hardened controller-disabled benchmark startup detection in:
  - `scripts/benchmark/run_tps_profiles.sh`
- Runtime-start detection now uses:
  - `started schedule=` (controller-enabled path),
  - `ready to produce block, has enough approvals` (controller-disabled info-log path),
  - `RUST_LOG=.*--home .* run` launch marker (controller-disabled warn-log path).
- This prevents `--disable-controller --loglevel warn` runs from waiting until `--startup-timeout` when info-level readiness lines are suppressed.
- Added explicit controller-mode visibility in benchmark outputs:
  - per-profile `summary.json` now includes `controller_enabled` (boolean),
  - aggregate `summary.csv` and `summary.md` now include a `controller_enabled` column.

Primary file:
- `scripts/benchmark/run_tps_profiles.sh`

Verification reruns:
- `bash -n scripts/benchmark/run_tps_profiles.sh`
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 60 --duration-override 4 --run-grace 8 --startup-timeout 15 --num-accounts 20 --metrics-interval 1 --skip-build --disable-controller --allow-nonzero-run-status --loglevel warn --out-dir artifacts/benchmarks/controller-null-warn-fallback-20260222T075409Z`
  - produced `artifacts/benchmarks/controller-null-warn-fallback-20260222T075409Z/summary.json`
  - observed `controller_enabled=false`, `signal_11_from_log=0`

## Continuation (2026-02-22): benchmark timeout-phase and start-marker diagnostics

Implemented:
- Updated benchmark startup marker compatibility:
  - startup detection now recognizes both legacy `started schedule=` and current `starting the static load schedule`.
- Added explicit timeout and startup-detection diagnostics to per-profile summaries:
  - `timeout_phase` (`startup`, `runtime`, or `unknown` when timed out),
  - `schedule_started_from_log` (`0/1`).
- Extended aggregate outputs:
  - `summary.csv` now includes `timed_out`, `timeout_phase`, and `schedule_started_from_log`,
  - `summary.md` profile table now includes matching columns.

Primary file:
- `scripts/benchmark/run_tps_profiles.sh`

Verification reruns:
- `bash -n scripts/benchmark/run_tps_profiles.sh`
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 60 --duration-override 4 --run-grace 8 --startup-timeout 15 --num-accounts 20 --metrics-interval 1 --skip-build --disable-controller --allow-nonzero-run-status --loglevel warn --out-dir artifacts/benchmarks/controller-null-timeout-diagnostics-20260222T075716Z`
  - produced `artifacts/benchmarks/controller-null-timeout-diagnostics-20260222T075716Z/summary.json`
  - observed `timed_out=1`, `timeout_phase=\"runtime\"`, `schedule_started_from_log=1`, `signal_11_from_log=0`

## Continuation (2026-02-22): skip-build tx-generator binary preflight guard

Implemented:
- Added non-dry-run preflight validation for `--skip-build` mode in:
  - `scripts/benchmark/run_tps_profiles.sh`
- Guard behavior:
  - verifies `${TX_GEN_DIR}/neard` exists and is executable,
  - verifies the binary contains tx-generator benchmark markers,
  - fails fast with actionable guidance when marker checks fail.
- This prevents long benchmark runs against binaries that likely were not built with `--features tx_generator`.

Primary file:
- `scripts/benchmark/run_tps_profiles.sh`

Verification reruns:
- `bash -n scripts/benchmark/run_tps_profiles.sh`
- `./scripts/benchmark/run_tps_profiles.sh --dry-run --skip-build --profile baseline --metrics-interval 1` (passes dry-run path)
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 60 --duration-override 4 --run-grace 8 --startup-timeout 15 --num-accounts 20 --metrics-interval 1 --skip-build --disable-controller --allow-nonzero-run-status --loglevel warn --out-dir artifacts/benchmarks/skip-build-preflight-guard-20260222T080020Z` (expected preflight failure)
  - observed error:
    - `does not appear to include tx_generator benchmark markers`
    - guidance to rerun without `--skip-build`

## Continuation (2026-02-22): full rebuild tx-generator benchmark validation

Validated:
- Executed benchmark runner without `--skip-build` to force fresh release build with tx-generator support:
  - `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 60 --duration-override 4 --run-grace 8 --startup-timeout 60 --num-accounts 20 --metrics-interval 1 --allow-nonzero-run-status --out-dir artifacts/benchmarks/post-build-txgen-verify-20260222T080207Z`
- Build result:
  - `Finished release profile [optimized]` for `neard` with `tx_generator` feature.
- Runtime evidence in `neard.log`:
  - `starting the static load schedule`
  - `completed running the schedule`
  - transaction-generator `diff=StatsLocal ... rate=...` lines present.
- Summary evidence:
  - `artifacts/benchmarks/post-build-txgen-verify-20260222T080207Z/baseline/summary.json`
  - observed:
    - `controller_enabled=true`
    - `schedule_started_from_log=1`
    - `schedule_completed_from_log=1`
    - `timed_out=0`
    - `effective_run_status=0`
    - `signal_11_from_log=0`
    - non-null throughput fields (`avg_tps_from_log`, `peak_tps_from_log`).

## Continuation (2026-02-22): run-localnet phase gating for benchmark timing

Implemented:
- Hardened benchmark timing loop in `scripts/benchmark/run_tps_profiles.sh` to prevent setup-phase false positives:
  - introduced `run_localnet_started` detection based on `RUST_LOG=.*--home .* run` launch line,
  - schedule-start detection now runs only after run-localnet launch is observed,
  - controller-disabled path now treats run-localnet launch as runtime start fallback.
- Added per-profile diagnostics:
  - `run_localnet_started_from_log` (`0/1`) in `summary.json`,
  - corresponding `run_localnet_started_from_log` column in `summary.csv`,
  - matching `run launched` column in `summary.md`.
- Eliminated transient startup noise by guarding early log greps behind `[[ -f "$log_file" ]]`.

Primary file:
- `scripts/benchmark/run_tps_profiles.sh`

Verification reruns:
- `bash -n scripts/benchmark/run_tps_profiles.sh`
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 60 --duration-override 4 --run-grace 8 --startup-timeout 15 --num-accounts 20 --metrics-interval 1 --skip-build --disable-controller --allow-nonzero-run-status --loglevel warn --out-dir artifacts/benchmarks/phase-gate-controller-null-20260222T081610Z`
  - observed in `artifacts/benchmarks/phase-gate-controller-null-20260222T081610Z/baseline/summary.json`:
    - `run_localnet_started_from_log=1`
    - `schedule_started_from_log=1`
    - `timed_out=0`
    - `run_status=0`
    - `signal_11_from_log=0`
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 60 --duration-override 4 --run-grace 8 --startup-timeout 30 --num-accounts 20 --metrics-interval 1 --skip-build --allow-nonzero-run-status --out-dir artifacts/benchmarks/phase-gate-controller-enabled-20260222T081850Z`
  - observed in `artifacts/benchmarks/phase-gate-controller-enabled-20260222T081850Z/baseline/summary.json`:
    - `run_localnet_started_from_log=1`
    - `schedule_started_from_log=1`
    - `schedule_completed_from_log=1`
    - `effective_run_status=0`
    - `signal_11_from_log=0`
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 40 --duration-override 3 --run-grace 5 --startup-timeout 15 --num-accounts 10 --metrics-interval 1 --skip-build --disable-controller --allow-nonzero-run-status --loglevel warn --out-dir artifacts/benchmarks/logfile-guard-smoke-20260222T082027Z`
  - confirmed no transient `grep ... No such file or directory` log noise during startup.

## Continuation (2026-02-22): benchmark metric baseline subtraction

Implemented:
- Hardened benchmark metric accounting in `scripts/benchmark/run_tps_profiles.sh`:
  - captures pre-run metric baselines at run-localnet launch,
  - computes benchmark-only deltas for:
    - `final_success_metric`
    - `final_failed_metric`
  - preserves raw counters for auditability in per-profile summary:
    - `pre_run_success_metric_baseline`
    - `pre_run_failed_metric_baseline`
    - `final_success_metric_raw`
    - `final_failed_metric_raw`.
- This removes `create-accounts` transaction pollution from benchmark final metric outputs.

Primary file:
- `scripts/benchmark/run_tps_profiles.sh`

Verification reruns:
- `bash -n scripts/benchmark/run_tps_profiles.sh`
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 60 --duration-override 4 --run-grace 8 --startup-timeout 30 --num-accounts 20 --metrics-interval 1 --skip-build --allow-nonzero-run-status --out-dir artifacts/benchmarks/metric-delta-enabled-20260222T082130Z`
  - observed in `artifacts/benchmarks/metric-delta-enabled-20260222T082130Z/baseline/summary.json`:
    - `pre_run_success_metric_baseline=20`
    - `final_success_metric_raw=192`
    - `final_success_metric=172` (baseline-adjusted)
    - `final_failed_metric=0`
    - `effective_run_status=0`, `signal_11_from_log=0`.
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 40 --duration-override 3 --run-grace 5 --startup-timeout 15 --num-accounts 10 --metrics-interval 1 --skip-build --disable-controller --allow-nonzero-run-status --loglevel warn --out-dir artifacts/benchmarks/metric-delta-null-20260222T082259Z`
  - observed in `artifacts/benchmarks/metric-delta-null-20260222T082259Z/baseline/summary.json`:
    - `pre_run_success_metric_baseline=10`
    - `final_success_metric_raw=80`
    - `final_success_metric=70` (baseline-adjusted)
    - `final_failed_metric=0`
    - `run_status=0`, `timed_out=0`.

## Continuation (2026-02-22): suppress bootstrap tx-generator idle log noise

Implemented:
- Updated `scripts/benchmark/run_tps_profiles.sh` account-creation bootstrap path to run with `RUST_LOG="info,transaction-generator=off"` in both dry-run command output and live execution.
- This suppresses expected tx-generator idle/no-schedule noise emitted during `create-accounts` before the benchmark schedule begins, while preserving normal runtime tx-generator telemetry.

Primary file:
- `scripts/benchmark/run_tps_profiles.sh`

Verification reruns:
- `bash -n scripts/benchmark/run_tps_profiles.sh`
- `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 40 --duration-override 3 --run-grace 5 --startup-timeout 30 --num-accounts 10 --metrics-interval 1 --skip-build --allow-nonzero-run-status --out-dir artifacts/benchmarks/bootstrap-log-cleanup-20260222T082433Z`
  - observed in `artifacts/benchmarks/bootstrap-log-cleanup-20260222T082433Z/baseline/summary.json`:
    - `run_status=143`, `effective_run_status=0`, `timed_out=0`
    - `run_localnet_started_from_log=1`, `schedule_started_from_log=1`, `schedule_completed_from_log=1`
    - `pre_run_success_metric_baseline=10`, `final_success_metric_raw=144`, `final_success_metric=134`, `final_failed_metric=0`
  - observed in `artifacts/benchmarks/bootstrap-log-cleanup-20260222T082433Z/baseline/neard.log`:
    - no matches for `tx generator idle` / `no schedule provided` during bootstrap.

## Continuation (2026-02-22): mempool ancestor/descendant RPC graph completion

Implemented:
- Replaced placeholder responses for `getmempoolancestors` and `getmempooldescendants` in `bitinfinity-btcrpc/src/main.rs`.
- Added pending-mempool graph construction from cached raw transaction inputs:
  - parses input `txid` references from pending raw tx hex,
  - computes transitive ancestor/descendant relationships among pending transactions.
- Added Bitcoin Core-compatible behavior improvements:
  - required `txid` parameter validation,
  - `-5` error when requested tx is not in pending mempool,
  - optional `verbose=true` object response with per-entry mempool metadata and dependency fields.

Primary file:
- `bitinfinity-btcrpc/src/main.rs`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc test_getmempool_relations -- --nocapture`
  - `test_getmempool_relations_track_transitive_pending_graph` passed:
    - validates transitive ancestors (`grandchild -> child -> parent`) and descendants (`parent -> child -> grandchild`) resolution.
    - validates verbose descendants object includes both direct and transitive children.
  - `test_getmempool_relations_reject_non_pending_txid` passed:
    - validates confirmed/non-pending txids return `-5` for both relation methods.

## Continuation (2026-02-22): getmempoolentry pending-only enforcement

Implemented:
- Hardened `getmempoolentry` in `bitinfinity-btcrpc/src/main.rs` to align with mempool semantics:
  - now returns `-5` when the requested txid exists in cache but is not pending (`near_tx_hash` is not `pending:*`),
  - retains successful mempool-entry output for pending transactions.
- This removes a cache-vs-mempool mismatch where previously confirmed entries could be surfaced as mempool entries.

Primary file:
- `bitinfinity-btcrpc/src/main.rs`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc test_getmempool -- --nocapture`
  - `test_getmempoolentry_requires_pending_entry` passed:
    - confirms confirmed/non-pending txids return `-5` from `getmempoolentry`.
  - `test_getmempoolentry_accepts_pending_entry` passed:
    - confirms pending txids return normal mempool-entry fields.
  - `test_getmempool_relations_track_transitive_pending_graph` passed.
  - `test_getmempool_relations_reject_non_pending_txid` passed.

## Continuation (2026-02-22): walletprocesspsbt unlock-state enforcement

Implemented:
- Hardened `walletprocesspsbt` in `bitinfinity-btcrpc/src/main.rs` to enforce wallet lock state before signing:
  - returns Bitcoin Core-compatible `-13` when wallet is locked,
  - continues normal PSBT processing/signing path when wallet is unlocked.
- Added focused unit coverage for both locked and unlocked paths.

Primary file:
- `bitinfinity-btcrpc/src/main.rs`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc test_walletprocesspsbt -- --nocapture`
  - `test_walletprocesspsbt_requires_unlocked_wallet` passed:
    - confirms locked wallet returns `-13`.
  - `test_walletprocesspsbt_allows_unlocked_wallet` passed:
    - confirms unlocked wallet returns PSBT payload with completion status.
- `cargo test -p bitinfinity-btcrpc test_getmempool -- --nocapture`
  - confirms mempool relation/entry coverage remains green after the walletprocesspsbt change.

## Continuation (2026-02-22): fuzz CI soak cadence expansion

Implemented:
- Hardened `.github/workflows/nightly-fuzz.yml` toward Phase 0.2 runtime expectations:
  - schedule increased from once daily to every 6 hours (`0 */6 * * *`),
  - per-target default fuzz duration increased from `1800s` (30m) to `21600s` (6h),
  - job timeout increased from `90` to `360` minutes to match the new soak duration,
  - workflow-level concurrency guard added (`nightly-fuzz-${{ github.ref }}`) to avoid overlapping soak runs.
- Resulting coverage intent: ~24 hours cumulative fuzz runtime per target per day (4 runs/day * 6h).

Primary file:
- `.github/workflows/nightly-fuzz.yml`

Verification reruns:
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/nightly-fuzz.yml")'`
  - confirms workflow YAML remains valid after schedule/duration updates.

## Continuation (2026-02-22): controller-null benchmark stability rerun sweep

Validated:
- Executed a 3-run controller-disabled stability sweep in warn-mode to verify no recurring startup/runtime crash behavior:
  - `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 40 --duration-override 3 --run-grace 5 --startup-timeout 15 --num-accounts 10 --metrics-interval 1 --skip-build --disable-controller --allow-nonzero-run-status --loglevel warn --out-dir artifacts/benchmarks/controller-null-stability-20260222T083947Z-r1`
  - `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 40 --duration-override 3 --run-grace 5 --startup-timeout 15 --num-accounts 10 --metrics-interval 1 --skip-build --disable-controller --allow-nonzero-run-status --loglevel warn --out-dir artifacts/benchmarks/controller-null-stability-20260222T083947Z-r2`
  - `./scripts/benchmark/run_tps_profiles.sh --profile baseline --tps-override 40 --duration-override 3 --run-grace 5 --startup-timeout 15 --num-accounts 10 --metrics-interval 1 --skip-build --disable-controller --allow-nonzero-run-status --loglevel warn --out-dir artifacts/benchmarks/controller-null-stability-20260222T083947Z-r3`
- Observed in each run's per-profile summary:
  - `run_status=0`, `effective_run_status=0`, `timed_out=0`
  - `run_localnet_started_from_log=1`, `schedule_started_from_log=1`
  - `schedule_completed_from_log=0` (expected with controller disabled)
  - `signal_11_from_log=0`
  - `final_success_metric=70`, `final_failed_metric=0`
- Log scans across all three runs reported no matches for `tx generator idle`, `no schedule provided`, `signal 11`, or `SIGSEGV`.

## Continuation (2026-02-22): RPC compatibility matrix documentation

Implemented:
- Added dedicated RPC compatibility reference document:
  - `docs/rpc-compatibility-matrix.md`
  - covers Tier 1/Tier 2/Tier 3 methods from Issue #11 with status labels (`Core-like`, `Adapted`, `Intentional stub`).
  - explicitly documents intentional PoS/networking stubs and current adapter caveats.
- Linked the new matrix from `docs/index.html` in the throughput/methodology section for discoverability.

Primary files:
- `docs/rpc-compatibility-matrix.md`
- `docs/index.html`

Verification reruns:
- `rg -n "rpc-compatibility-matrix.md|Tier 1|Intentional PoS" docs/index.html docs/rpc-compatibility-matrix.md -S`
  - confirms matrix content headings and site link are present.

## Continuation (2026-02-22): full btcrpc regression pass after mempool/PSBT hardening

Validated:
- Executed full crate test suite:
  - `cargo test -p bitinfinity-btcrpc -- --nocapture`
- Result:
  - `53 passed`, `0 failed`, `0 ignored`.
- This includes newly added mempool relationship tests, mempoolentry pending-only tests, and walletprocesspsbt lock-state tests alongside existing PSBT/quantum/signature coverage.

## Continuation (2026-02-22): Issue #1 core-goal verification test sweep

Validated:
- Executed targeted Issue #1-aligned crate suites:
  - `cargo test --manifest-path near-account-id/Cargo.toml`
    - result: `10 passed`, `0 failed`.
    - includes Bitcoin account-ID acceptance and BTC implicit-account detection tests.
  - `cargo test -p bitinfinity-tools`
    - result: `21 passed`, `0 failed`, `1 ignored`.
    - includes Patoshi reassignment, signature recovery/address derivation, and genesis-builder registry tests.

## Issue #1 goal check

Status:
- Bitcoin addresses as account IDs: **Achieved** (canonical + backward-compatible legacy)
- No-claim first-transaction signature recovery flow: **Achieved** (already present; preserved)
- User keys secp256k1 / validator keys ed25519 split: **Achieved** (already present; unchanged)
- Patoshi reassignment tooling: **Achieved** (dedicated auto-generated Bitcoin keypair output + reassignment path)

## Continuation (2026-02-22): expanded unloaded-wallet guard coverage across wallet RPC surface

Implemented:
- Extended `-18` loaded-wallet enforcement beyond the initial lifecycle set to additional wallet-scoped handlers, including:
  - balance/transaction surfaces (`getbalance`, `gettransaction`, `listtransactions`, `listsinceblock`, `getunconfirmedbalance`);
  - UTXO/funding/coin-control flows (`listunspent`, `fundrawtransaction`, `walletcreatefundedpsbt`, `lockunspent`, `listlockunspent`);
  - wallet maintenance/import flows (`keypoolrefill`, `backupwallet`, `importaddress`, `importpubkey`, `setlabel`, `walletpassphrasechange`, `encryptwallet`, `listreceivedbylabel`, `getreceivedbylabel`, `listlabels`);
  - address/balance views (`getaddressinfo`, `getbalances`, `listreceivedbyaddress`, `listaddressgroupings`, `getaddressesbylabel`, `getaccount`).
- Added regression coverage:
  - `test_extended_wallet_methods_reject_unloaded_wallet`
    - verifies representative wallet methods now consistently return `-18` after `unloadwallet`.

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `56 passed`, `0 failed`.
  - confirms newly added unloaded-wallet guard test alongside all prior mempool/PSBT/lifecycle coverage.

## Continuation (2026-02-22): secp256k1 recovery panic hardening + dedicated crypto fuzz target

Implemented:
- Hardened `nearcore/core/crypto/src/signature.rs` `Secp256K1Signature::recover` to remove panic paths:
  - replaced `RecoveryId::from_i32(...).unwrap()` with error-mapped handling;
  - replaced `Message::from_slice(...).unwrap()` with explicit error-mapped handling.
- Added regression unit test:
  - `test_secp256k1_recover_rejects_invalid_recovery_id`
  - validates invalid recovery IDs return an error instead of panicking.
- Added dedicated fuzz harness for crypto recovery paths:
  - `nearcore/core/crypto/fuzz/Cargo.toml`
  - `nearcore/core/crypto/fuzz/fuzz_targets/fuzz_secp256k1_recover.rs`
  - exercises `Secp256K1Signature::recover`, signature value checks, and secp256k1 `Signature::from_parts`/verify pathways under arbitrary inputs.
- Wired new fuzz target into CI pipelines:
  - `.github/workflows/ci.yml` adds 30s smoke run;
  - `.github/workflows/nightly-fuzz.yml` matrix now includes `nearcore/core/crypto` `fuzz_secp256k1_recover`.

Verification reruns:
- `cargo test --manifest-path nearcore/Cargo.toml -p near-crypto test_secp256k1_recover_rejects_invalid_recovery_id -- --nocapture`
  - result: test passed.
- `cargo check --manifest-path nearcore/core/crypto/fuzz/Cargo.toml`
  - result: fuzz crate/target compiles successfully.
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); YAML.load_file(".github/workflows/nightly-fuzz.yml")'`
  - result: workflow YAML remains valid.

## Continuation (2026-02-22): Patoshi CSV parsing fuzz coverage expansion

Implemented:
- Refactored Patoshi CSV loading to support reader-based parsing:
  - added `load_patoshi_addresses_from_reader` in `bitinfinity-tools/src/patoshi.rs`;
  - kept `load_patoshi_addresses(Path)` as thin file-wrapper over reader parser.
- Added parser correctness test:
  - `test_load_patoshi_addresses_from_reader_trims_and_deduplicates`.
- Added dedicated fuzz target for Patoshi ingestion/reassignment edge cases:
  - `bitinfinity-tools/fuzz/Cargo.toml`
  - `bitinfinity-tools/fuzz/fuzz_targets/fuzz_patoshi_csv.rs`
  - fuzzes arbitrary CSV bytes through Patoshi parsing + `reassign_patoshi` map updates.
- Wired the new Patoshi fuzz target into CI pipelines:
  - `.github/workflows/ci.yml` adds 30s smoke run;
  - `.github/workflows/nightly-fuzz.yml` matrix now includes `bitinfinity-tools` `fuzz_patoshi_csv`.

Verification reruns:
- `cargo test -p bitinfinity-tools`
  - result: `22 passed`, `0 failed`, `1 ignored`.
  - includes the new reader-based Patoshi parser unit test.
- `cargo check --manifest-path bitinfinity-tools/fuzz/Cargo.toml`
  - result: fuzz crate/target compiles successfully.
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); YAML.load_file(".github/workflows/nightly-fuzz.yml")'`
  - result: workflow YAML remains valid after matrix expansion.

## Continuation (2026-02-22): balance-arithmetic guard hardening for send RPC paths

Implemented:
- Added BTC→satoshi conversion guard helper in btcrpc:
  - `btc_to_satoshis_checked(amount_btc: f64) -> Option<u64>`
  - rejects non-finite values, non-positive amounts, overflow, and sub-satoshi precision.
- Hardened `sendtoaddress` arithmetic:
  - now uses checked conversion instead of lossy `as u64` casts;
  - rejects subtract-fee underflow (`-3`) instead of risking invalid zero/underflowed sends.
- Hardened `sendmany` arithmetic:
  - now rejects invalid/sub-satoshi per-recipient amounts (`-3`) instead of silently skipping/truncating.
- Added regression tests:
  - `test_btc_to_satoshis_checked_validation`
  - `test_sendtoaddress_rejects_too_small_amount_after_subtract_fee`
  - `test_sendmany_rejects_sub_satoshi_amount`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `59 passed`, `0 failed`.
  - includes all new arithmetic guard tests plus prior wallet/mempool/PSBT/quantum coverage.

## Continuation (2026-02-22): dedicated fuzz target for BTC amount-math guard paths

Implemented:
- Extracted amount conversion guard into reusable module:
  - `bitinfinity-btcrpc/src/amounts.rs`
  - hosts `btc_to_satoshis_checked` for shared runtime + fuzz coverage.
- Added btcrpc fuzz target:
  - `bitinfinity-btcrpc/fuzz/fuzz_targets/fuzz_amount_math.rs`
  - feeds arbitrary IEEE-754 values into BTC→satoshi conversion and checked arithmetic branches.
- Registered target in fuzz tooling:
  - `bitinfinity-btcrpc/fuzz/Cargo.toml` includes `fuzz_amount_math`.
  - `.github/workflows/ci.yml` adds 30s smoke run for `fuzz_amount_math`.
  - `.github/workflows/nightly-fuzz.yml` matrix now includes `bitinfinity-btcrpc` `fuzz_amount_math`.

Verification reruns:
- `cargo check --manifest-path bitinfinity-btcrpc/fuzz/Cargo.toml`
  - result: fuzz crate with new amount target compiles.
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `59 passed`, `0 failed`.
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); YAML.load_file(".github/workflows/nightly-fuzz.yml")'`
  - result: workflow YAML remains valid.

## Continuation (2026-02-22): RPC error-code reference documentation

Implemented:
- Added dedicated RPC error-code reference:
  - `docs/rpc-error-codes.md`
  - documents emitted JSON-RPC codes currently used by `bitinfinity-btcrpc`, grouped by common protocol vs wallet/transaction semantics.
- Linked reference from the main docs landing page:
  - `docs/index.html` now links benchmark methodology, RPC compatibility matrix, and RPC error code reference together.

Verification reruns:
- `rg -n "rpc-error-codes.md|RPC error code reference|-32700|-32602|-18|-3" docs/index.html docs/rpc-error-codes.md -S`
  - confirms navigation link + core code table entries are present.

## Continuation (2026-02-22): nonce-floor normalization consistency across send/sign flows

Implemented:
- Added reusable nonce normalization helper in btcrpc state:
  - `RpcState::normalize_nonce_for_first_bitcoin_tx(nonce, latest_block_height)`.
- Applied normalization consistently to first-transaction-sensitive paths:
  - `sendrawtransaction`
  - `sendtoaddress`
  - `sendmany`
  - `signrawtransactionwithwallet` (base nonce for per-output signing)
  - shared `get_block_and_nonce` helper (used by NEAR-native tx builders).
- This removes inconsistent behavior where some flows used raw `next_nonce` while others enforced the Bitcoin first-tx nonce floor.
- Added regression coverage:
  - `test_nonce_floor_normalization_for_first_bitcoin_tx`.

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `60 passed`, `0 failed`.
  - includes new nonce normalization test plus all prior wallet/mempool/PSBT/fuzz-hardening coverage.

## Continuation (2026-02-22): NEAR convenience RPC amount-validation hardening

Implemented:
- Replaced lossy BTC→satoshi casts with checked conversion in NEAR-native convenience RPC methods:
  - `stakenearsatoshis`
  - `createnearaccount` (initial transfer amount validation)
  - `fundgaskey`
  - `withdrawgaskey`
- Added strict validation behavior aligned with hardened send paths:
  - rejects non-finite/negative/sub-satoshi/invalid amounts with deterministic `-3` errors instead of silent truncation.
- Added regression tests:
  - `test_stake_rejects_sub_satoshi_amount`
  - `test_createnearaccount_rejects_negative_initial_balance`
  - `test_fundgaskey_rejects_sub_satoshi_amount`
  - `test_withdrawgaskey_rejects_sub_satoshi_amount`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `64 passed`, `0 failed`.
  - includes new NEAR convenience amount-validation tests plus prior nonce-floor/sendmany/sendtoaddress hardening coverage.

## Continuation (2026-02-22): raw/PSBT intent amount parsing hardening

Implemented:
- Replaced additional lossy float-to-`u64` conversions in Bitcoin transaction intent/PSBT flows with checked satoshi conversion:
  - `createrawtransaction` output parsing now enforces satoshi precision before tx-intent encoding.
  - `signrawtransactionwithwallet` intent decoding rejects invalid/sub-satoshi output amounts.
  - `fundrawtransaction` intent decoding rejects invalid/sub-satoshi amounts and total-amount overflow.
  - `parse_psbt_output_pairs` now returns satoshi-denominated outputs using checked conversion.
  - `createpsbt` and `walletcreatefundedpsbt` consume validated satoshi outputs directly.
- Added regression tests:
  - `test_createpsbt_rejects_sub_satoshi_output_amount`
  - `test_createrawtransaction_rejects_sub_satoshi_output_amount`
  - `test_fundrawtransaction_rejects_sub_satoshi_intent_output`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `67 passed`, `0 failed`.
  - includes new raw/PSBT/funding sub-satoshi rejection coverage alongside all prior hardening tests.

## Continuation (2026-02-22): NEAR action amount-conversion cleanup for advanced RPC paths

Implemented:
- Added shared non-negative BTC conversion helper:
  - `btc_to_satoshis_non_negative_checked(amount_btc: f64) -> Option<u64>`
  - allows exact zero; rejects negative/non-finite/sub-satoshi/overflow values.
- Removed remaining lossy casts in NEAR advanced action handling:
  - `addnearkey` function-call allowance parsing now uses checked conversion.
  - `sendneartx` action parsing now validates/converts amounts for:
    - `transfer`
    - `function_call` (`deposit_btc`)
    - `stake`
    - `add_function_call_key` (`allowance_btc`)
    - `transfer_to_gas_key`
    - `withdraw_from_gas_key`
  - invalid amounts now return deterministic `-3` errors instead of truncating via cast.
- Added regression coverage for the new helper:
  - `test_btc_to_satoshis_non_negative_checked_validation`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `68 passed`, `0 failed`.
  - includes new non-negative conversion helper assertions plus prior send/raw/PSBT/NEAR hardening tests.

## Continuation (2026-02-22): sendneartx fail-fast action validation ordering

Implemented:
- Refactored `sendneartx` action handling into a two-phase flow:
  - phase 1: parse and validate action payloads (including amount checks) into a validated action list;
  - phase 2: load sender key / fetch nonce+block hash / build and submit transaction.
- Added internal validated-action representation so `stake` can be finalized with sender pubkey after key lookup while all malformed amount/action payloads are rejected earlier.
- Behavioral improvement:
  - invalid/sub-satoshi action payloads now fail deterministically before wallet-key lookup and NEAR RPC calls.
- Added regression test:
  - `test_sendneartx_rejects_sub_satoshi_transfer_before_key_lookup`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `69 passed`, `0 failed`.
  - includes the new sendneartx fail-fast validation-ordering assertion plus prior hardening coverage.

## Continuation (2026-02-22): addnearkey fail-fast allowance validation ordering

Implemented:
- Reordered `addnearkey` function-call key path so allowance parsing/validation happens before sender key lookup and block/nonce RPC calls.
- This keeps behavior consistent with other hardened flows: malformed allowance amounts now fail deterministically without requiring wallet key presence or network availability.
- Added regression test:
  - `test_addnearkey_rejects_negative_allowance_before_key_lookup`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `70 passed`, `0 failed`.
  - includes both sendneartx/addnearkey fail-fast ordering assertions plus prior amount-math hardening coverage.

## Continuation (2026-02-22): mempool fee satoshi conversion cleanup

Implemented:
- Removed the final direct BTC float-to-satoshi cast in btcrpc (`getmempoolentry`).
- `getmempoolentry` fee now derives satoshis from `fee_yocto / YOCTO_PER_SATOSHI` with:
  - explicit `u64` bounds handling;
  - minimum fee floor alignment (`1000 sats`, matching `0.00001 BTC` fallback floor).
- This completes the in-file elimination of lossy `* 100_000_000.0 as u64` amount conversions in `bitinfinity-btcrpc/src/main.rs`.

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `70 passed`, `0 failed`.
  - confirms no regression after fee conversion cleanup.

## Continuation (2026-02-22): expanded sendneartx fail-fast regression coverage

Implemented:
- Added additional fail-fast tests to lock amount validation ordering for advanced `sendneartx` action types:
  - `test_sendneartx_rejects_negative_function_call_deposit_before_key_lookup`
  - `test_sendneartx_rejects_negative_function_call_key_allowance_before_key_lookup`
- These complement existing transfer and addnearkey fail-fast tests and ensure malformed deposits/allowances are rejected before wallet-key/network lookups.

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `72 passed`, `0 failed`.
  - confirms expanded fail-fast coverage without regressions.

## Continuation (2026-02-22): Issue #1 core-goal verification rerun after btcrpc hardening

Validated:
- Re-ran Issue #1-aligned suites to ensure core goals remained intact after recent btcrpc hardening:
  - `cargo test --manifest-path near-account-id/Cargo.toml -- --nocapture`
    - result: `10 passed`, `0 failed`.
    - confirms Bitcoin account-ID acceptance and BTC implicit account detection coverage remains green.
  - `cargo test -p bitinfinity-tools -- --nocapture`
    - result: `22 passed`, `0 failed`, `1 ignored`.
    - confirms Patoshi reassignment, signature recovery/address derivation, and genesis registry paths remain green.

## Continuation (2026-02-22): checked satoshi-sum and nonce-overflow hardening in signing/funding flows

Implemented:
- Added shared arithmetic helper:
  - `checked_sum_satoshis(...) -> Option<u64>` in `bitinfinity-btcrpc/src/amounts.rs`.
- Replaced remaining unchecked satoshi total sums in high-impact transaction paths:
  - `sendrawtransaction` payment-output total.
  - `signrawtransactionwithwallet` intent-output totals.
  - `walletcreatefundedpsbt` output total.
- Added explicit overflow error handling:
  - overflowed totals now return deterministic errors (`-22` or `-32602` depending on context) instead of relying on unchecked `sum()`.
- Hardened per-output nonce derivation in `signrawtransactionwithwallet`:
  - replaced `base_nonce + i as u64` with checked addition and explicit error on overflow.
- Added regression coverage:
  - `test_checked_sum_satoshis_overflow_protection`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `73 passed`, `0 failed`.
  - includes the new checked-sum overflow test plus all prior amount-validation/fail-fast hardening coverage.

## Continuation (2026-02-22): received-amount accumulator overflow hardening

Implemented:
- Hardened `getreceivedbyaddress` and `getreceivedbylabel` satoshi aggregation loops:
  - replaced unchecked `+=` accumulation with checked addition;
  - return deterministic `-32000` errors on overflow instead of potential wrap/panic paths.
- Added regression tests:
  - `test_getreceivedbyaddress_rejects_accumulator_overflow`
  - `test_getreceivedbylabel_rejects_accumulator_overflow`

Verification reruns:
- `cargo test -p bitinfinity-btcrpc -- --nocapture`
  - result: `75 passed`, `0 failed`.
  - includes new received-amount overflow regression coverage plus all prior hardening tests.

## Issue #11 remaining high-priority gaps (not completed here)

Still open and required for full #11 closure:
- External audit/bounty/legal/governance/infra phases.
- Full launch-gate completion across all #11 phases.

This change set advances Phase 0 hardening and removes a key blocker against Issue #1’s zero-friction address goal.

## Continuation (2026-03-04): launch-gate nightly-fuzz parameterization + Issue #1 verification rerun

Implemented:
- Extended launch orchestration to pass explicit nightly-fuzz health criteria end-to-end:
  - `scripts/launch/run_readiness_gate.sh`
  - `scripts/launch/generate_evidence_bundle.sh`
  - `scripts/launch/run_launch_rehearsal.sh`
- Added pass-through controls for:
  - workflow name
  - lookback window days
  - minimum runs
  - max fetched runs
  - allow-in-progress override
- Updated manual-dispatch workflows to expose the same parameters:
  - `.github/workflows/launch-evidence.yml`
  - `.github/workflows/launch-rehearsal.yml`
- Added dedicated Issue #1 launch checker:
  - `scripts/launch/check_issue1_core_goals.sh`
  - verifies `near-account-id` and `bitinfinity-tools` target suites.
- Wired Issue #1 checker into readiness/evidence/rehearsal flow by default, with explicit opt-out for fast local iteration:
  - `--skip-issue1-goal-checks`.
- Updated launch docs/README to document strict/tuned nightly-fuzz gating and Issue #1 checker behavior.

Verification:
- `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally.
- `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist --check-nightly-fuzz-health --nightly-fuzz-workflow CI --nightly-fuzz-branch jayzalowitz/btc-near-fork-plan --nightly-fuzz-window-days 0 --nightly-fuzz-min-runs 0 --nightly-fuzz-max-runs 50 --nightly-fuzz-allow-in-progress` passed locally.
- `./scripts/launch/generate_evidence_bundle.sh --mode smoke --check-nightly-fuzz-health --nightly-fuzz-workflow CI --nightly-fuzz-branch jayzalowitz/btc-near-fork-plan --nightly-fuzz-window-days 0 --nightly-fuzz-min-runs 0 --nightly-fuzz-max-runs 50 --nightly-fuzz-allow-in-progress --allow-dirty` passed locally.
- `./scripts/launch/run_launch_rehearsal.sh --mode smoke --check-nightly-fuzz-health --nightly-fuzz-workflow CI --nightly-fuzz-branch jayzalowitz/btc-near-fork-plan --nightly-fuzz-window-days 0 --nightly-fuzz-min-runs 0 --nightly-fuzz-max-runs 50 --nightly-fuzz-allow-in-progress --skip-release-manifest --allow-dirty` passed locally.

Issue #1 core-goal verification rerun:
- Re-ran the target suites used in prior Issue #1 validation:
  - `cargo test --manifest-path near-account-id/Cargo.toml -- --nocapture`
    - result: `10 passed`, `0 failed`.
  - `cargo test -p bitinfinity-tools -- --nocapture`
    - result: `22 passed`, `0 failed`, `1 ignored`.
- Outcome: Issue #1 core-goal coverage remains green on current launch-readiness branch changes.

## Continuation (2026-03-04): clean-mode rehearsal orchestration fix

Implemented:
- Fixed a launch-rehearsal orchestration failure where `run_launch_rehearsal.sh` created `artifacts/launch-rehearsals/...` before invoking `generate_evidence_bundle.sh`, causing a false dirty-worktree rejection in strict mode.
- Updated `run_launch_rehearsal.sh` to stage evidence and optional release-manifest outputs under a temporary directory first, then copy artifacts into the final rehearsal output root after command execution completes.
- Preserved final artifact layout and summary paths under:
  - `artifacts/launch-rehearsals/<timestamp>-<shortsha>/...`

Verification:
- `./scripts/launch/run_launch_rehearsal.sh --mode smoke --include-release-manifest --release-manifest-skip-build --operator "launch-readiness"` passed locally on commit `3cdce7b63` with strict clean-worktree behavior (without `--allow-dirty`).
- The run produced:
  - readiness-gate pass,
  - evidence bundle,
  - release artifact manifest,
  - rehearsal summary artifacts.

## Continuation (2026-03-04): full-mode strict rehearsal release-manifest fix

Implemented:
- Fixed a follow-on strict-mode failure in `run_launch_rehearsal.sh` where full-gate execution mutated tracked `target/.rustc_info.json`, causing `generate_release_manifest.sh` to reject the worktree as dirty.
- Added targeted pre-manifest cleanup in strict mode:
  - restore generated `target/.rustc_info.json` before invoking release-manifest generation.

Verification:
- `./scripts/launch/run_launch_rehearsal.sh --mode full --include-fuzz --release-manifest-skip-build --operator "launch-readiness"` passed locally on commit `783a956b1` with strict clean-worktree behavior (without `--allow-dirty`).
- The run produced:
  - full readiness-gate pass (`--full --include-fuzz`),
  - launch evidence bundle,
  - release artifact manifest,
  - rehearsal summary artifacts.

## Continuation (2026-03-04): genesis determinism launch gate automation

Implemented:
- Added explicit deterministic genesis-time support to genesis tooling:
  - `bitinfinity-tools generate-genesis --genesis-time <RFC3339>`.
  - tool normalizes provided timestamp to canonical UTC RFC3339-second format.
- Added launch gate #9 verifier:
  - `scripts/launch/check_genesis_determinism.sh`.
  - executes two genesis generation runs with identical input/config and asserts `genesis.json` SHA256 hash equality.
  - emits machine-readable metadata (`--json-out`) including hash pair, record counts, and total supply string.
  - requires explicit `--satoshi-address` when using `--patoshi-csv` to prevent nondeterministic auto-key generation in Patoshi reassignment flow.
- Wired deterministic-genesis check into launch readiness:
  - `scripts/launch/run_readiness_gate.sh` now runs deterministic testnet-fixture verification in smoke/full modes.
- Included deterministic-genesis artifacts in evidence bundling snapshots:
  - `scripts/launch/generate_evidence_bundle.sh` now captures `docs/genesis-determinism-check.md` and `scripts/launch/check_genesis_determinism.sh`.

Verification:
- `cargo test -p bitinfinity-tools` passed locally (`22 passed`, `0 failed`, `1 ignored`).
- `./scripts/launch/check_genesis_determinism.sh --testnet --num-accounts 32 --json-out /tmp/genesis-determinism.json` passed locally on commit `5437c6dc7`.
- `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally on commit `5437c6dc7`, including deterministic-genesis gate execution.

## Continuation (2026-03-04): genesis supply reconciliation verifier

Implemented:
- Added `bitinfinity-tools verify-genesis`:
  - loads `genesis.json`,
  - parses all account records,
  - computes `sum(account.amount + account.locked)` with `u128`,
  - compares computed total against declared `total_supply`,
  - emits optional machine-readable JSON summary (`--json-out`),
  - exits non-zero on reconciliation mismatch.
- Updated `scripts/launch/check_genesis_determinism.sh` to invoke `verify-genesis` for both reruns and fail if either reconciliation result is false.
- Updated launch docs to treat this as a gate #10 evidence primitive.

Verification:
- `cargo test -p bitinfinity-tools` passed locally (`22 passed`, `0 failed`, `1 ignored`).
- `./scripts/launch/check_genesis_determinism.sh --testnet --num-accounts 32 --json-out /tmp/genesis-determinism.json` passed locally.
  - includes `declared_total_supply == computed_total_supply`.
- `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally with deterministic-genesis + reconciliation checks enabled.

## Continuation (2026-03-04): pinned deterministic fixture hash regression gate

Implemented:
- Extended `scripts/launch/check_genesis_determinism.sh` with `--expected-hash <sha256>`:
  - validates the expected hash format,
  - fails if either rerun hash differs from expected,
  - emits `expected_hash` and `matches_expected_hash` in `--json-out` output.
- Tightened readiness execution:
  - `scripts/launch/run_readiness_gate.sh` now enforces the canonical deterministic fixture hash in gate #9:
    - `95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d`
- Updated launch docs/README for the new pinned-hash workflow.

Verification:
- `./scripts/launch/check_genesis_determinism.sh --testnet --num-accounts 100 --chain-id bitinfinity-mainnet --genesis-time 2026-01-01T00:00:00Z --expected-hash 95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d --json-out /tmp/genesis-determinism-pinned.json` passed locally.
- `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally with pinned-hash enforcement active in deterministic-genesis gate execution.

## Continuation (2026-03-04): strict rehearsal manifest guard generalized for tracked target artifacts

Implemented:
- Fixed a strict-mode launch-rehearsal regression where smoke/full readiness execution modified tracked files under `target/` (for example, `target/debug/bitinfinity-tools`), causing `generate_release_manifest.sh` to fail dirty-worktree checks.
- Updated `scripts/launch/run_launch_rehearsal.sh` to restore all tracked `target/` diffs before strict release-manifest execution (instead of only `target/.rustc_info.json`).

Verification:
- `./scripts/launch/run_launch_rehearsal.sh --mode smoke --include-release-manifest --release-manifest-skip-build --operator "launch-readiness"` passed locally on commit `196b0f43d` in strict mode (no `--allow-dirty`).
- Run output included:
  - readiness gate pass,
  - checklist report generation,
  - release manifest generation (`--skip-build`),
  - rehearsal summary artifact creation.

## Continuation (2026-03-04): full launch-readiness rerun on latest head

Verification:
- `./scripts/launch/run_readiness_gate.sh --full --include-fuzz --skip-checklist` passed locally on commit `947011618`.
- Gate coverage on this rerun included:
  - docs/script/checklist syntax checks,
  - Issue #1 target suites,
  - deterministic genesis hash + supply reconciliation with pinned expected hash,
  - release build/version checks,
  - workspace + `near-account-id` test/clippy/fmt/audit,
  - all configured fuzz-smoke targets.

## Continuation (2026-03-04): readiness pinned-hash override via environment

Implemented:
- Updated `scripts/launch/run_readiness_gate.sh` to accept `GENESIS_FIXTURE_EXPECTED_HASH` as an environment override for the pinned deterministic fixture hash.
- Documented the override in launch docs for intentional fixture-hash rotations.

Verification:
- `GENESIS_FIXTURE_EXPECTED_HASH=95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d ./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally.

## Continuation (2026-03-04): snapshot-vs-genesis supply reconciliation gate tooling

Implemented:
- Added `bitinfinity-tools verify-snapshot-supply`:
  - compares `genesis.json` supply against `bitcoin-cli gettxoutsetinfo` `total_amount`,
  - parses BTC amount to satoshis with decimal-safe validation,
  - enforces configurable satoshi tolerance (`--tolerance-sats`, default `1`),
  - emits optional machine-readable JSON summary (`--json-out`),
  - exits non-zero on tolerance violation.
- Added launch wrapper:
  - `scripts/launch/check_snapshot_supply_reconciliation.sh`
- Wired launch flows:
  - `scripts/launch/run_readiness_gate.sh`: required-doc and syntax checks include the new script/doc.
  - `scripts/launch/generate_evidence_bundle.sh`: snapshots now include reconciliation doc/script.
- Added documentation:
  - `docs/snapshot-supply-reconciliation.md`
  - README + docs hub + launch docs + site index links.

Verification:
- `cargo test -p bitinfinity-tools` passed locally (`26 passed`, `0 failed`, `1 ignored`) including new parser/conversion unit tests.
- `./scripts/launch/check_snapshot_supply_reconciliation.sh --genesis <generated genesis> --txoutsetinfo <fixture json> --tolerance-sats 0 --json-out <summary>` passed locally.
  - result: `difference_satoshis=0`, `within_tolerance=true`.
- `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally with new snapshot-reconciliation script syntax check active.
- Rerun on clean commit `fb113bc50`:
  - `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally.
  - `./scripts/launch/check_snapshot_supply_reconciliation.sh --genesis <generated genesis> --txoutsetinfo <fixture gettxoutsetinfo.json> --tolerance-sats 0 --json-out <summary>` passed locally (`difference_satoshis=0`, `within_tolerance=true`).

## Continuation (2026-03-04): snapshot gate pass-through in evidence/rehearsal orchestration

Implemented:
- Extended `scripts/launch/run_readiness_gate.sh` with optional gate #10 enforcement flags:
  - `--check-snapshot-supply`
  - `--snapshot-genesis`
  - `--snapshot-txoutsetinfo`
  - `--snapshot-tolerance-sats`
  - `--snapshot-json-out`
- Extended `scripts/launch/generate_evidence_bundle.sh` to pass the same snapshot flags into readiness execution and capture snapshot artifacts in the evidence bundle:
  - `snapshot-inputs.txt` (input paths + SHA256 hashes + tolerance),
  - `snapshot-gettxoutsetinfo.json`,
  - `snapshot-supply-check.json`.
- Extended `scripts/launch/run_launch_rehearsal.sh` to parse/validate/pass through the same snapshot flags to evidence generation and to record snapshot execution parameters in rehearsal `SUMMARY.md` / `summary.json`.
- Updated launch docs/README for the new orchestration-level gate #10 usage.

Verification:
- `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist --check-snapshot-supply --snapshot-genesis <generated genesis> --snapshot-txoutsetinfo <generated gettxoutsetinfo.json> --snapshot-tolerance-sats 0 --snapshot-json-out <summary>` passed locally (`difference_satoshis=0`, `within_tolerance=true`).
- `./scripts/launch/generate_evidence_bundle.sh --mode smoke --allow-dirty --check-snapshot-supply --snapshot-genesis <generated genesis> --snapshot-txoutsetinfo <generated gettxoutsetinfo.json> --snapshot-tolerance-sats 0 --snapshot-json-out <summary> --out-dir <tmp>` passed locally and produced snapshot artifacts in the bundle.
- `./scripts/launch/run_launch_rehearsal.sh --mode smoke --allow-dirty --skip-release-manifest --check-snapshot-supply --snapshot-genesis <generated genesis> --snapshot-txoutsetinfo <generated gettxoutsetinfo.json> --snapshot-tolerance-sats 0 --snapshot-json-out <summary> --out-dir <tmp>` passed locally, confirming end-to-end snapshot gate pass-through.

## Continuation (2026-03-04): technical whitepaper baseline + launch-gate enforcement

Implemented:
- Added `docs/technical-whitepaper.md` as a repository-controlled technical baseline covering:
  - architecture and identity/signature model,
  - genesis and supply reconciliation model,
  - economics/throughput positioning,
  - security and launch control-plane references.
- Updated launch gating:
  - `scripts/launch/run_readiness_gate.sh` now requires `docs/technical-whitepaper.md` in required launch docs.
  - `scripts/launch/generate_evidence_bundle.sh` now snapshots `technical-whitepaper.md` and includes it in bundle summary.
- Updated discovery/docs surfacing:
  - `docs/documentation-hub.md`
  - `docs/index.html`
  - `README.md`
  - `docs/launch-evidence-bundle.md`
  - `docs/launch-readiness-gates.md`

Verification:
- `bash -n scripts/launch/run_readiness_gate.sh` passed.
- `bash -n scripts/launch/generate_evidence_bundle.sh` passed.
- `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally after whitepaper gating changes.
- `./scripts/launch/generate_evidence_bundle.sh --mode smoke --skip-gate --allow-dirty --out-dir <tmp>` passed locally and produced a bundle containing `technical-whitepaper.md`.
