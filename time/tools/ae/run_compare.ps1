# Ground-truth comparison of Flowtime and Twixtor Pro inside After Effects.
# Renders ref / twixtor / flowtime from compare\src (TIFF sequence of the clip),
# each in its own AE session (AE 23.4 fails the second item of a scripted
# render queue in a fresh session), then scores both against the real frames.
# Closes and restarts After Effects. Usage: run_compare.ps1 [-Install] [-Step 4]
param([switch]$Install, [int]$Step = 4)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$cmp = Join-Path $root 'compare'
$base = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'compare.jsx'))
$jobs = @(@('ref', 'ref'), @('twixtor', 'tw'), @('flowtime', 'ft'))
$first = $true
foreach ($j in $jobs) {
    $name = $j[0]
    Remove-Item (Join-Path $cmp $name) -Recurse -Force -ErrorAction SilentlyContinue
    $log = Join-Path $cmp "ae_log_$name.json"
    Remove-Item $log -ErrorAction SilentlyContinue
    $js = $base.Replace('var jobs = [["ref", ref], ["twixtor", tw], ["flowtime", ft]];', "var jobs = [[`"$name`", $($j[1])]];").Replace('/ae_log.json', "/ae_log_$name.json").Replace('var STEP = 4;', "var STEP = $Step;")
    $jsx = Join-Path $env:TEMP "flowtime_job_$name.jsx"
    [IO.File]::WriteAllText($jsx, $js)
    $restart = @{ Script = $jsx }
    if (-not ($Install -and $first)) { $restart.NoInstall = $true }
    & (Join-Path $PSScriptRoot 'restart_ae.ps1') @restart | Out-Null
    $first = $false
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while (-not (Test-Path $log) -and $sw.Elapsed.TotalSeconds -lt 600) { Start-Sleep 2 }
    if (-not (Test-Path $log)) { throw "${name}: no result from AE" }
    $m = (Get-Content $log -Raw | Select-String -Pattern "$name render [0-9.]+ s").Matches
    Write-Host ($(if ($m) { $m[0].Value } else { "${name}: render failed, see $log" }))
}
& (Join-Path $root 'dist\flowtime.exe') score (Join-Path $cmp 'ref') (Join-Path $cmp 'flowtime') (Join-Path $cmp 'twixtor') $Step (Join-Path $cmp 'sheets') | Select-Object -Last 4
