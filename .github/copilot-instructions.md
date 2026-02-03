# Copilot Instructions

## Build, test, lint
- Build (release): `cargo build --release`
- Build (debug): `cargo build`
- Run tests: `cargo test`
- Run a single test: `cargo test test_uri_direct`
- Lint: no dedicated lint task found (use `cargo clippy` if you add it locally)

## High-level architecture
- Single-binary Rust CLI (`src/main.rs`) that calls the FreeDesktop Screenshot portal over D-Bus (`org.freedesktop.portal.Screenshot`) using zbus, waits for the `Response` signal, and copies the returned temp file into a local destination directory.
- Screenshot result parsing is centralized in `extract_uri_from_map`, which recursively scans `zbus::zvariant::Value` payloads to find a `file://` URI.
- Destination directory defaults to `~/Pictures` and can be overridden with `CLAW_SCREENSHOT_DIR`.

## Key conventions
- Portal responses are handled via async signal stream with a 10s timeout; if no file is found, the app returns an error rather than silently succeeding.
- The app logs via `env_logger` (configure with `RUST_LOG=info` or similar); success is also printed as `SAVED:<path>`.
- Tests focus on portal response parsing (`src/tests.rs`) rather than D-Bus integration.
- For reliable portal permissions, run via a desktop entry (`.desktop`) as described in README/installer; direct terminal runs may be denied by the portal.
