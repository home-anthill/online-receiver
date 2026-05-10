# Changelog (AI-assisted changes)

## Redis Online State

**`modifiedAt` always set**
`insert_or_update_online()` now writes `modifiedAt` on initial Redis hash creation as well as on updates.
For new keys, `createdAt` and `modifiedAt` are intentionally equal so readers can rely on both fields
being present without special-casing first-seen devices.

## Observability

**Redacted MQTT processing logs**
The MQTT loop now logs received, parsed, and successfully processed online messages at `INFO` using `target: "app"`, so they appear on stdout in development and production. The log output includes topic, device UUID, feature UUID, payload size, and payload. Signed timestamp, nonce, and signature are omitted from message metadata logs. Processing failures include the MQTT topic when available.

**Clean MQTT subscriber session**
The online receiver now connects with `clean_session(true)`. Online heartbeat messages are ephemeral, so persistent broker subscriptions are unnecessary and can cause duplicate deliveries after repeated restarts with the same client ID. Raw signed MQTT payloads are no longer logged at `DEBUG`; only payload size is logged before parsing.

## HTTP Health Endpoint

**Rocket HTTP Server Added**
Added a Rocket web server running concurrently with the MQTT event loop. The entry point was changed from `#[tokio::main]` to `#[rocket::main]`; the MQTT loop now runs as a background `tokio::task::spawn` task. Rocket serves a `GET /keepalive` health endpoint returning `{"alive": true}` (HTTP 200), used by Kubernetes liveness probes. Runs on port 8088 in debug mode and port 80 in release mode.

**New Modules: routes/, catchers/, errors/api_error.rs**
Added `routes/api.rs` with the `keep_alive` handler. Added `catchers/mod.rs` with Rocket error catchers for HTTP 400, 404, 500, and 503 — each logs via `tracing::error!` and returns an `ApiError` JSON response. Added `errors/api_error.rs` defining `ApiResponse` and `ApiError` structs, both implementing Rocket's `Responder` trait for JSON HTTP responses.

**Rocket.toml Configuration**
Added `Rocket.toml` to configure ports, JSON body limits (8 KiB), and `cli_colors = false` for plaintext logs. The `secret_key` in the release profile is a placeholder — must be replaced with a real key in production (`openssl rand -base64 32`).

## Security

**Signed envelope validation tightened**
Online MQTT notifications must carry a 32-character lowercase-hex nonce and 64-character lowercase-hex signature before HMAC verification and Redis replay-cache keying.

**Online signed payload binds feature class**
Online heartbeat signatures now include the literal `online` feature name in the canonical HMAC input (`deviceUuid\nfeatureUuid\nonline\ntimestamp\nnonce\npayloadJson`). This keeps the shared firmware telemetry signing format aligned with the consumer's feature-bound sensor telemetry protocol.

**TLS/SSL Certificate Handling Hardened**
Replaced three `.unwrap()` calls on `SslOptionsBuilder::trust_store`, `key_store`, and `private_key` with proper error propagation via `MqttError::SslConfigError`. Added `SslConfigError(String)` variant to `MqttError`. Malformed certificate input now returns an error instead of crashing the service.

**Sensitive Values Redacted from Logs**
MQTT username was being logged in plaintext despite a comment claiming all sensitive values were redacted. Changed to log `mqtt_user = [REDACTED]`. Redis credentials are never logged—only the presence of configured credentials is noted in logs.

**Redis Key Injection Prevention**
All three UUIDs from untrusted MQTT payloads (`api_token`, `device_uuid`, `feature_uuid`) are now validated as UUIDv4 before interpolation into Redis keys using the `uuid` crate. Added `is_valid_uuid_v4` validation function and `InvalidUuidError` variant to `RedisError`. This eliminates the risk of key injection from malformed input passed to Redis.

**MQTT Payload Size Validation**
Added `MAX_PAYLOAD_BYTES = 65_536` (64 KiB) size check in `get_string_payload` before JSON deserialization. Large payloads are rejected with `MessageError::PayloadTooLargeError` to prevent excessive memory allocation from oversized MQTT messages.

**Combined CA File Handling**
CA file merging now uses atomic file operations to eliminate time-of-check time-of-use (TOCTOU) race conditions. Replaced the `exists()` check → `remove_file()` → `File::create()` sequence with a single atomic `OpenOptions::new().write(true).create(true).truncate(true).open()` operation. Combined file is written to an absolute path (`/tmp/rootca_and_cert.pem`) rather than CWD-relative paths, removing path-traversal risks from unexpected working-directory changes.

**Last Will and Testament Topic Scoped**
LWT message is now published to `online/lwt` instead of the generic `test` topic, preventing disconnect events from leaking to unrelated subscribers on that generic topic.

## Idiomatic Rust

**Error Handling Modernized**
Replaced verbose `anyhow::Error::from(X)` with idiomatic `X.into()` at early-return sites and removed the outer wrapper from `map_err` closures. Simplified error-only logging patterns using `.inspect_err(|err| error!(...))` instead of `match` blocks used solely for logging. Silent error discards in Redis operations (`exists` and `hset_multiple`) now log the original error at `error!` level before returning the domain error variant, improving observability.

**Function Signatures Improved**
Changed `&String` parameters to `&str` in `merge_ca_files` and `build_connect_options` to accept any string-like value and follow Rust API guidelines. Changed `&bool` parameters to `bool` in `build_connect_options` for `mqtt_auth` and `mqtt_tls`, eliminating unnecessary dereferences inside the function since `bool` is `Copy`. Removed redundant `use std::string::String` imports from multiple modules (prelude item).

**Type Annotations Refined**
Changed Redis `exists` return type from manual `Value::Int(1)` comparison to idiomatic `bool` by leveraging the `redis` crate's support for direct `bool` returns. Replaced `Deserialize<'a>` with `DeserializeOwned` in `message_payload_to_bytes` to correctly reflect that deserialized values don't borrow from input; since all fields are owned, `DeserializeOwned` (equivalent to `for<'de> Deserialize<'de>`) is the appropriate bound.

**Duration and String Expressions Clarified**
Replaced `Duration::from_millis(30000)` and `Duration::from_millis(5000)` with `Duration::from_secs(30)` and `Duration::from_secs(5)` to express intent without mental division. Removed redundant `String` → `str` conversions via `.as_str()` since `String` derefs to `str` and `&db_key` is idiomatic. Removed duplicate topic logging in `subscribe` which logged the topic list twice in different formats.

**Error Propagation Improved**
Changed `let _ = process_mqtt_message(...).await` which silently dropped all processing errors to use `.inspect_err(|err| error!(...))` so failures are visible in logs.

## Configuration

**Redis Authentication Support**
Added `redis_username` and `redis_password` fields to `Env` struct, both optional with `#[serde(default)]` for backwards compatibility. Fields default to empty string when env vars are absent, preserving compatibility for CI. Credentials are injected into the Redis URI before connection: `redis://host:port` becomes `redis://username:password@host:port`. If only password is set (empty username), the URI becomes `redis://:password@host:port`, compatible with legacy `requirepass` mode. The constructed Redis URL is never logged.

**Environment Template Updated**
`.env_template` now includes `REDIS_USERNAME=redisuser` and `REDIS_PASSWORD=Password1!` entries, matching the named ACL user created by the local Docker Redis command.
Updated the MQTT sample credentials from the generic Mosquitto example account to a service-specific subscriber account: `MQTT_USER=online_receiver_sub` and `MQTT_PASSWORD=OnlineReceiverPassword1!`. This keeps local examples aligned with the intended per-service MQTT principal.
