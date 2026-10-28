[CmdletBinding()]
param([string]$PackageRoot='', [string]$MediaCoreRoot='', [string]$BackupRoot='',
      [string]$Restore='', [switch]$Check, [string]$ResultPath='')
$ErrorActionPreference='Stop'
Import-Module (Join-Path $PSScriptRoot 'installer/Flow.Install.psm1') -Force
try {
    if (-not $PackageRoot) { $PackageRoot=$PSScriptRoot }
    if (-not $MediaCoreRoot) { $MediaCoreRoot=Get-FlowDefaultRoot }
    if (-not $BackupRoot) { $BackupRoot=Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'Flow\Backups' }
    if (-not $Check -and -not $ResultPath) { $ResultPath=Join-Path ([IO.Path]::GetTempPath()) ('Flow-result-'+[guid]::NewGuid().ToString('N')+'.json') }
    $PackageRoot=[IO.Path]::GetFullPath($PackageRoot)
    $MediaCoreRoot=[IO.Path]::GetFullPath($MediaCoreRoot)
    $BackupRoot=[IO.Path]::GetFullPath($BackupRoot)
    if($Restore){$Restore=[IO.Path]::GetFullPath($Restore)}
    if($ResultPath){$ResultPath=[IO.Path]::GetFullPath($ResultPath)}
    $identity=[Security.Principal.WindowsIdentity]::GetCurrent()
    $admin=(New-Object Security.Principal.WindowsPrincipal($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $Check -and -not $admin) {
        $q={param($s) "'"+$s.Replace("'","''")+"'"}
        $command='& '+(& $q $PSCommandPath)+' -PackageRoot '+(& $q $PackageRoot)+' -MediaCoreRoot '+(& $q $MediaCoreRoot)+' -BackupRoot '+(& $q $BackupRoot)
        if ($Restore) { $command+=' -Restore '+(& $q $Restore) }
        if ($ResultPath) { $command+=' -ResultPath '+(& $q $ResultPath) }
        $command+='; exit $LASTEXITCODE'
        $encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
        $sys=if([Environment]::Is64BitOperatingSystem -and -not [Environment]::Is64BitProcess){'Sysnative'}else{'System32'}
        $ps=Join-Path ([Environment]::GetFolderPath('Windows')) ($sys+'\WindowsPowerShell\v1.0\powershell.exe')
        $child=Start-Process -FilePath $ps -ArgumentList @('-NoLogo','-NoProfile','-ExecutionPolicy','Bypass','-EncodedCommand',$encoded) -Verb RunAs -WindowStyle Hidden -Wait -PassThru
        $details=if(Test-Path -LiteralPath $ResultPath){Get-Content -LiteralPath $ResultPath -Raw -Encoding UTF8|ConvertFrom-Json}else{$null}
        if ($child.ExitCode -ne 0) {
            if ($details -and $details.Message) { throw $details.Message }
            throw 'Installation stopped. Close Adobe applications, check the package, and retry.'
        }
        Write-Host ($details | Format-List | Out-String)
        exit 0
    }
    if ($Check) { $result=Get-FlowInstallPlan $PackageRoot $MediaCoreRoot }
    elseif ($Restore) { $result=Restore-FlowInstall $Restore }
    else { $result=Invoke-FlowInstall $PackageRoot $MediaCoreRoot $BackupRoot }
    if ($ResultPath) { $result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ResultPath -Encoding UTF8 }
    if ($Check) { Write-Host "Verified $($result.PluginCount) plugins and $($result.PanelCount) panels. Planned changes: $($result.Operations.Count); duplicates: $($result.Duplicates)." }
    else { Write-Host ($result | Format-List | Out-String) }
    exit 0
} catch {
    $message=$_.Exception.Message
    if ($ResultPath) { @{Status='Error';Message=$message} | ConvertTo-Json | Set-Content -LiteralPath $ResultPath -Encoding UTF8 }
    Write-Host ('Flow: '+$message) -ForegroundColor Red
    exit 1
}
