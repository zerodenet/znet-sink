# Bump the release version, commit, tag, and push every configured remote.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts/update_version.ps1 0.1.0-rc.1
#        .\scripts\update_version.ps1 0.1.0 -Force
param(
    [Parameter(Mandatory = $true, HelpMessage = "New SemVer, e.g. 0.1.0 or 0.1.0-rc.1")]
    [string]$Version,

    [switch]$Force
)

$ErrorActionPreference = "Stop"

function Die {
    param([string]$Message)
    Write-Host "ERROR: $Message" -ForegroundColor Red
    exit 1
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [scriptblock]$Command,
        [Parameter(Mandatory = $true)]
        [string]$FailureMessage
    )

    & $Command
    if ($LASTEXITCODE -ne 0) {
        Die $FailureMessage
    }
}

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
Set-Location $RepoRoot

Invoke-Checked { node scripts/version-manifests.mjs validate $Version } "invalid release version '$Version'"

try {
    $PackageJson = [System.IO.File]::ReadAllText((Join-Path $RepoRoot "package.json")) | ConvertFrom-Json
    $CurrentVersion = $PackageJson.version
} catch {
    Die "could not determine current version from package.json: $($_.Exception.Message)"
}
if (-not $CurrentVersion) {
    Die "could not determine current version from package.json"
}

# Refuse to release from a partially synchronized or dirty tree. This prevents
# unrelated local work from being swept into the generated release commit.
Invoke-Checked { node scripts/version-manifests.mjs check $CurrentVersion } "current release version metadata is inconsistent"
$Dirty = git status --porcelain
if ($LASTEXITCODE -ne 0) {
    Die "failed to inspect git worktree"
}
if ($Dirty) {
    Die "release requires a clean worktree"
}

$Branch = git symbolic-ref --quiet --short HEAD
if ($LASTEXITCODE -ne 0 -or -not $Branch) {
    Die "release requires a named branch (detached HEAD is not supported)"
}

$Remotes = @(git remote)
if ($LASTEXITCODE -ne 0 -or $Remotes.Count -eq 0) {
    Die "no git remotes configured; cannot push"
}

$RetagOnly = $false
if ($CurrentVersion -eq $Version) {
    if (-not $Force) {
        Die "version $Version is already current; use -Force to rebuild and republish its tag"
    }
    $RetagOnly = $true
    Write-Host "Version $Version is already current; rebuilding its tag at HEAD" -ForegroundColor Cyan
} else {
    if ($Force) {
        Die "-Force only rebuilds the already-current version tag; omit it for a normal version bump"
    }
    Invoke-Checked { node scripts/version-manifests.mjs assert-newer $Version $CurrentVersion } "release version must be greater than $CurrentVersion"
    Write-Host "Bumping $CurrentVersion -> $Version" -ForegroundColor Cyan
}

$Tag = "v$Version"

# Probe every tag before changing manifests, so a collision cannot leave a
# release commit behind without a matching tag.
if (-not $Force) {
    git rev-parse --quiet --verify "refs/tags/$Tag" *> $null
    if ($LASTEXITCODE -eq 0) {
        Die "tag $Tag already exists locally"
    }

    foreach ($Remote in $Remotes) {
        git ls-remote --exit-code --tags $Remote "refs/tags/$Tag" *> $null
        $ProbeStatus = $LASTEXITCODE
        if ($ProbeStatus -eq 0) {
            Die "tag $Tag already exists on remote $Remote"
        }
        if ($ProbeStatus -ne 2) {
            Die "failed to check tag $Tag on remote $Remote"
        }
    }
}

$Manifests = @(
    "package.json",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/tauri.conf.json",
    "src-tauri/Info.plist"
)

if (-not $RetagOnly) {
    $RollbackManifests = $true
    try {
        Invoke-Checked { node scripts/version-manifests.mjs set $Version } "failed to update release manifests"

        # Cargo owns Cargo.lock. Replacing matching version strings directly can
        # corrupt third-party dependency entries that happen to share a version.
        Write-Host "Syncing src-tauri/Cargo.lock via cargo..." -ForegroundColor Cyan
        Invoke-Checked { cargo check --manifest-path src-tauri/Cargo.toml -q } "cargo failed to synchronize Cargo.lock"
        Invoke-Checked { node scripts/version-manifests.mjs check $Version } "updated release version metadata is inconsistent"
        Invoke-Checked { git diff --check } "release manifests contain whitespace errors"

        Invoke-Checked { git add -- $Manifests } "failed to stage release manifests"
        git diff --cached --quiet
        if ($LASTEXITCODE -eq 0) {
            Die "version update produced no manifest changes"
        }
        if ($LASTEXITCODE -ne 1) {
            Die "failed to inspect staged release manifests"
        }

        $CommitMessage = "chore(release): bump version to $Version"
        Write-Host ""
        Write-Host "Committing: $CommitMessage" -ForegroundColor Green
        Invoke-Checked { git commit -m $CommitMessage } "failed to create release commit"
        $RollbackManifests = $false
    } finally {
        if ($RollbackManifests) {
            Write-Host "Release preparation failed; restoring version manifests" -ForegroundColor Yellow
            git restore --staged --worktree -- $Manifests *> $null
        }
    }
}

if ($Force) {
    Write-Host "Force-tagging current HEAD: $Tag" -ForegroundColor Green
    Invoke-Checked { git tag -fa $Tag -m $Tag } "failed to recreate release tag $Tag"
} else {
    Write-Host "Tagging: $Tag" -ForegroundColor Green
    Invoke-Checked { git tag -a $Tag -m $Tag } "failed to create release tag $Tag"
}

Write-Host ""
foreach ($Remote in $Remotes) {
    Write-Host "Pushing $Branch -> $Remote" -ForegroundColor Yellow
    Invoke-Checked { git push $Remote $Branch } "failed to push $Branch to $Remote"

    if ($Force) {
        Write-Host "Force-pushing tag $Tag -> $Remote" -ForegroundColor Yellow
        Invoke-Checked { git push --force $Remote "refs/tags/${Tag}:refs/tags/${Tag}" } "failed to force-push tag $Tag to $Remote"
    } else {
        Write-Host "Pushing tag $Tag -> $Remote" -ForegroundColor Yellow
        Invoke-Checked { git push $Remote "refs/tags/${Tag}:refs/tags/${Tag}" } "failed to push tag $Tag to $Remote"
    }
}

Write-Host ""
Write-Host "Done; release $Version pushed to all remotes" -ForegroundColor Green
