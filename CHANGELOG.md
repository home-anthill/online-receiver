# Changelog

## 3.0.0

### Features

- `insert_or_update_online()` now writes `modifiedAt` on initial Redis hash creation as well as on updates, with new keys using the same value for `createdAt` and `modifiedAt`.
- The MQTT loop now logs received, parsed, and successfully processed online messages at `INFO` with redacted signature metadata and topic-aware failure logs.
- Added a Rocket web server running alongside the MQTT event loop, including a `GET /keepalive` health endpoint returning `{"alive": true}` for Kubernetes liveness probes.
- Added `routes/api.rs`, `catchers/mod.rs`, and `errors/api_error.rs` for the health endpoint, JSON API responses, and Rocket error catchers for HTTP 400, 404, 500, and 503.
- Added Redis authentication support through optional `redis_username` and `redis_password` environment fields, with credentials injected into the Redis URI without logging the constructed URL.

### Bug fixes

- MQTT now connects with `clean_session(true)` to avoid duplicate delivery of ephemeral online heartbeat messages after restarts with the same client ID.
- Raw signed MQTT payloads are no longer logged at `DEBUG`; only payload size is logged before parsing.
- MQTT message processing errors are no longer silently discarded and are now logged with `.inspect_err(|err| error!(...))`.

### Security issues

- Online MQTT notifications must now carry a 32-character lowercase-hex nonce and 64-character lowercase-hex signature before HMAC verification and Redis replay-cache keying.
- Online heartbeat signatures now bind the literal `online` feature name in the canonical HMAC input.
- Replaced `.unwrap()` calls in TLS/SSL certificate setup with error propagation via `MqttError::SslConfigError`, so malformed certificate input no longer crashes the service.
- MQTT usernames and Redis credential details are redacted from logs.
- UUIDs from untrusted MQTT payloads are validated as UUIDv4 before interpolation into Redis keys, preventing Redis key injection.
- Added a 64 KiB MQTT payload size limit before JSON deserialization to prevent excessive memory allocation.
- CA file merging now uses atomic file operations and writes to `/tmp/rootca_and_cert.pem`, avoiding TOCTOU and CWD-relative path risks.
- LWT messages are now published to `online/lwt` instead of the generic `test` topic.

### Idiomatic Rust issues

- Replaced verbose `anyhow::Error::from(X)` and wrapper-heavy `map_err` patterns with idiomatic `.into()` and simpler closures.
- Replaced logging-only `match` blocks and silent Redis error discards with `.inspect_err(...)` and explicit error logs.
- Changed `&String` parameters to `&str` and `&bool` parameters to `bool` in MQTT helper functions.
- Removed redundant `use std::string::String` imports.
- Changed Redis `exists` handling to use the crate's direct `bool` return support instead of manually comparing `Value::Int(1)`.
- Replaced `Deserialize<'a>` with `DeserializeOwned` in `message_payload_to_bytes`.
- Replaced millisecond durations with `Duration::from_secs(...)` where appropriate.
- Removed redundant `.as_str()` conversions and duplicate topic logging in `subscribe`.

### Chores

- Added `Rocket.toml` with port configuration, 8 KiB JSON body limits, and plaintext log settings.
- Documented that the release `secret_key` placeholder in `Rocket.toml` must be replaced before production use.
- Updated `.env_template` with Redis username/password entries and service-specific MQTT sample credentials.

### Tests

- No test-only changes recorded.
