[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$configPath = Join-Path $repositoryRoot "src-tauri\tauri.conf.json"
$tauriConfig = Get-Content -Raw $configPath | ConvertFrom-Json
$version = $tauriConfig.version
$commit = (& git -C $repositoryRoot rev-parse --short HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($commit)) {
    throw "Unable to read the current Git commit."
}

$status = & git -C $repositoryRoot status --porcelain=v1 --untracked-files=all
$workingTreeDirty = -not [string]::IsNullOrWhiteSpace(($status -join "`n"))
$targetDirectory = Join-Path $env:TEMP "TomatoDesk\cargo-target"
$env:CARGO_TARGET_DIR = $targetDirectory
$env:CARGO_INCREMENTAL = "0"

Push-Location $repositoryRoot
try {
    & pnpm tauri build --bundles nsis
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri NSIS build failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

$bundleDirectory = Join-Path $targetDirectory "release\bundle\nsis"
$installer = Get-ChildItem -Path $bundleDirectory -Filter "*-setup.exe" -File |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1
if (-not $installer) {
    throw "No NSIS setup executable was produced in $bundleDirectory."
}

$artifactDirectory = Join-Path $repositoryRoot "artifacts\installers\$version\$commit"
New-Item -ItemType Directory -Force -Path $artifactDirectory | Out-Null
$artifactPath = Join-Path $artifactDirectory $installer.Name
Copy-Item -LiteralPath $installer.FullName -Destination $artifactPath -Force

$hash = (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash.ToLowerInvariant()
$checksumText = "$hash  $($installer.Name)`n"
$manifest = [ordered]@{
    product = $tauriConfig.productName
    version = $version
    sourceCommit = $commit
    workingTreeDirty = $workingTreeDirty
    buildTimeUtc = [DateTime]::UtcNow.ToString("o")
    installer = $installer.Name
    sha256 = $hash
}
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText("$artifactPath.sha256", $checksumText, $utf8NoBom)
[System.IO.File]::WriteAllText(
    (Join-Path $artifactDirectory "manifest.json"),
    ($manifest | ConvertTo-Json -Depth 4) + "`n",
    $utf8NoBom
)

Write-Host "Installer: $artifactPath"
Write-Host "SHA-256: $hash"
Write-Host "Manifest: $(Join-Path $artifactDirectory 'manifest.json')"
