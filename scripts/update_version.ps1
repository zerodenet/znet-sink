# update_version.ps1 — Bump the project version across all manifest files,
# commit, tag, and push to every configured remote.
#
# Usage:   powershell -ExecutionPolicy Bypass -File scripts/update_version.ps1 0.1.0 [-Force]
#          .\scripts\update_version.ps1 0.1.0 -Force
param(
    [Parameter(Mandatory = $true, HelpMessage = "New semver version, e.g. 0.1.0 or 0.1.0-beta.1")]
    [string]$Version,

    [switch]$Force
)

$ErrorActionPreference = "Stop"

# ── helpers ──────────────────────────────────────────────────────────────
function Die {
    Write-Host "ERROR: $args" -ForegroundColor Red
    exit 1
}

function IsValidVersion {
    param([string]$v)
    return $v -match '^\d+\.\d+\.\d+(-[A-Za-z0-9.]+)?$'
}

# ── guard ────────────────────────────────────────────────────────────────
if (-not (IsValidVersion $Version)) {
    Die "invalid version '$Version' — expected semver, e.g. 0.1.0 or 0.1.0-beta.1"
}

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
Set-Location $RepoRoot

# ── discover current version from package.json ───────────────────────────
$packageJson = [System.IO.File]::ReadAllText((Join-Path $RepoRoot "package.json")) | ConvertFrom-Json
$CurrentVersion = $packageJson.version

if (-not $CurrentVersion) {
    Die "could not determine current version from package.json"
}

$retagOnly = $false
if ($CurrentVersion -eq $Version) {
    if (-not $Force) {
        Die "version $Version is already the current version — use -Force to rebuild and republish its tag"
    }
    $dirty = git status --porcelain
    if ($LASTEXITCODE -ne 0) {
        Die "failed to inspect git worktree"
    }
    if ($dirty) {
        Die "-Force requires a clean worktree before tagging the current HEAD"
    }
    $retagOnly = $true
    Write-Host "Version $Version is already current — rebuilding its tag at HEAD" -ForegroundColor Cyan
} else {
    if ($Force) {
        Die "-Force only rebuilds the already-current version tag; omit it for a normal version bump"
    }
    Write-Host "Bumping $CurrentVersion → $Version" -ForegroundColor Cyan
}

# ── update files ─────────────────────────────────────────────────────────
# Use the .NET File API rather than Get-Content/Set-Content: Windows
# PowerShell 5.1 defaults to the system ANSI codepage for text I/O, which
# corrupts non-ASCII bytes (e.g. the Chinese description in package.json), and
# Set-Content -Encoding UTF8 would prepend a BOM that breaks TOML/JSON
# manifests. Set-Location does not sync to .NET's current directory, so read
# and write through absolute paths joined to $RepoRoot.
$files = @(
    "package.json",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/tauri.conf.json"
)

if (-not $retagOnly) {
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)

    foreach ($file in $files) {
        if (-not (Test-Path $file)) {
            Die "expected manifest file not found: $file"
        }
        $fullPath = Join-Path $RepoRoot $file
        $content = [System.IO.File]::ReadAllText($fullPath)
        $escapedCurrent = [regex]::Escape($CurrentVersion)
        # Replace first occurrence of the version string only (Cargo.lock has a
        # top-level entry plus dependency entries; we want the top-level one).
        $newContent = $content -replace $escapedCurrent, $Version
        if ($newContent -eq $content) {
            Die "version '$CurrentVersion' not found in $file"
        }
        [System.IO.File]::WriteAllText($fullPath, $newContent, $utf8NoBom)
        Write-Host "  updated $file"
    }

    # ── commit ───────────────────────────────────────────────────────────
    git add $files
    if ($LASTEXITCODE -ne 0) {
        Die "failed to stage release manifests"
    }

    $commitMsg = "chore(release): bump version to $Version"
    Write-Host ""
    Write-Host "Committing: $commitMsg" -ForegroundColor Green
    git commit -m $commitMsg
    if ($LASTEXITCODE -ne 0) {
        Die "failed to create release commit"
    }
}

$tag = "v$Version"
if ($Force) {
    Write-Host "Force-tagging current HEAD: $tag" -ForegroundColor Green
    git tag -fa $tag -m $tag
} else {
    Write-Host "Tagging: $tag" -ForegroundColor Green
    git tag -a $tag -m $tag
}
if ($LASTEXITCODE -ne 0) {
    Die "failed to create release tag $tag"
}

# ── push to all remotes ──────────────────────────────────────────────────
$remotes = git remote
if (-not $remotes) {
    Die "no git remotes configured — cannot push"
}

$branch = git rev-parse --abbrev-ref HEAD

Write-Host ""
foreach ($remote in $remotes) {
    Write-Host "Pushing $branch → $remote" -ForegroundColor Yellow
    git push $remote $branch
    if ($LASTEXITCODE -ne 0) {
        Die "failed to push $branch to $remote"
    }

    if ($Force) {
        Write-Host "Force-pushing tag $tag → $remote" -ForegroundColor Yellow
        git push --force $remote "refs/tags/${tag}:refs/tags/${tag}"
    } else {
        Write-Host "Pushing tag $tag → $remote" -ForegroundColor Yellow
        git push $remote $tag
    }
    if ($LASTEXITCODE -ne 0) {
        Die "failed to push tag $tag to $remote"
    }
}

Write-Host ""
Write-Host "Done — version $Version pushed to all remotes" -ForegroundColor Green
