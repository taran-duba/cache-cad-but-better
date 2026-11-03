# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Permafrost: a Tauri v2 desktop app (Rust core + React UI) that shares SolidWorks files over Google Drive for FRC teams, with check-out locks and atomic check-ins. The repo also contains a C# sidecar (SwRefs) and a separate marketing website. See `README.md` for the problem/solution summary and `docs/SETUP.md` / `docs/TESTING.md` for team setup and the Windows test plan.

## Commands

```bash
pnpm install
CADSYNC_DEMO=1 pnpm tauri dev          # run the app against an in-memory fake Drive (no Google setup; works on macOS)
pnpm build                             # tsc + vite build of the UI only
cargo test --manifest-path src-tauri/Cargo.toml                 # Rust tests
cargo test --manifest-path src-tauri/Cargo.toml <test_name>     # single Rust test
dotnet test sidecar/SwRefs.Tests       # SwRefs tests
pnpm tauri build                       # installer/dmg -> src-tauri/target/release/bundle/
```

- There is no JS lint or JS test runner. CI (`.github/workflows/macos.yml`, `windows.yml`) runs the Rust and SwRefs tests, builds, then smoke-tests the built app.
- On Windows, every Rust build (`tauri dev`, `cargo test`) needs `src-tauri/binaries/SwRefs-x86_64-pc-windows-msvc.exe`. Run `pnpm sidecar` once first (it runs `scripts/publish-swrefs.ps1`). `tauri.windows.conf.json` wires this into `pnpm tauri build` and adds the `externalBin`. macOS/Linux don't bundle SwRefs.
- The website is a separate package: `cd website && pnpm dev | build | typecheck`.

## Architecture

**Rust core (`src-tauri/src/`)** — the logic lives here; the UI is thin.
- `drive/`: Drive client behind a trait, with the real HTTP implementation (`http.rs`), OAuth (`auth.rs`), and an in-memory `fake.rs` used by tests and demo mode. Prefer testing sync logic against the fake.
- `index/`: SQLite index of the chosen Drive folder plus deterministic Drive-name → safe-Windows-path mapping (`paths.rs`, `names.rs`). The UI browses this index and never waits on the network.
- `sync/`: `crawl` (initial crawl + incremental Drive change feed), `pull` (planner), `locks` (Drive content restriction + lock properties), `push` (atomic check-in: assembly and changed parts land together or not at all), `remove`, `localfs` (read-only local files for anything not checked out).
- `refs.rs`: client for the SwRefs sidecar and pre-check-in validation (blocks missing files, files outside the vault, duplicate names).
- `app/`: Tauri commands (`commands.rs`, all registered in `lib.rs`'s `generate_handler!`), config, `AppState`, background poller, logging/crash reports, demo mode, and `smoke.rs`.

**Frontend (`src/`)**: React 19 + Vite. `src/api.ts` is the typed wrapper over Tauri `invoke`; when adding a command, register it in `lib.rs` and add its wrapper there. Views are in `src/views/`.

**Sidecar (`sidecar/SwRefs`, C#/.NET 8)**: reads assembly references by driving SolidWorks itself through its API, so it only works on Windows with SolidWorks installed. Published as a self-contained single-file exe.

**Website (`website/`)**: standalone Vite/React site with prerendering. `website/api/release.ts` is a serverless function that resolves the latest installers from GitHub Releases (`/download/windows|windows-msi|mac`).

## Environment variables and build-time config

- `CADSYNC_DEMO=1`: start in demo mode.
- `CADSYNC_SMOKE=<result.json>`: run the app as a headless-ish smoke test (used by CI) and write results to that file; see `app/smoke.rs` for `CADSYNC_SMOKE_HOLD_SECS` and `CADSYNC_SMOKE_REQUIRE_SIGNIN`.
- `CADSYNC_BUILTIN_CLIENT_ID` / `CADSYNC_BUILTIN_CLIENT_SECRET`: compiled into the binary via `option_env!` so Google sign-in is built in. CI sets them from the `CADSYNC_GOOGLE_CLIENT_ID/SECRET` repo secrets; locally, `src-tauri/google-oauth.json` (gitignored) is used. Never commit it.

## Notes

- The bundle identifier is `org.frc.cadsync` and the env vars use the `CADSYNC_` prefix: the project was renamed from CadSync/CacheCAD-era naming, so both names appear.
- Mac builds are ad-hoc signed and not notarized.
- In-app updates: `tauri-plugin-updater` reads `latest.json` from the newest GitHub Release (endpoint and public key in `tauri.conf.json`). The frontend side is `src/updater.ts`. `release.yml` (tag `v*`, must equal the `tauri.conf.json` version) builds, signs and publishes. `createUpdaterArtifacts` is deliberately only set in that workflow's `--config` override, so normal CI and local builds don't need the signing key. See README "Releasing and updating".
- Status per README: not yet run against real Google Drive or SolidWorks.
