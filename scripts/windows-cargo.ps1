[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet("check", "build", "clippy", "test", "fmt")]
    [string]$Command = "check",

    [Parameter(Position = 1, ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs
)

$ErrorActionPreference = "Stop"

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repositoryRoot "src-tauri\Cargo.toml"
$targetDirectory = $env:CARGO_TARGET_DIR

if ([string]::IsNullOrWhiteSpace($targetDirectory)) {
    $targetDirectory = Join-Path $env:TEMP "TomatoDesk\cargo-target"
}

New-Item -ItemType Directory -Force -Path $targetDirectory | Out-Null
$env:CARGO_TARGET_DIR = $targetDirectory
$env:CARGO_INCREMENTAL = "0"

Write-Host "Using Windows Cargo: $(Get-Command cargo | Select-Object -ExpandProperty Source)"
Write-Host "Cargo target directory: $targetDirectory"

& cargo $Command --manifest-path $manifestPath @CargoArgs
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
