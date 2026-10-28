# Builds the Flowtime AE plugin and the CLI into dist\.
# Toolchain: system cargo, or a portable windows-gnu one (cargo\, rustup\, gcc\w64devkit\)
# taken from $env:PETAL_TOOLCHAIN or ..\..\.tools\rust-daw if that folder exists.
$ErrorActionPreference = 'Stop'

$tools = if ($env:PETAL_TOOLCHAIN) { $env:PETAL_TOOLCHAIN } else { Join-Path $PSScriptRoot '..\..\.tools\rust-daw' }
if (Test-Path "$tools\cargo\bin\cargo.exe") {
    $env:CARGO_HOME = (Resolve-Path "$tools\cargo").Path
    $env:RUSTUP_HOME = (Resolve-Path "$tools\rustup").Path
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
    if (Test-Path "$tools\gcc\w64devkit\bin") {
        $env:PATH = (Resolve-Path "$tools\gcc\w64devkit\bin").Path + ";$env:PATH"
        $env:LIBRARY_PATH = (Resolve-Path "$PSScriptRoot\.cargo\libstub").Path
    }
}

Push-Location $PSScriptRoot
try {
    cargo build --release -p flowtime-ae -p flowtime-cli
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    New-Item -ItemType Directory -Force dist | Out-Null
    Copy-Item target\release\flowtime_ae.dll dist\Flowtime.aex -Force
    Copy-Item target\release\flowtime.exe dist\flowtime.exe -Force
    Write-Host "dist\Flowtime.aex, dist\flowtime.exe"
} finally {
    Pop-Location
}
