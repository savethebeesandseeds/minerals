# Rewrites every local ref to remove private/runtime artifacts. This script
# deliberately never pushes; publishing rewritten history is a separate,
# manual decision after review.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $ConfirmHistoryRewrite
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($ConfirmHistoryRewrite -cne "REWRITE-MAIN-HISTORY") {
    throw "Refusing to rewrite history. Pass -ConfirmHistoryRewrite REWRITE-MAIN-HISTORY exactly."
}

$repositoryCandidate = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$rootOutput = & git -C $repositoryCandidate -c "safe.directory=$repositoryCandidate" rev-parse --show-toplevel 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "Could not identify the repository root: $($rootOutput -join [Environment]::NewLine)"
}
$repositoryRoot = (Resolve-Path (($rootOutput | Select-Object -First 1).ToString().Trim())).Path
if ($repositoryRoot -ne $repositoryCandidate) {
    throw "This script must be run from its own Waajacu repository checkout."
}

function Invoke-RepositoryGit {
    param(
        [Parameter(Mandatory = $true)]
        [string[]] $Arguments
    )

    $output = & git -C $repositoryRoot -c "safe.directory=$repositoryRoot" @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "git $($Arguments -join ' ') failed: $($output -join [Environment]::NewLine)"
    }
    return @($output)
}

$branch = (Invoke-RepositoryGit -Arguments @("symbolic-ref", "--quiet", "--short", "HEAD") | Select-Object -First 1).ToString().Trim()
if ($branch -cne "main") {
    throw "History cleanup is permitted only while the main branch is checked out (found '$branch')."
}

$localBranches = @(
    Invoke-RepositoryGit -Arguments @(
        "for-each-ref",
        "--format=%(refname:short)",
        "refs/heads"
    ) | ForEach-Object { $_.ToString().Trim() } | Where-Object { $_ }
)
if ($localBranches.Count -ne 1 -or $localBranches[0] -cne "main") {
    throw "History cleanup requires main to be the only local branch. Found: $($localBranches -join ', ')"
}

$status = @(Invoke-RepositoryGit -Arguments @("status", "--porcelain=v1", "--untracked-files=all"))
if ($status.Count -ne 0) {
    throw "History cleanup requires a completely clean worktree and index. Commit or safely store all changes first."
}

# `git filter-repo` is intentionally required instead of Git's error-prone
# filter-branch implementation.
Invoke-RepositoryGit -Arguments @("filter-repo", "--version") | Out-Null

$head = (Invoke-RepositoryGit -Arguments @("rev-parse", "--short=12", "HEAD") | Select-Object -First 1).ToString().Trim()
$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssZ")
$backupDirectory = Join-Path $repositoryRoot ".history-backup"
New-Item -ItemType Directory -Path $backupDirectory -Force | Out-Null
$backupBundle = Join-Path $backupDirectory "before-history-scrub-$timestamp-$head.bundle"
$backupRefs = Join-Path $backupDirectory "before-history-scrub-$timestamp-$head-refs.txt"
$backupRemotes = Join-Path $backupDirectory "before-history-scrub-$timestamp-$head-remotes.txt"

Invoke-RepositoryGit -Arguments @("bundle", "create", $backupBundle, "--all") | Out-Null
Invoke-RepositoryGit -Arguments @("bundle", "verify", $backupBundle) | Out-Null
Invoke-RepositoryGit -Arguments @("show-ref", "--head", "--dereference") |
    Set-Content -LiteralPath $backupRefs -Encoding utf8NoBOM
Invoke-RepositoryGit -Arguments @("remote", "-v") |
    Set-Content -LiteralPath $backupRemotes -Encoding utf8NoBOM

$filterArguments = @(
    "filter-repo",
    "--force",
    "--invert-paths",
    "--path", ".archives/",
    "--path", ".cloudflared/",
    "--path", ".history-backup/",
    "--path", ".tmp/",
    "--path", "tmp/",
    "--path", "target/",
    "--path", "map-wasm/target/",
    "--path", "dist/",
    "--path", "public-dist/",
    "--path", "public-releases/",
    "--path", "data/backups/",
    "--path", "data/images/",
    "--path", "data/reports/",
    "--path", "data/.report-work/",
    "--path", ".env.local",
    "--path", ".env.production",
    "--path", ".env.staging",
    "--path", ".env.private",
    "--path", "waajacu-public-catalog-pages.tar.gz",
    "--path-glob", ".env.*.local",
    "--path-glob", ".env.production.*",
    "--path-glob", ".env.staging.*",
    "--path-glob", ".env.private.*",
    "--path-glob", "data/*.db",
    "--path-glob", "data/*.db-*",
    "--path-glob", "data/*.db-journal",
    "--path-glob", "data/*.sqlite*",
    "--path-glob", "data/.registry-ready-*.tmp",
    "--path-glob", "data/minerals/*/report.*",
    "--path-glob", "data/minerals/**/report.*",
    "--path-glob", "**/__pycache__/**",
    "--path-glob", "catalog-[0-9a-f]*.sqlite3",
    "--path-glob", "**/catalog-[0-9a-f]*.sqlite3",
    "--path-glob", "**/catalog-[0-9a-f]*.sqlite3.br",
    "--path-glob", "**/catalog-[0-9a-f]*.sqlite3.gz",
    "--path-glob", "*.p12",
    "--path-glob", "*.pfx",
    "--path-glob", "**/*.p12",
    "--path-glob", "**/*.pfx"
)

Invoke-RepositoryGit -Arguments $filterArguments | Out-Null

$branchAfter = (Invoke-RepositoryGit -Arguments @("symbolic-ref", "--quiet", "--short", "HEAD") | Select-Object -First 1).ToString().Trim()
if ($branchAfter -cne "main") {
    throw "Unexpected branch after rewrite: '$branchAfter'. Restore from $backupBundle."
}
$statusAfter = @(Invoke-RepositoryGit -Arguments @("status", "--porcelain=v1", "--untracked-files=all"))
if ($statusAfter.Count -ne 0) {
    throw "The rewritten checkout is not clean. Restore from $backupBundle and investigate."
}

$checker = Join-Path $repositoryRoot "tools/check-public-boundary.py"
$python3 = Get-Command python3 -ErrorAction SilentlyContinue
$python = Get-Command python -ErrorAction SilentlyContinue
$py = Get-Command py -ErrorAction SilentlyContinue
if ($null -ne $python3) {
    & $python3.Source $checker --history
} elseif ($null -ne $python) {
    & $python.Source $checker --history
} elseif ($null -ne $py) {
    & $py.Source -3 $checker --history
} else {
    throw "Python is required to run the post-rewrite public-boundary validation. Restore from $backupBundle if needed."
}
if ($LASTEXITCODE -ne 0) {
    throw "Post-rewrite public-boundary validation failed. Restore from $backupBundle and investigate."
}

Invoke-RepositoryGit -Arguments @("bundle", "verify", $backupBundle) | Out-Null

Write-Host "History rewrite and validation completed locally."
Write-Host "Sensitive backup bundle: $backupBundle"
Write-Host "Original refs: $backupRefs"
Write-Host "Original remotes: $backupRemotes"
Write-Warning "This script did not push anything. git-filter-repo may remove the origin remote as a safety measure; review the saved remote list and rewritten history before reconnecting or force-pushing."
