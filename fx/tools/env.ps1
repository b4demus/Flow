# Dot-source to get the portable windows-gnu Rust toolchain on PATH:  . tools\env.ps1
$tools = if ($env:PETAL_TOOLCHAIN) { $env:PETAL_TOOLCHAIN } else { Join-Path $PSScriptRoot '..\..\..\.tools\rust-daw' }
if (Test-Path "$tools\cargo\bin\cargo.exe") {
    $env:CARGO_HOME = (Resolve-Path "$tools\cargo").Path
    $env:RUSTUP_HOME = (Resolve-Path "$tools\rustup").Path
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
    if (Test-Path "$tools\gcc\w64devkit\bin") {
        $env:PATH = (Resolve-Path "$tools\gcc\w64devkit\bin").Path + ";$env:PATH"
        $env:LIBRARY_PATH = (Resolve-Path "$PSScriptRoot\..\.cargo\libstub").Path
    }
}
