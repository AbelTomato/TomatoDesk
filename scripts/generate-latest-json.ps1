[CmdletBinding()]
param(
    [string]$ArtifactDirectory,
    [string]$Repository = "AbelTomato/TomatoDesk",
    [string]$Tag
)

$ErrorActionPreference = "Stop"

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$configPath = Join-Path $repositoryRoot "src-tauri\tauri.conf.json"
$tauriConfig = Get-Content -Raw $configPath | ConvertFrom-Json
$version = $tauriConfig.version
if ([string]::IsNullOrWhiteSpace($Tag)) {
    $Tag = "v$version"
}

if ([string]::IsNullOrWhiteSpace($ArtifactDirectory)) {
    $versionDirectory = Join-Path $repositoryRoot "artifacts\installers\$version"
    if (-not (Test-Path -LiteralPath $versionDirectory)) {
        throw "No artifact directory found for version $version under artifacts/installers."
    }
    $ArtifactDirectory = (Get-ChildItem -LiteralPath $versionDirectory -Directory |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1).FullName
}
if (-not (Test-Path -LiteralPath $ArtifactDirectory)) {
    throw "Artifact directory does not exist: $ArtifactDirectory"
}

$installers = @(Get-ChildItem -LiteralPath $ArtifactDirectory -Filter "*-setup.exe" -File)
if ($installers.Count -ne 1) {
    throw "Expected exactly one NSIS installer in $ArtifactDirectory, found $($installers.Count)."
}
$installer = $installers[0]
$signaturePath = "$($installer.FullName).sig"
if (-not (Test-Path -LiteralPath $signaturePath)) {
    throw "Installer signature is missing: $signaturePath"
}
$signature = (Get-Content -Raw -LiteralPath $signaturePath).Trim()
if ([string]::IsNullOrWhiteSpace($signature)) {
    throw "Installer signature is empty: $signaturePath"
}

$notesPath = Join-Path $repositoryRoot "docs\releases\$Tag.md"
if (Test-Path -LiteralPath $notesPath) {
    $notes = (Get-Content -Raw -LiteralPath $notesPath).Trim()
}
else {
    $notes = "TomatoDesk $Tag 更新。安装包未签名，升级前请确认来源。"
}

$manifest = [ordered]@{
    version   = $version
    notes     = $notes
    pub_date  = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = [ordered]@{
        "windows-x86_64" = [ordered]@{
            signature = $signature
            url       = "https://github.com/$Repository/releases/download/$Tag/$($installer.Name)"
        }
    }
}

$latestPath = Join-Path $ArtifactDirectory "latest.json"
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText(
    $latestPath,
    ($manifest | ConvertTo-Json -Depth 6) + "`n",
    $utf8NoBom
)

Write-Host "Updater manifest: $latestPath"
Write-Host "Updater artifact: $($installer.Name)"
