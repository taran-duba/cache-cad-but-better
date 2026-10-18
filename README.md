# cadsync

SolidWorks + Google Drive file sharing for FRC teams, built to fix what went wrong with CacheCAD:

| Problem | What cadsync does |
| --- | --- |
| Slow loading | A local SQLite index of the whole shared drive, updated incrementally from Drive's change feed. Browsing never waits on the network. |
| Crashes | Drive names (`/`, emoji, `CON`, duplicates, trailing dots) are mapped to safe Windows names; sync runs in the background with a crash-safe journal. |
| Overwrites and conflicts | Check-out locks (Drive content restriction plus lock properties), and read-only local files for anything you haven't checked out. |
| Broken mates | Atomic check-ins: an assembly and its changed parts arrive together or not at all. References are read with the SolidWorks Document Manager, and check-ins are blocked for missing files, files outside the vault, and duplicate names. |

## Layout

- `src-tauri/`: Rust core (Tauri v2)
  - `drive/`: Google Drive client, OAuth, and an in-memory fake for tests and demo mode
  - `index/`: SQLite index and deterministic path mapping
  - `sync/`: crawl and changes, pull planner, locks, atomic push
  - `refs.rs`: SwRefs client and pre-check-in validation
  - `app/`: Tauri commands, config, background poller
- `src/`: React UI
- `sidecar/`: SwRefs (C#), which reads SolidWorks references with the Document Manager API
- `docs/SETUP.md`: one-time team setup (Google OAuth client, shared drive, license key)
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

Build on a Windows x64 machine with SolidWorks installed (the SwRefs build needs its Document
Manager interop DLL).

1. Install Node.js with pnpm, Rust (MSVC toolchain, the `rustup` default on Windows), the
   Microsoft C++ Build Tools and WebView2 ([Tauri prerequisites](https://tauri.app/start/prerequisites/)),
   and the .NET 8 SDK.
2. If SolidWorks is not in the default location, point `SWDM_INTEROP_PATH` at
   `SolidWorks.Interop.swdocumentmgr.dll` (see [sidecar/README.md](sidecar/README.md)):

   ```powershell
   $env:SWDM_INTEROP_PATH = "D:\SOLIDWORKS\api\redist\SolidWorks.Interop.swdocumentmgr.dll"
   ```

3. Build:

   ```powershell
   pnpm install
   pnpm tauri build
   ```

The installers are written to `src-tauri/target/release/bundle/` (`nsis/*-setup.exe` and
`msi/*.msi`).

On Windows, `pnpm tauri build` first runs `pnpm sidecar` (set in `src-tauri/tauri.windows.conf.json`),
which runs [scripts/publish-swrefs.ps1](scripts/publish-swrefs.ps1). That publishes SwRefs as a
self-contained single-file `win-x64` exe to `src-tauri/binaries/SwRefs-x86_64-pc-windows-msvc.exe`,
and Tauri bundles it (`externalBin`) as `SwRefs.exe` next to `cadsync.exe`, which is where the app
looks for it. It fails if the interop DLL can't be found, so an installer can't ship without the
real reader. The Document Manager license key is not bundled; each team sets it up as described
in `docs/SETUP.md`.

Every Rust build on Windows (`pnpm tauri dev`, `cargo test`) needs that file to exist, so on a
Windows dev machine run `pnpm sidecar` once first. Without SolidWorks, use
`pnpm sidecar -AllowNoInterop`. That build only answers `refs` in fake mode.

macOS and Linux builds don't bundle SwRefs. The Windows-only settings are in
`tauri.windows.conf.json`.

## Not done yet

- Not yet run against real Google Drive or SolidWorks. Work through
  [docs/TESTING.md](docs/TESTING.md) on a test shared drive before the team relies on it.
- The Windows installer with the bundled SwRefs.exe has not been built or installed on Windows yet.
- Restoring old versions from History, renames and moves with reference rewriting, and the
  SolidWorks add-in.
