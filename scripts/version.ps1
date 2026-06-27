param(
    [Parameter(Position = 0)]
    [ValidateSet("patch", "minor", "major")]
    [string]$Bump = "patch",

    [string]$Version,

    [switch]$Push
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-Git {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    & git @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "git $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
}

function Get-WorkspaceVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Lines
    )

    $inWorkspacePackage = $false
    for ($index = 0; $index -lt $Lines.Count; $index++) {
        $line = $Lines[$index]
        if ($line -eq "[workspace.package]") {
            $inWorkspacePackage = $true
            continue
        }
        if ($inWorkspacePackage -and $line.StartsWith("[")) {
            break
        }
        if ($inWorkspacePackage -and $line -match '^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"\s*$') {
            return [PSCustomObject]@{
                Index   = $index
                Version = $Matches["version"]
            }
        }
    }

    throw "Could not find [workspace.package] version in Cargo.toml"
}

function Get-BumpedVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Current,

        [Parameter(Mandatory = $true)]
        [string]$Part
    )

    $parts = $Current.Split(".") | ForEach-Object { [int]$_ }
    switch ($Part) {
        "major" { return "$($parts[0] + 1).0.0" }
        "minor" { return "$($parts[0]).$($parts[1] + 1).0" }
        "patch" { return "$($parts[0]).$($parts[1]).$($parts[2] + 1)" }
    }
}

$repoRoot = (& git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0) {
    throw "This script must be run inside a git repository."
}
Set-Location $repoRoot

$dirty = & git status --porcelain
if ($dirty) {
    throw "Working tree is not clean. Commit or stash changes before bumping the version."
}

$cargoToml = Join-Path $repoRoot "Cargo.toml"
$lines = [string[]](Get-Content -Path $cargoToml)
$versionInfo = Get-WorkspaceVersion -Lines $lines
$currentVersion = $versionInfo.Version

if ($Version) {
    if ($Version -notmatch '^\d+\.\d+\.\d+$') {
        throw "Version must use MAJOR.MINOR.PATCH format, for example 0.2.0."
    }
    $nextVersion = $Version
}
else {
    $nextVersion = Get-BumpedVersion -Current $currentVersion -Part $Bump
}

if ($nextVersion -eq $currentVersion) {
    throw "Version is already $nextVersion."
}

$tag = "v$nextVersion"
$existingTag = & git tag --list $tag
if ($existingTag) {
    throw "Tag $tag already exists."
}

$lines[$versionInfo.Index] = "version = `"$nextVersion`""
Set-Content -Path $cargoToml -Value $lines -Encoding UTF8

cargo update -w
if ($LASTEXITCODE -ne 0) {
    throw "cargo update -w failed."
}

Invoke-Git -Arguments @("add", "Cargo.toml", "Cargo.lock")
Invoke-Git -Arguments @("commit", "-m", "chore(release): $tag")
Invoke-Git -Arguments @("tag", "-a", $tag, "-m", $tag)

if ($Push) {
    $branch = (& git branch --show-current).Trim()
    if (-not $branch) {
        throw "Cannot push from a detached HEAD."
    }
    Invoke-Git -Arguments @("push", "origin", $branch)
    Invoke-Git -Arguments @("push", "origin", $tag)
    Write-Output "Released $tag and pushed branch $branch."
}
else {
    Write-Output "Created release commit and tag $tag locally. Push with:"
    Write-Output "  git push origin HEAD"
    Write-Output "  git push origin $tag"
}
