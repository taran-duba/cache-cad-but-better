# Permafrost

SolidWorks + Google Drive file sharing for FRC teams, built to fix what went wrong with CacheCAD:

| Problem | What Permafrost does |
| --- | --- |
| Slow loading | A local SQLite index of your CAD folder (a whole shared drive, or just one folder inside it), updated incrementally from Drive's change feed. Browsing never waits on the network. |
| Crashes | Drive names (`/`, emoji, `CON`, duplicates, trailing dots) are mapped to safe Windows names; sync runs in the background with a crash-safe journal. |
| Overwrites and conflicts | Check-out locks (Drive content restriction plus lock properties), and read-only local files for anything you haven't checked out. |
| Broken mates | Atomic check-ins: an assembly and its changed parts arrive together or not at all. References are read by asking SolidWorks itself, and check-ins are blocked for missing files, files outside the vault, and duplicate names. |

## Layout

- `src-tauri/`: Rust core (Tauri v2)
  - `drive/`: Google Drive client, OAuth, and an in-memory fake for tests and demo mode
  - `index/`: SQLite index and deterministic path mapping
  - `sync/`: crawl and changes, pull planner, locks, atomic push
  - `refs.rs`: SwRefs client and pre-check-in validation
  - `app/`: Tauri commands, config, background poller
- `src/`: React UI
- `sidecar/`: SwRefs (C#), which reads SolidWorks references through the SolidWorks API
- `docs/SETUP.md`: one-time team setup (Google OAuth client, CAD folder)
- `docs/TESTING.md`: step-by-step Windows test plan with two PCs, and a results table to fill in

## Develop

```bash
pnpm install
```

```bash
CADSYNC_DEMO=1 pnpm tauri dev
```

Demo mode runs against an in-memory drive with sample robot files, so no Google setup is needed.
It works on macOS too. SolidWorks reference reading only works on Windows.

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

```bash
dotnet test sidecar/SwRefs.Tests
```

## Build the Windows installer

Build on a Windows x64 machine. SolidWorks isn't needed to build; it only has to be installed on
the PCs that run Permafrost.

1. Install Node.js with pnpm, Rust (MSVC toolchain, the `rustup` default on Windows), the
   Microsoft C++ Build Tools and WebView2 ([Tauri prerequisites](https://tauri.app/start/prerequisites/)),
   and the .NET 8 SDK.
2. Build:

   ```powershell
   pnpm install
   pnpm tauri build
   ```

The installers are written to `src-tauri/target/release/bundle/` (`nsis/*-setup.exe` and
`msi/*.msi`).

On Windows, `pnpm tauri build` first runs `pnpm sidecar` (set in `src-tauri/tauri.windows.conf.json`),
which runs [scripts/publish-swrefs.ps1](scripts/publish-swrefs.ps1). That publishes SwRefs as a
self-contained single-file `win-x64` exe to `src-tauri/binaries/SwRefs-x86_64-pc-windows-msvc.exe`,
and Tauri bundles it (`externalBin`) as `SwRefs.exe` next to `permafrost.exe`, which is where the app
looks for it.

Every Rust build on Windows (`pnpm tauri dev`, `cargo test`) needs that file to exist, so on a
Windows dev machine run `pnpm sidecar` once first.

macOS and Linux builds don't bundle SwRefs. The Windows-only settings are in
`tauri.windows.conf.json`.

## Mac build

CI builds an Apple Silicon `Permafrost.app` and `.dmg` on every push (`.github/workflows/macos.yml`).
It's ad-hoc signed, not notarized, so on another Mac the first launch needs right-click › Open.
There's no Intel build yet. SolidWorks doesn't run on macOS, so reference checking is off there;
syncing, locks and check-ins work.

Build one yourself with `pnpm tauri build`.

## Releasing and updating

Installed copies update themselves. They check GitHub Releases for a newer signed version (at
startup and every 6 hours), show an **Update to x.y.z · restart** button, and install it only when
clicked. Settings › Updates has a manual **Check for updates**. Dev builds don't check.

To ship a release:

1. Bump `version` in `src-tauri/tauri.conf.json` (and `package.json`, `src-tauri/Cargo.toml`).
2. Commit, then `git tag v<version> && git push origin v<version>`.

[`.github/workflows/release.yml`](.github/workflows/release.yml) builds and signs the Windows and
Apple Silicon bundles, uploads them with a `latest.json` manifest to a draft release, and publishes
it once both platforms succeed. The tag must match the `tauri.conf.json` version.

One-time setup: the updater signing key. The public half is in `tauri.conf.json`; keep the private
key (and its password, if any) in the repo secrets `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. If it is lost, installed copies can no longer be updated and
people have to reinstall by hand. Generate a new pair with `pnpm tauri signer generate -w <file>`
and put the new public key in `tauri.conf.json`. Updates only apply to builds that already contain
the updater, so the first release with it must be installed manually.

## Not done yet

- Not yet run against real Google Drive or SolidWorks. Work through
  [docs/TESTING.md](docs/TESTING.md) on a test shared drive before the team relies on it.
- The Windows installer with the bundled SwRefs.exe has not been built or installed on Windows yet.
- Restoring old versions from History, renames and moves with reference rewriting, and the
  SolidWorks add-in.
