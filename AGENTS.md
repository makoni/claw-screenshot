# Agent Instructions

Guidance for AI agents (and humans) working in this repository.

## Build, test, lint
- Build (release): `cargo build --release`
- Build (debug): `cargo build`
- Run tests: `cargo test`
- Run a single test: `cargo test test_uri_direct`
- Lint: no dedicated lint task in CI; run `cargo clippy` locally.

## High-level architecture
- Single-binary Rust CLI (`src/main.rs`) that calls the FreeDesktop Screenshot portal over D-Bus (`org.freedesktop.portal.Screenshot`) using zbus, waits for the `Response` signal, and copies/moves the returned temp file into a local destination directory.
- Screenshot result parsing is centralized in `extract_uri_from_map`, which recursively scans `zbus::zvariant::Value` payloads to find a `file://` URI.
- Destination directory defaults to the XDG pictures dir (`~/Pictures`) and can be overridden with `CLAW_SCREENSHOT_DIR`. If neither can be resolved, the app errors out with a hint to set `CLAW_SCREENSHOT_DIR` — there are no hardcoded fallback paths.
- When `CLAW_SCREENSHOT_DIR` is set the file is moved (rename, with copy+remove fallback); for the default directory it is copied, leaving the portal's temp file in place.

## Key conventions
- Portal responses are handled via async signal stream with a 10s timeout; if no file is found, the app returns an error rather than silently succeeding.
- The app logs via `env_logger` (configure with `RUST_LOG=info` or similar); success is printed to stdout as `Saved screenshot: <path>`.
- Tests focus on portal response parsing (`src/tests.rs`) rather than D-Bus integration.
- For reliable portal permissions, run via a desktop entry (`.desktop`) as described in README/installer; direct terminal runs may be denied by the portal.

## Repo layout
- `src/` — all code (single binary + tests module).
- `installer/` — install.sh / uninstall.sh + hicolor icons.
- `flatpak/` — Flatpak manifest, metainfo, .desktop.
- `.github/workflows/` — CI: binaries, deb/rpm packages, AppImage, Flatpak, release.
