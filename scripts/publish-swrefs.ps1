<#
.SYNOPSIS
  Publishes the SwRefs sidecar into src-tauri/binaries/ for Tauri's externalBin.

.DESCRIPTION
  Runs a self-contained, single-file win-x64 `dotnet publish` of sidecar/SwRefs and copies the
  result to src-tauri/binaries/SwRefs-<target triple>.exe, the name Tauri's externalBin expects.
  `pnpm tauri build` runs this on Windows through beforeBuildCommand (src-tauri/tauri.windows.conf.json).

  The real Document Manager reader is compiled only when the SolidWorks interop DLL is found:
  -InteropPath, else SWDM_INTEROP_PATH, else the default SolidWorks install location.
  Without it the script fails, unless -AllowNoInterop is given (dev machines without SolidWorks;
  that build only answers `refs` in fake mode).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/publish-swrefs.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/publish-swrefs.ps1 -AllowNoInterop
#>
param(
    [string]$InteropPath = $env:SWDM_INTEROP_PATH,
    [string]$TargetTriple = "x86_64-pc-windows-msvc",
    [string]$Rid = "win-x64",
    [switch]$AllowNoInterop
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
$project = Join-Path $repo "sidecar/SwRefs/SwRefs.csproj"
$publishDir = Join-Path $repo "sidecar/SwRefs/bin/publish-$Rid"
$binDir = Join-Path $repo "src-tauri/binaries"
$dest = Join-Path $binDir "SwRefs-$TargetTriple.exe"

$defaultInterop = "C:\Program Files\SOLIDWORKS Corp\SOLIDWORKS\api\redist\SolidWorks.Interop.swdocumentmgr.dll"
if (-not $InteropPath -and (Test-Path -LiteralPath $defaultInterop)) {
    $InteropPath = $defaultInterop
}

if ($InteropPath) {
    if (-not (Test-Path -LiteralPath $InteropPath)) {
        throw "SolidWorks Document Manager interop DLL not found at '$InteropPath'."
    }
    Write-Host "Using Document Manager interop: $InteropPath"
} elseif ($AllowNoInterop) {
    Write-Warning "No Document Manager interop DLL; this SwRefs build cannot read real SolidWorks files."
} else {
    throw ("SolidWorks.Interop.swdocumentmgr.dll not found. Set SWDM_INTEROP_PATH or pass -InteropPath " +
        "(see sidecar/README.md), or pass -AllowNoInterop for a fake-only build.")
}

# MSBuild reads SWDM_INTEROP_PATH from the environment; this avoids quoting a path with spaces.
$env:SWDM_INTEROP_PATH = $InteropPath

if (Test-Path -LiteralPath $publishDir) {
    Remove-Item -LiteralPath $publishDir -Recurse -Force
}

& dotnet publish $project -c Release -r $Rid --self-contained true -o $publishDir `
    -p:PublishSingleFile=true -p:EnableCompressionInSingleFile=true -p:DebugType=None
if ($LASTEXITCODE -ne 0) {
    throw "dotnet publish failed with exit code $LASTEXITCODE."
}

$exe = Join-Path $publishDir "SwRefs.exe"
if (-not (Test-Path -LiteralPath $exe)) {
    throw "dotnet publish did not produce $exe."
}

New-Item -ItemType Directory -Force -Path $binDir | Out-Null
Copy-Item -LiteralPath $exe -Destination $dest -Force
Write-Host "Wrote $dest"
