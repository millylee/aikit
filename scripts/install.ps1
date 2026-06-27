$ErrorActionPreference = "Stop"

$Repo = if ($env:AIKIT_REPO) { $env:AIKIT_REPO } else { "millylee/aikit" }
$Version = if ($env:AIKIT_VERSION) { $env:AIKIT_VERSION } else { "latest" }
$BinDir = if ($env:AIKIT_BIN_DIR) { $env:AIKIT_BIN_DIR } else { Join-Path $HOME ".local\bin" }

$Architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($Architecture -ne [System.Runtime.InteropServices.Architecture]::X64) {
    throw "Unsupported Windows architecture. aikit currently supports x64 Windows only."
}

$Archive = "aikit-x86_64-pc-windows-msvc.zip"

function Test-PathEntry {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if (-not $PathValue) {
        return $false
    }

    foreach ($PathEntry in ($PathValue -split [System.IO.Path]::PathSeparator)) {
        if ([string]::Equals($PathEntry, $Entry, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }

    return $false
}

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

    $ResolvedBinDir = (New-Item -ItemType Directory -Path $BinDir -Force).FullName
    $BinDir = $ResolvedBinDir
    $InstallPath = Join-Path $BinDir "aikit.exe"
    Copy-Item (Join-Path $TempDir "aikit.exe") $InstallPath -Force

    Write-Host "Installed aikit to $InstallPath"

    if (-not (Test-PathEntry -PathValue $env:PATH -Entry $BinDir)) {
        $env:PATH = "$BinDir$([System.IO.Path]::PathSeparator)$env:PATH"
        Write-Host "Added $BinDir to PATH for this PowerShell session."
    }

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not (Test-PathEntry -PathValue $UserPath -Entry $BinDir)) {
        $NewUserPath = if ($UserPath) {
            "$UserPath$([System.IO.Path]::PathSeparator)$BinDir"
        } else {
            $BinDir
        }
        [Environment]::SetEnvironmentVariable("Path", $NewUserPath, "User")
        Write-Host "Added $BinDir to your user PATH. Restart other terminals to pick it up."
    }
} finally {
    Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
