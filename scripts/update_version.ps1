# Prepare and publish a release according to the branch-aware release policy.
param(
    [Parameter(Mandatory = $true, HelpMessage = "X.Y.Z on develop/main, or X.Y.Z-rc on main")]
    [string]$Version
)

$ErrorActionPreference = "Stop"
function Die { param([string]$Message) Write-Host "ERROR: $Message" -ForegroundColor Red; exit 1 }
function Invoke-Checked {
    param([Parameter(Mandatory = $true)][scriptblock]$Command, [Parameter(Mandatory = $true)][string]$FailureMessage)
    & $Command
    if ($LASTEXITCODE -ne 0) { Die $FailureMessage }
}

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
Set-Location $RepoRoot
try {
    $CurrentVersion = ([System.IO.File]::ReadAllText((Join-Path $RepoRoot "package.json")) | ConvertFrom-Json).version
} catch { Die "could not determine current version from package.json: $($_.Exception.Message)" }
if (-not $CurrentVersion) { Die "could not determine current version from package.json" }

Invoke-Checked { node scripts/version-manifests.mjs check $CurrentVersion } "current release version metadata is inconsistent"
$Dirty = git status --porcelain
if ($LASTEXITCODE -ne 0) { Die "failed to inspect git worktree" }
if ($Dirty) { Die "release requires a clean worktree" }

$PlanJson = & node scripts/release-policy.mjs plan $Version
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
try { $Plan = ($PlanJson -join "`n") | ConvertFrom-Json } catch { Die "release policy returned invalid plan JSON" }
$ReleaseVersion = [string]$Plan.releaseVersion
$Tag = [string]$Plan.tag
$Channel = [string]$Plan.channel
$BuildNumber = [int]$Plan.buildNumber
$Branch = [string]$Plan.branch

if ($Channel -eq "stable") {
    Invoke-Checked { node scripts/check-stable-readiness.mjs } "stable qualification is incomplete"
}

if ($CurrentVersion -eq $ReleaseVersion) { Die "release version $ReleaseVersion is already current; release tags are immutable" }
Invoke-Checked { node scripts/version-manifests.mjs assert-newer $ReleaseVersion $CurrentVersion } "release version must advance from $CurrentVersion"

$Remotes = @(git remote)
if ($LASTEXITCODE -ne 0 -or $Remotes.Count -eq 0) { Die "no git remotes configured" }
if ($Remotes -notcontains "origin") { Die "release authority remote 'origin' is required" }

git rev-parse --quiet --verify "refs/tags/$Tag" *> $null
if ($LASTEXITCODE -eq 0) { Die "tag $Tag already exists locally" }
foreach ($Remote in $Remotes) {
    git ls-remote --exit-code --tags $Remote "refs/tags/$Tag" *> $null
    $ProbeStatus = $LASTEXITCODE
    if ($ProbeStatus -eq 0) { Die "tag $Tag already exists on remote $Remote" }
    if ($ProbeStatus -ne 2) { Die "failed to check tag $Tag on remote $Remote" }
}

$Manifests = @("package.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/tauri.conf.json", "src-tauri/Info.plist")
$BaseHead = git rev-parse HEAD
if ($LASTEXITCODE -ne 0) { Die "failed to resolve current HEAD" }
$RollbackManifests = $true
try {
    Write-Host "Release plan: $Version -> $ReleaseVersion ($Channel, native build $BuildNumber)" -ForegroundColor Cyan
    Invoke-Checked { node scripts/version-manifests.mjs set $ReleaseVersion $BuildNumber } "failed to update release manifests"
    Invoke-Checked { cargo check --manifest-path src-tauri/Cargo.toml -q } "cargo failed to synchronize Cargo.lock"
    Invoke-Checked { node scripts/version-manifests.mjs check $ReleaseVersion $BuildNumber } "updated release metadata is inconsistent"
    Invoke-Checked { git diff --check } "release manifests contain whitespace errors"
    Invoke-Checked { git add -- $Manifests } "failed to stage release manifests"
    git diff --cached --quiet
    if ($LASTEXITCODE -eq 0) { Die "version update produced no manifest changes" }
    if ($LASTEXITCODE -ne 1) { Die "failed to inspect staged release manifests" }
    Invoke-Checked { git commit -m "chore(release): $ReleaseVersion" } "failed to create release commit"
    $RollbackManifests = $false
} finally {
    if ($RollbackManifests) { git restore --staged --worktree -- $Manifests *> $null }
}

Invoke-Checked { git tag -a $Tag -m $Tag } "failed to create release tag $Tag"
Write-Host "Publishing $Branch and $Tag atomically to release authority origin..." -ForegroundColor Yellow
& git push --atomic origin $Branch "refs/tags/${Tag}:refs/tags/${Tag}"
if ($LASTEXITCODE -ne 0) {
    Write-Host "Authority push failed; restoring the local pre-release state" -ForegroundColor Yellow
    git tag -d $Tag *> $null
    git reset --hard $BaseHead *> $null
    Die "failed to publish release to origin"
}

$MirrorFailures = @()
foreach ($Remote in $Remotes) {
    if ($Remote -eq "origin") { continue }
    Write-Host "Mirroring $Branch and $Tag -> $Remote" -ForegroundColor Yellow
    & git push --atomic $Remote $Branch "refs/tags/${Tag}:refs/tags/${Tag}"
    if ($LASTEXITCODE -ne 0) {
        $MirrorFailures += $Remote
        Write-Warning "mirror $Remote failed; origin remains the release authority"
    }
}
Write-Host "Done; release $ReleaseVersion submitted to origin" -ForegroundColor Green
if ($MirrorFailures.Count -gt 0) { Write-Warning "mirror sync failed for: $($MirrorFailures -join ', ')" }
