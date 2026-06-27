$ErrorActionPreference = "Stop"

$Repo = if ($env:AIKIT_REPO) { $env:AIKIT_REPO } else { "aikit-rs/aikit" }
$Version = if ($env:AIKIT_VERSION) { $env:AIKIT_VERSION } else { "latest" }
$BinDir = if ($env:AIKIT_BIN_DIR) { $env:AIKIT_BIN_DIR } else { Join-Path $HOME ".aikit\bin" }

$Architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($Architecture -ne [System.Runtime.InteropServices.Architecture]::X64) {
    throw "Unsupported Windows architecture. aikit currently supports x64 Windows only."
}

$Archive = "aikit-x86_64-pc-windows-msvc.zip"

if ($Version -eq "latest") {
    $Url = "https://github.com/$Repo/releases/latest/download/$Archive"
} else {
    $Url = "https://github.com/$Repo/releases/download/$Version/$Archive"
}

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $TempDir | Out-Null

try {
    $ArchivePath = Join-Path $TempDir $Archive
    Invoke-WebRequest -Uri $Url -OutFile $ArchivePath

    Expand-Archive -Path $ArchivePath -DestinationPath $TempDir -Force

    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    $InstallPath = Join-Path $BinDir "aikit.exe"
    Copy-Item (Join-Path $TempDir "aikit.exe") $InstallPath -Force

    Write-Host "Installed aikit to $InstallPath"

    $PathEntries = ($env:PATH -split [System.IO.Path]::PathSeparator) | Where-Object { $_ }
    if ($PathEntries -notcontains $BinDir) {
        Write-Host "Add $BinDir to your PATH to run aikit from any directory."
    }
} finally {
    Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
