param(
    [Parameter(Mandatory = $true)][string]$TargetTriple,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$ArchiveBase
)

# Package a Fleen release archive for one Windows target triple.
#
# Usage: scripts/package-release.ps1 -TargetTriple <triple> -Version <version> -ArchiveBase <base>

$ErrorActionPreference = 'Stop'

$RepoRoot = (git rev-parse --show-toplevel).Trim()
$StdVersion = (Get-Content -Raw (Join-Path $RepoRoot 'std\VERSION')).Trim()
if ($StdVersion -ne $Version) {
    Write-Error "std/VERSION ($StdVersion) does not match version ($Version)"
    exit 1
}

$Staging = Join-Path ([System.IO.Path]::GetTempPath()) ("fleen-package-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Staging | Out-Null

try {
    $Binaries = @('fleen-vm.exe', 'fleen-compiler.exe', 'fleen-verify.exe', 'flint-cli.exe')
    foreach ($Bin in $Binaries) {
        $Src = Join-Path $RepoRoot "target\$TargetTriple\release\$Bin"
        if (-not (Test-Path $Src)) {
            Write-Error "missing binary: $Src"
            exit 1
        }
        Copy-Item -Path $Src -Destination $Staging
    }

    Copy-Item -Recurse -Path (Join-Path $RepoRoot 'std') -Destination (Join-Path $Staging 'std')

    $Readme = @'
# Fleen $Version — $TargetTriple

This archive contains the Fleen $Version toolchain for $TargetTriple.

## Contents

- `fleen-vm.exe` — Fleen virtual machine / one-stop driver (compile, verify, execute a .fln file)
- `fleen-compiler.exe` — Fleen compiler (lex, parse, resolve, typeck, lower, codegen)
- `fleen-verify.exe` — Fleen static verification tooling
- `flint-cli.exe` — Flint package manager (flint new/build/run/test/check)
- `std\` — the Fleen standard library (must match the compiler version)
- `README.md` — this file

## Install

1. Add the four binaries to your PATH (copy the extracted directory onto PATH).
2. Copy `std\` to `$env:FLINT_HOME\std\$Version\`:
   `New-Item -ItemType Directory -Force "$env:FLINT_HOME\std\$Version" | Out-Null`
   `Copy-Item -Recurse ".\std\*" "$env:FLINT_HOME\std\$Version\"`.
   The default `$env:FLINT_HOME` is `~\.flint`.
   `std\VERSION` must equal `$Version`; a mismatch is a hard error.
'@
    $Readme = $Readme.Replace('$Version', $Version).Replace('$TargetTriple', $TargetTriple)
    Set-Content -Path (Join-Path $Staging 'README.md') -Value $Readme -Encoding UTF8

    $ArtifactsDir = Join-Path $RepoRoot 'target\release-artifacts'
    New-Item -ItemType Directory -Force -Path $ArtifactsDir | Out-Null
    $ZipPath = Join-Path $ArtifactsDir "$ArchiveBase.zip"
    Compress-Archive -Path (Join-Path $Staging '*') -DestinationPath $ZipPath -Force
    Write-Output $ZipPath
} finally {
    Remove-Item -Recurse -Force $Staging -ErrorAction SilentlyContinue
}
