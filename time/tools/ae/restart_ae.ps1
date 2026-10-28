# Closes After Effects, installs dist\Flowtime.aex and starts AE running a script.
# Usage: restart_ae.ps1 [-Script path.jsx] [-NoInstall]
param([string]$Script, [switch]$NoInstall)
$ErrorActionPreference = 'Stop'
$ae = 'C:\Program Files\Adobe\Adobe After Effects 2023\Support Files\AfterFX.exe'
$dst = 'C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\Flowtime\Flowtime.aex'
$src = Join-Path $PSScriptRoot '..\..\dist\Flowtime.aex'

$p = Get-Process AfterFX -ErrorAction SilentlyContinue
if ($p) {
    # Quit through AE itself: a killed AE shows "Crash Repair Options" on the
    # next start, which blocks the startup script.
    $quit = Join-Path $env:TEMP 'flowtime_quit_ae.jsx'
    Set-Content $quit -Encoding ASCII -Value 'app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES); app.quit();'
    Start-Process -FilePath $ae -ArgumentList '-r', $quit | Out-Null
    if (-not $p.WaitForExit(60000)) {
        $p | Stop-Process -Force
        Start-Sleep 2
    }
}
Get-Process aerender -ErrorAction SilentlyContinue | Stop-Process -Force

if (-not $NoInstall) {
    Copy-Item $src $dst -Force
    Write-Host "installed $((Get-Item $dst).LastWriteTime)"
}
if ($Script) {
    Start-Process -FilePath $ae -ArgumentList '-r', (Resolve-Path $Script).Path | Out-Null
} else {
    Start-Process -FilePath $ae | Out-Null
}
Write-Host 'AE started'
