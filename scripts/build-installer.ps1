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

if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY)) {
    $localKeyPath = Join-Path $HOME ".tauri\tomatodesk.key"
    if (-not (Test-Path -LiteralPath $localKeyPath)) {
        throw "Updater signing key is missing. Set TAURI_SIGNING_PRIVATE_KEY or place the private key at $localKeyPath."
    }
    $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content -Raw -LiteralPath $localKeyPath).Trim()
}

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

$installerSignaturePath = "$($installer.FullName).sig"
if (-not (Test-Path -LiteralPath $installerSignaturePath)) {
    throw "Installer was not signed: $installerSignaturePath is missing. Check createUpdaterArtifacts and the signing key."
}

$artifactDirectory = Join-Path $repositoryRoot "artifacts\installers\$version\$commit"
New-Item -ItemType Directory -Force -Path $artifactDirectory | Out-Null
$artifactPath = Join-Path $artifactDirectory $installer.Name
Copy-Item -LiteralPath $installer.FullName -Destination $artifactPath -Force
Get-ChildItem -Path $bundleDirectory -Filter "*.sig" -File |
    ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $artifactDirectory -Force }

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
    updater = [ordered]@{
        artifact = $installer.Name
        signature = "$($installer.Name).sig"
    }
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

& (Join-Path $PSScriptRoot "generate-latest-json.ps1") -ArtifactDirectory $artifactDirectory
