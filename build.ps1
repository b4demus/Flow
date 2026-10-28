# Builds everything in the Flow set and packs the installer:
#   fx\    the 41 effects (ex-Facet)      -> fx\dist\F_*.aex
#   time\  Flowtime retiming effect       -> time\dist\Flowtime.aex
#   cut\   Flowcut panel (ex-Dedupe)      -> used as is
#   curves\ FlowCurves panel (ex-ezcurves)-> used as is
# then tools\package.ps1 -> Flow-Setup.exe.  Use -SkipRust to repack without rebuilding.
# Toolchain: portable windows-gnu Rust from ..\.tools\rust-daw (see fx\tools\env.ps1).
param([switch]$SkipRust, [string]$Version='2026.10.28.1')
$ErrorActionPreference='Stop'
if (-not $SkipRust) {
    & (Join-Path $PSScriptRoot 'fx\build.ps1')
    & (Join-Path $PSScriptRoot 'time\build.ps1')
}
& (Join-Path $PSScriptRoot 'tools\package.ps1') -Version $Version
