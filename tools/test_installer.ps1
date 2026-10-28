# Integration tests for the Flow installer. They use isolated copies of the Adobe folder
# layout (MediaCore + two After Effects) and real Flow binaries; a real installation is never touched.
# Run build.ps1 (or tools\package.ps1) first.
param([string]$Workspace=(Join-Path $PSScriptRoot '..\work\installer-tests'))
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$Workspace=[IO.Path]::GetFullPath($Workspace)
if (-not $Workspace.StartsWith($root+'\work\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Tests must stay inside Flow/work.' }
$base=Join-Path $Workspace ([guid]::NewGuid().ToString('N'))
$null=New-Item -ItemType Directory -Path $base -Force
$module=Import-Module (Join-Path $root 'installer/Flow.Install.psm1') -Force -PassThru
# Host checks are controlled only in this private, isolated test module session.
& $module { function script:Get-FlowRunningHosts { @() } }
function Assert($Condition,[string]$Message) { if(-not $Condition){throw $Message} }
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Reject([scriptblock]$Code,[string]$Pattern) {
    $errorText=''
    try { & $Code | Out-Null } catch { $errorText=$_.Exception.Message }
    Assert ($errorText -match $Pattern) "Expected failure /$Pattern/, got: $errorText"
}
function Snapshot([string]$Dir) {
    @((Get-ChildItem -LiteralPath $Dir -File -Recurse -ErrorAction SilentlyContinue | Sort-Object FullName | ForEach-Object { $_.FullName+' '+(Hash $_.FullName) })) -join "`n"
}
function Test-Case([string]$Name,[scriptblock]$Code) { & $Code; Write-Output "PASS $Name" }
function SetPackageMeta { $meta|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $pkg 'bundle.json') -Encoding UTF8 }

# --- a small package built from the real payload
$keep=@('F_Glow.aex','F_Hotspots.aex','Flowtime.aex')
$pkg=Join-Path $base "package space ' & !"
$null=New-Item -ItemType Directory -Path (Join-Path $pkg 'dist') -Force
$meta=Get-Content -LiteralPath (Join-Path $root 'bundle.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$meta.plugins=@($meta.plugins|Where-Object{$_.name -in $keep});$meta.pluginCount=$keep.Count
foreach($p in $meta.plugins){Copy-Item -LiteralPath (Join-Path $root ('dist/'+$p.name)) -Destination (Join-Path $pkg ('dist/'+$p.name))}
$panels=@('Flowcut.jsx','FlowCurves.jsx')
foreach($n in $panels){Copy-Item -LiteralPath (Join-Path $root ('dist/'+$n)) -Destination (Join-Path $pkg ('dist/'+$n))}
foreach($a in $meta.assets){$dst=Join-Path $pkg $a.source;$null=New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($dst)) -Force;Copy-Item -LiteralPath (Join-Path $root $a.source) -Destination $dst}
SetPackageMeta
$names=@($keep)+$panels
$sums=@(Get-Content -LiteralPath (Join-Path $root 'SHA256SUMS.txt')|Where-Object{$line=$_;@($names|Where-Object{$line.EndsWith('dist/'+$_)}).Count})
Assert ($sums.Count -eq $names.Count) 'Test package checksums'
$sums|Set-Content -LiteralPath (Join-Path $pkg 'SHA256SUMS.txt') -Encoding ascii

# --- isolated Adobe layout: MediaCore + After Effects 2023 and 2024
$adobe=Join-Path $base "Adobe space ' & !"
$media=Join-Path $adobe 'Common\Plug-ins\7.0\MediaCore'; $backups=Join-Path $base 'Backups'
$ae1=Join-Path $adobe 'Adobe After Effects 2023\Support Files'; $ae2=Join-Path $adobe 'Adobe After Effects 2024\Support Files'
$ui1=Join-Path $ae1 'Scripts\ScriptUI Panels'; $ui2=Join-Path $ae2 'Scripts\ScriptUI Panels'
foreach($d in @($media,$ui1,$ui2,(Join-Path $ae1 'Plug-ins'),(Join-Path $ae2 'Plug-ins'))){$null=New-Item -ItemType Directory -Path $d -Force}

Test-Case 'Fresh installation: plugins, panels in every After Effects, licences' {
    $r=Invoke-FlowInstall $pkg $media $backups
    Assert ($r.Status -eq 'Installed' -and $r.PluginCount -eq 3 -and $r.PanelCount -eq 2 -and $r.ScriptTargets -eq 2) 'Fresh install result'
    Assert (@(Get-ChildItem -LiteralPath (Join-Path $media 'Flow') -Filter *.aex).Count -eq 3) 'Wrong plugin count'
    Assert ((Hash (Join-Path $media 'Flow/F_Hotspots.aex')) -eq (Hash (Join-Path $pkg 'dist/F_Hotspots.aex'))) 'Wrong installed binary'
    foreach($ui in @($ui1,$ui2)){foreach($n in $panels){Assert ((Hash (Join-Path $ui $n)) -eq (Hash (Join-Path $pkg ('dist/'+$n)))) "Panel $n missing in $ui"}}
    Assert (Test-Path -LiteralPath (Join-Path $media 'Flow/licenses/IJG-README.txt')) 'Missing license'
    $script:first=$r
}
Test-Case 'Repeated installation is a no-op' {
    $count=@(Get-ChildItem -LiteralPath $backups -Directory).Count
    $r=Invoke-FlowInstall $pkg $media $backups
    Assert ($r.Status -eq 'AlreadyInstalled' -and $r.Changed -eq 0) 'Not idempotent'
    Assert (@(Get-ChildItem -LiteralPath $backups -Directory).Count -eq $count) 'No-op made another backup'
}
Test-Case 'Manual restore of a fresh installation' {
    $r=Restore-FlowInstall $first.Backup
    Assert ($r.Status -eq 'Restored') 'Restore status'
    Assert (-not (Test-Path -LiteralPath (Join-Path $media 'Flow/F_Glow.aex'))) 'New plugin survived restore'
    Assert (-not (Test-Path -LiteralPath (Join-Path $ui1 'Flowcut.jsx'))) 'New panel survived restore'
}

# --- the machine as it is before Flow: Facet, Flowtime, Dedupe, ezcurves, plus unrelated files
$null=New-Item -ItemType Directory -Path (Join-Path $media 'Flowtime'),(Join-Path $media 'Facet\licenses'),(Join-Path $media 'Other') -Force
$legacyHot=Join-Path $media 'Flowtime/Facet Hotspots.aex'   # a Facet effect once installed into the Flowtime folder
[IO.File]::WriteAllBytes($legacyHot,([Text.Encoding]::ASCII.GetBytes('MZ old Facet Hotspots plugin B4DM Facet Hotspots')+[byte[]](0)))
$legacyGlow=Join-Path $media 'Facet/F_Glow.aex'
[IO.File]::WriteAllBytes($legacyGlow,([Text.Encoding]::ASCII.GetBytes('MZ old Facet Glow plugin B4DM Facet Glow')+[byte[]](0)))
$oldFlowtime=Join-Path $media 'Flowtime/Flowtime.aex'        # same identity as the new one, wrong place and old build
[IO.File]::WriteAllBytes($oldFlowtime,([IO.File]::ReadAllBytes((Join-Path $pkg 'dist/Flowtime.aex'))+[byte[]](11,12,13)))
Copy-Item (Join-Path $root 'tools/fixtures/facet-THIRD-PARTY-NOTICES.txt') (Join-Path $media 'Facet/THIRD-PARTY-NOTICES.txt')
Copy-Item (Join-Path $root 'fx/third_party/ijg/README') (Join-Path $media 'Facet/licenses/IJG-README.txt')
Copy-Item (Join-Path $root 'fx/third_party/ijg/FACET.md') (Join-Path $media 'Facet/licenses/IJG-FACET.md')
$unrelatedPlugin=Join-Path $media 'Other/Other.aex';[IO.File]::WriteAllText($unrelatedPlugin,'unrelated host plugin')
$oldDedupe=Join-Path $ui1 'Dedupe.jsx';[IO.File]::WriteAllText($oldDedupe,"(function () {`n    var SETTINGS = ""Dedupe"";`n})();")
$oldEz=Join-Path $ui1 'ezcurves.jsx';[IO.File]::WriteAllText($oldEz,"#target aftereffects`n#targetengine ""EZCurves""`n")
$unrelatedPanel=Join-Path $ui1 'VR Comp Editor.jsx';[IO.File]::WriteAllText($unrelatedPanel,'// somebody else')
$sameNameOtherDedupe=Join-Path $ui2 'Dedupe.jsx';[IO.File]::WriteAllText($sameNameOtherDedupe,'// a different script that just happens to be called Dedupe')
$oldGlowFlow=Join-Path $media 'Flow/F_Glow.aex'              # an earlier Flow build already in place
$null=New-Item -ItemType Directory -Path (Join-Path $media 'Flow') -Force
[IO.File]::WriteAllBytes($oldGlowFlow,([IO.File]::ReadAllBytes((Join-Path $pkg 'dist/F_Glow.aex'))+[byte[]](21,22,23)))
$unrelatedHashes=@($unrelatedPlugin,$unrelatedPanel,$sameNameOtherDedupe|ForEach-Object{(Hash $_)})
$original=Snapshot $adobe
Test-Case 'Upgrade retires Facet, Flowtime folder, Dedupe and ezcurves; keeps unrelated files' {
    $r=Invoke-FlowInstall $pkg $media $backups
    # 3 plugins found by identity (Facet Hotspots, Facet Glow, old Flowtime) + 3 Facet licence files + 2 panels
    Assert ($r.Duplicates -eq 8) "Unexpected number of retired files: $($r.Duplicates)"
    foreach($gone in @($legacyHot,$legacyGlow,$oldFlowtime,$oldDedupe,$oldEz)){Assert (-not (Test-Path -LiteralPath $gone)) "Legacy file survived: $gone"}
    Assert (-not (Test-Path -LiteralPath (Join-Path $media 'Facet'))) 'Empty Facet folder left behind'
    Assert (-not (Test-Path -LiteralPath (Join-Path $media 'Flowtime'))) 'Empty Flowtime folder left behind'
    Assert (@($unrelatedPlugin,$unrelatedPanel,$sameNameOtherDedupe|ForEach-Object{(Hash $_)}) -join ',' -eq ($unrelatedHashes -join ',')) 'Unrelated file changed'
    Assert ((Hash (Join-Path $media 'Flow/Flowtime.aex')) -eq (Hash (Join-Path $pkg 'dist/Flowtime.aex'))) 'Flowtime not installed'
    Assert ((Hash $oldGlowFlow) -eq (Hash (Join-Path $pkg 'dist/F_Glow.aex'))) 'Earlier Flow build not updated'
    $script:upgrade=$r
}
Test-Case 'Manual restore recovers legacy files and original bytes' {
    $null=Restore-FlowInstall $upgrade.Backup
    Assert ((Snapshot $adobe) -eq $original) 'Original tree not restored exactly'
}
Test-Case 'Corrupt payload is rejected without writes' {
    $p=Join-Path $pkg 'dist/F_Glow.aex';$bytes=[IO.File]::ReadAllBytes($p)
    try { [IO.File]::WriteAllBytes($p,($bytes+[byte[]](99)));Reject { Invoke-FlowInstall $pkg $media $backups } 'Checksum mismatch' }
    finally { [IO.File]::WriteAllBytes($p,$bytes) }
    Assert ((Snapshot $adobe) -eq $original) 'Corrupt payload changed installation'
}
Test-Case 'Panel with foreign content is rejected as a payload' {
    $p=Join-Path $pkg 'dist/Flowcut.jsx';$bytes=[IO.File]::ReadAllBytes($p)
    $sumPath=Join-Path $pkg 'SHA256SUMS.txt';$sumText=[IO.File]::ReadAllText($sumPath)
    try {
        [IO.File]::WriteAllText($p,'// not Flowcut')
        $fixed=($sumText -split '\r?\n'|Where-Object{$_}|ForEach-Object{if($_ -like '*dist/Flowcut.jsx'){(Hash $p)+'  dist/Flowcut.jsx'}else{$_}}) -join "`r`n"
        [IO.File]::WriteAllText($sumPath,$fixed)
        Reject { Invoke-FlowInstall $pkg $media $backups } 'Wrong panel identity'
    } finally { [IO.File]::WriteAllBytes($p,$bytes);[IO.File]::WriteAllText($sumPath,$sumText) }
    Assert ((Snapshot $adobe) -eq $original) 'Bad panel payload changed installation'
}
Test-Case 'Unrelated destination collision is rejected (plugin and panel)' {
    $p=Join-Path $media 'Flow/F_Glow.aex';$old=[IO.File]::ReadAllBytes($p);[IO.File]::WriteAllText($p,'not a Flow plugin')
    try { Reject { Invoke-FlowInstall $pkg $media $backups } 'Unrelated file';Assert ([IO.File]::ReadAllText($p) -eq 'not a Flow plugin') 'Collision overwritten' }
    finally { [IO.File]::WriteAllBytes($p,$old) }
    $q=Join-Path $ui2 'Flowcut.jsx';[IO.File]::WriteAllText($q,'// my own Flowcut')
    try { Reject { Invoke-FlowInstall $pkg $media $backups } 'Unrelated file';Assert ([IO.File]::ReadAllText($q) -eq '// my own Flowcut') 'Panel collision overwritten' }
    finally { [IO.File]::Delete($q) }
}
Test-Case 'Backups inside plugin search paths are rejected' { Reject { Invoke-FlowInstall $pkg $media (Join-Path $media 'Backups') } 'Backups must be outside' }
Test-Case 'Backups inside a ScriptUI Panels folder are rejected' { Reject { Invoke-FlowInstall $pkg $media (Join-Path $ui1 'Backups') } 'Backups must be outside' }
Test-Case 'Locked old plugin prevents all writes' {
    $stream=[IO.File]::Open($legacyHot,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read)
    try { Reject { Invoke-FlowInstall $pkg $media $backups } 'being used|another process|access|process cannot|Exception' }
    finally { $stream.Dispose() }
    Assert ((Snapshot $adobe) -eq $original) 'Lock caused partial installation'
}
Test-Case 'Host running blocks install without closing it' {
    & $module { function script:Get-FlowRunningHosts { @('AfterFX') } }
    try { Reject { Invoke-FlowInstall $pkg $media $backups } 'Close Adobe applications';Assert ((Snapshot $adobe) -eq $original) 'Running host caused writes' }
    finally { & $module { function script:Get-FlowRunningHosts { @() } } }
}
Test-Case 'Injected mid-commit failure automatically restores every file' {
    & $module { $script:failed=$false;function script:Copy-FlowFile([string]$Source,[string]$Destination) {
        if (-not $script:failed -and $Destination -like '*.flow-*.tmp' -and $Destination -like '*Flowcut*') { $script:failed=$true;throw 'Injected write failure' }
        [IO.File]::Copy($Source,$Destination,$true)
    } }
    try { Reject { Invoke-FlowInstall $pkg $media $backups } 'was rolled back';Assert ((Snapshot $adobe) -eq $original) 'Failed transaction changed tree' }
    finally { & $module { function script:Copy-FlowFile([string]$Source,[string]$Destination) { [IO.File]::Copy($Source,$Destination,$true) } } }
}
Test-Case 'Path traversal in metadata is rejected' {
    $meta.plugins[0].name='../F_Glow.aex';SetPackageMeta
    try { Reject { Invoke-FlowInstall $pkg $media $backups } 'Invalid plugin name' }
    finally { $meta.plugins[0].name='F_Glow.aex';SetPackageMeta }
    $meta.scripts=@('../Flowcut.jsx','FlowCurves.jsx');SetPackageMeta
    try { Reject { Invoke-FlowInstall $pkg $media $backups } 'Invalid or duplicate panel' }
    finally { $meta.scripts=@('Flowcut.jsx','FlowCurves.jsx');SetPackageMeta }
}
Test-Case 'Restore refuses to overwrite a later user edit' {
    $r=Invoke-FlowInstall $pkg $media $backups
    $p=Join-Path $media 'Flow/F_Glow.aex';[IO.File]::AppendAllText($p,'later edit')
    $before=Snapshot $adobe
    Reject { Restore-FlowInstall $r.Backup } 'changed since installation'
    Assert ((Snapshot $adobe) -eq $before) 'Restore partially wrote before rejecting later edit'
}
Test-Case 'Without After Effects only the plugins are installed' {
    $media2=Join-Path $base "Solo ' & !\Common\Plug-ins\7.0\MediaCore";$null=New-Item -ItemType Directory -Path $media2 -Force
    $r=Invoke-FlowInstall $pkg $media2 (Join-Path $base 'Backups2')
    Assert ($r.Status -eq 'Installed' -and $r.ScriptTargets -eq 0 -and $r.PluginCount -eq 3) 'Solo install result'
    Assert (@(Get-ChildItem -LiteralPath (Join-Path $media2 'Flow') -Filter *.aex).Count -eq 3) 'Solo plugins missing'
}
Write-Output "$((Select-String -Path $PSCommandPath -Pattern '^Test-Case ').Count) installer integration tests passed. Isolated data: $base"
