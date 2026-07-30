# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

## Project Overview

alarm-receiver is a Rust microservice in the home-anthill smart home system. It subscribes to heartbeat (`online/+/features/+`) and generic alarm (`alarms/+/features/+/+`) MQTT topics, validates signed envelopes, updates online state in Redis DB 0, and queues alarms in Redis DB 3.

## Build & Development Commands

All common tasks are available via `make`:

| Command | Description |
|---------|-------------|
| `make build` | Format + lint + debug build |
| `make release` | Format + lint + release build |
| `make test` | Run tests (RUST_BACKTRACE=full, single-threaded) |
| `make test-coverage` | Generate code coverage via grcov |
| `make fmt` | Format code with `cargo fmt` |
| `make lint` | Lint with `cargo clippy` |
| `make check` | Audit dependencies for vulnerabilities (`cargo audit`) |
| `make run` | Format + lint + watch mode (requires cargo-watch) |
| `make deps` | Install all dev dependencies |
| `make clean` | Remove build artifacts |

Run a single test: `cargo test <test_name> -- --nocapture --test-threads 1`

## Architecture

**Structure:** The service is split into a library (`src/lib.rs`) and binary (`src/main.rs`). The binary orchestrates initialization and the main loop; the library exports all logic for potential reuse.

**Entry point:** `src/main.rs` — `#[rocket::main]` async entry point that initializes config, connects to Redis (with optional authentication), sets up MQTT client, spawns the MQTT event loop as a background `tokio::task::spawn` task, then launches a Rocket HTTP server for health checking.

**Module structure:**
- **config/** — `Env` struct deserialized from environment variables (with `#[serde(default)]` for optional fields like `redis_username` and `redis_password` to maintain backwards compatibility); logging setup (daily rolling files via tracing)
- **mqtt/** — `MqttClient` (async paho-mqtt wrapper), `MqttOptions` (connection/TLS builder with proper error propagation), `MqttConfig`, `get_string_payload` / `get_bytes_from_payload` helpers; subscribes at QoS 0 with `clean_session(true)` because online heartbeats are ephemeral and stale persistent subscriptions can duplicate deliveries after restarts
- **db/** — `insert_or_update_online()` manages DB 0 heartbeat hashes; `insert_alarm_event()` writes a 24-hour DB 3 event hash plus the `alarms:pending` sorted-set index. UUIDs are validated before Redis key interpolation.
- **models/** — Two distinct envelope types: `Notification<T>` is the raw MQTT JSON (just `apiToken`, `deviceUuid`, `featureUuid`, `payload`); `Message<T>` is the internal form that additionally embeds the parsed `Topic`. `OnlineMqttPayload` is intentionally empty — the `payload` field in incoming JSON is always `{}`. `Topic` parses `online/{device_uuid}/features/{feature_uuid}` topic strings.
- **errors/** — Custom error enums via `thiserror` (`MessageError` with `PayloadTooLargeError`, `RedisError` with `InvalidUuidError`, `MqttError` with `SslConfigError`); `ApiResponse` / `ApiError` types (in `api_error.rs`) implement Rocket's `Responder` for JSON HTTP responses
- **routes/** — `GET /keepalive` returns `{"alive": true}` with HTTP 200; used by Kubernetes liveness probes
- **catchers/** — Rocket error catchers for HTTP 400, 404, 500, 503; log via `tracing::error!` and return `ApiError` JSON

**Message flow:** MQTT message → size/UTF-8/JSON checks → strict topic binding → load API token and registered feature name from MongoDB → verify `deviceUuid\nfeatureUuid\nfeatureName\ntimestamp\nnonce\npayloadJson` HMAC → claim nonce in Redis DB 2 → update DB 0 heartbeat state or queue a DB 3 alarm. `motion` requires `value=1`; `thermostat-mode-error` is allowed only for a registered `mode` feature and requires `value=-1`.

The MQTT processing path logs received, parsed, and successfully processed messages at `INFO` with `target: "app"` so they appear on stdout in development and production. Message metadata logs include only useful clear-text identifiers and payload (`device_uuid`, `feature_uuid`, `payload`); protected protocol fields such as `timestamp`, `nonce`, and `signature` are omitted. Raw MQTT payloads must not be logged, even at `DEBUG`. Processing failures are logged at `ERROR` with the MQTT topic when available.

**HTTP server:** Rocket runs concurrently with the MQTT loop (port 8088 in debug, port 80 in release). Config lives in `Rocket.toml` (JSON body limit 8 KiB, `cli_colors = false` for plaintext logs). The `secret_key` in `Rocket.toml` is a placeholder — replace with a real key in production (`openssl rand -base64 32`).

## Code Style

- Rust 2024 edition
- Formatting: `rustfmt.toml` — 4-space indent, 120 char line width
- EditorConfig: 2-space indent for non-Rust files, UTF-8, LF
- Error handling: `thiserror` for domain errors, `anyhow` for propagation; use `X.into()` not `anyhow::Error::from(X)`
- Prefer `&str` over `&String` and pass `bool`/`Copy` types by value, not reference
- Use `DeserializeOwned` instead of `Deserialize<'a>` when the deserialized value does not borrow from the input
- Use `inspect_err(|e| error!(...))` + `?` instead of `match Ok/Err` blocks used only for logging
- Use `Duration::from_secs` not `Duration::from_millis` for whole-second durations
- Async/await throughout (tokio runtime)
- Combined CA file for TLS is written atomically to `/tmp/rootca_and_cert.pem`

## Security Conventions

- **Redis key injection prevention**: All three MQTT payload UUIDs (`api_token`, `device_uuid`, `feature_uuid`) are validated as UUIDv4 before interpolation into Redis keys. Untrusted input is never concatenated directly into keys; validation happens first using the `uuid` crate.
- MQTT payload size is capped at 64 KiB (`MAX_PAYLOAD_BYTES`) before JSON deserialization to prevent excessive memory allocation
- MQTT credentials are never logged — username logged as `[REDACTED]`, password not logged at all
- Redis credentials are never logged (though their presence in the constructed URI is noted in logs)
- Signed MQTT replay protection uses Redis `SET signed-replay:v1:{device_uuid}:{feature_uuid}:{nonce} 1 NX EX 720` after HMAC verification and before online-state updates.
- Online heartbeat signatures bind the feature class with the literal `online` in the canonical signed payload.
- MQTT processing logs must not print signed timestamp, nonce, or signature values; use the notification summary formatter for message metadata.
- LWT (Last Will and Testament) message is published to `online/lwt` (service-scoped topic) rather than a generic topic
- SSL/TLS errors are propagated with context (via `SslConfigError` variant) — no `.unwrap()` on certificate operations
- Combined CA file is written atomically using `OpenOptions::truncate(true)` to prevent TOCTOU race conditions

## Testing

- Tests are split into `src/tests_integration/` modules and run through the crate test harness
- Tests require Redis and Mosquitto (MQTT broker) running locally
- CI uses `ENV=testing` to skip file-based logging
- Test Redis keys use `test_` prefix (via `from_uuid_to_db_key` which checks `ENV`)
- Tests run single-threaded: `--test-threads 1`

## Docker

5-stage build using `cargo-chef` for dependency caching:
1. **Chef** — base image (`rust:trixie`) with build tools + cargo-chef installed
2. **Planner** — generates `recipe.json` from the source tree
3. **Builder** — cooks dependencies then compiles the release binary
4. **system-deps** (`debian:trixie-slim`) — installs CA certs; pre-creates `/app/logs` owned by UID 65534
5. **Runtime** (`dhi.io/debian-base:trixie`) — hardened image, no package manager; copies CA certs, app dir, and binary; runs as UID 65534 (`nobody`)

```bash
docker build -t ks89/alarm-receiver:latest .
```

## Environment Variables

See `.env_template` for all required variables. Key settings:

- **Redis**: `ONLINE_REDIS_URI` selects online DB 0, `REPLAY_REDIS_URI` selects replay DB 2, and `ALARMS_REDIS_URI` selects alarms DB 3. DB 15 is test-only. Credentials are injected consistently into all three URIs.
- **MQTT**: `MQTT_URL`, `MQTT_PORT`, `MQTT_CLIENT_ID`, `MQTT_AUTH`, `MQTT_USER`, `MQTT_PASSWORD`, `MQTT_TLS`, `ROOT_CA`, `MQTT_CERT_FILE`, `MQTT_KEY_FILE`. The sample `.env_template` uses a dedicated subscriber account (`alarm_receiver_sub`) for this service.
- **Logging**: `LOG_LEVEL` controls tracing filter (debug, info, warn, error)
