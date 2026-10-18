# SwRefs sidecar

A small .NET 8 console app that cadsync's Rust core starts as a child process to find out which
files a SolidWorks part, assembly or drawing references. It uses the SolidWorks Document Manager
API, so SolidWorks does not need to be running.

- `SwRefs/` – the app
- `SwRefs.Tests/` – xUnit tests (protocol, fake reader, end-to-end process run)
- `fixtures/sample-refs.json` – sample fixture for the fake reader

## Protocol

One JSON object per line on stdin; exactly one JSON line per request on stdout, in order.
Logs go to stderr only. The process exits when stdin closes. Blank lines are ignored.

```
→ {"id":"1","op":"ping"}
← {"id":"1","ok":true,"version":"0.1.0"}

→ {"id":"2","op":"refs","path":"C:\\Vault\\Robot.SLDASM"}
← {"id":"2","ok":true,"refs":[{"path":"C:\\Vault\\Arm.SLDASM","fileName":"Arm.SLDASM","exists":true,"isVirtual":false}]}

← {"id":"3","ok":false,"error":"file not found: ...","code":"NOT_FOUND"}
```

`refs` returns direct references only, deduplicated by path (case-insensitively). `path` is the
path stored in the file, and `exists` says whether a file is at that path on this machine.
Virtual components are included with `isVirtual: true`; they live inside the assembly, so ignore
`exists` for them.

| code | when |
| --- | --- |
| `BAD_REQUEST` | invalid JSON, not an object, missing/non-string `id` (response `id` is `null`), missing `op`, unknown `op`, missing or relative `path` |
| `UNSUPPORTED` | not `.sldasm`/`.sldprt`/`.slddrw`, or this build has no Document Manager support |
| `NOT_FOUND` | the file does not exist |
| `NO_LICENSE` | `CADSYNC_SWDM_KEY` is missing or rejected |
| `OPEN_FAILED` | the file exists but could not be read (damaged, newer SolidWorks version, …) |

## Environment variables

| variable | purpose |
| --- | --- |
| `CADSYNC_SWDM_KEY` | Document Manager license key. Required for the real reader. |
| `CADSYNC_SWREFS_FAKE` | Path to a JSON fixture. When set, the fake reader answers instead of the Document Manager. |

### The license key

The Document Manager needs a license key, which SolidWorks subscription holders (FRC teams get
SolidWorks through the sponsorship program) can request through the SolidWorks Customer Portal
(API support / Document Manager key request) or their reseller. **Never commit the key or put it in source.** Give it to the
sidecar through the environment, e.g. from the Rust side when spawning the process, or for a
manual test:

```powershell
$env:CADSYNC_SWDM_KEY = "<your key>"
```

The sidecar never logs the key.

## Building on Windows with the real Document Manager

1. Install the .NET 8 SDK.
2. Find the interop DLL on a machine with SolidWorks installed. It is usually at
   `C:\Program Files\SOLIDWORKS Corp\SOLIDWORKS\api\redist\SolidWorks.Interop.swdocumentmgr.dll`
   (the Document Manager API also ships it if installed separately). It is not redistributable
   on its own, so do not commit it.
3. Build or publish with `SWDM_INTEROP_PATH` pointing at it:

```powershell
dotnet publish sidecar/SwRefs -c Release -r win-x64 --self-contained -p:PublishSingleFile=true -p:SWDM_INTEROP_PATH="C:\Program Files\SOLIDWORKS Corp\SOLIDWORKS\api\redist\SolidWorks.Interop.swdocumentmgr.dll"
```

`SWDM_INTEROP_PATH` can also be set as an environment variable. When it is set, the build defines
`SWDM` and compiles `Readers/SwdmReferenceReader.cs`; the build fails if the path does not exist.
Without it, the app still builds (as on macOS) but `refs` answers `UNSUPPORTED` unless the fake
reader is selected.

The Document Manager is a 64-bit COM server registered by the SolidWorks install, so publish for
`win-x64`. For the Tauri installer, `scripts/publish-swrefs.ps1` (`pnpm sidecar`) runs this
publish and copies the result to `src-tauri/binaries/SwRefs-x86_64-pc-windows-msvc.exe`; see
"Build the Windows installer" in the top-level README.

## Building and testing anywhere else (fake mode)

```bash
dotnet test sidecar/SwRefs.Tests
```

Off Windows the projects target `net8.0` (on Windows, `net8.0-windows`) and roll forward to a
newer runtime if .NET 8 is not installed. To run the sidecar against the sample fixture:

```bash
CADSYNC_SWREFS_FAKE=sidecar/fixtures/sample-refs.json dotnet run --project sidecar/SwRefs
```

Fixture format (keys match case-insensitively; a ref without `exists` is checked on disk):

```json
{
  "C:\\Vault\\Robot.SLDASM": [
    { "path": "C:\\Vault\\Arm.SLDASM", "exists": true },
    "C:\\Vault\\Base.SLDPRT",
    { "path": "Spacer^Robot.sldprt", "isVirtual": true }
  ],
  "C:\\Vault\\Corrupt.SLDASM": { "error": "OPEN_FAILED", "message": "file is damaged" }
}
```

Paths not in the fixture return `NOT_FOUND`. If the fixture cannot be loaded, the sidecar still
answers `ping`, and every `refs` returns `OPEN_FAILED` with the reason.
