# Windows PowerShell 5.1 compatible. No host is closed by this module.
# One transactional installer for the whole Flow set: the After Effects plug-ins
# (MediaCore\Flow) and the two ScriptUI panels (Flowcut, FlowCurves). It also
# retires what the set replaces: Facet / Flowtime plug-ins, Dedupe / ezcurves panels.
Set-StrictMode -Version 2
$ErrorActionPreference='Stop'

# Panels in the package and the text that proves a file of that name is ours:
# an unrelated file with the same name is never overwritten.
$script:FlowScripts=@{ 'Flowcut.jsx'='var SETTINGS = "Flowcut";'; 'FlowCurves.jsx'='#targetengine "FlowCurves"' }
# Panels of the products that became Flow. Removed on upgrade, and only when the file really is theirs.
$script:FlowLegacyScripts=@{ 'Dedupe.jsx'='var SETTINGS = "Dedupe";'; 'ezcurves.jsx'='#targetengine "EZCurves"' }
# Licence files written by the Facet installer (exact hashes), removed together with its plug-ins.
$script:FlowLegacyDocs=@{
    'licenses\IJG-README.txt'='4cf9c190f184019b61c36bcc49d4a24ffeb2a838c6f1690d6c4db14cfb2d0478'
    'licenses\IJG-FACET.md'='53beb2c709aa0e672afeb359d52120180f88a1825b64ec8c127f42ebe4323602'
    'THIRD-PARTY-NOTICES.txt'='ad9ea386ed3b23fc1b0d686c791325051de0a13ed69cb3d4d8f8e3d31637eeb2'
}
$script:FlowDocLeaves=@('IJG-README.txt','IJG-FACET.md','IJG-FLOW.md','THIRD-PARTY-NOTICES.txt')
$script:FlowPluginId='^B4DM Flow(?:time| [A-Za-z0-9 ]+)$'
$script:FlowRestoreId='^B4DM (?:Flow(?:time| [A-Za-z0-9 ]+)|Facet [A-Za-z0-9 ]+)$'

function Get-FlowHash([string]$Path) {
    $sha=[Security.Cryptography.SHA256]::Create();$stream=$null
    try {
        $stream=[IO.File]::OpenRead($Path)
        [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-','').ToLowerInvariant()
    } finally { if($stream){$stream.Dispose()};$sha.Dispose() }
}
function Get-FlowFullPath([string]$Path) { [IO.Path]::GetFullPath($Path).TrimEnd('\','/') }
function Test-FlowUnder([string]$Path, [string]$Root) {
    $p=Get-FlowFullPath $Path; $r=Get-FlowFullPath $Root
    $p.Equals($r,[StringComparison]::OrdinalIgnoreCase) -or $p.StartsWith($r+'\',[StringComparison]::OrdinalIgnoreCase)
}
function Assert-FlowPath([string]$Path, [string[]]$Roots) {
    $p=Get-FlowFullPath $Path
    if (-not @($Roots | Where-Object { Test-FlowUnder $p $_ }).Count) { throw "Path outside installation roots: $p" }
    $part=$p
    while ($part -and $part -ne [IO.Path]::GetPathRoot($part)) {
        if (Test-Path -LiteralPath $part) {
            $item=Get-Item -LiteralPath $part -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Junctions/symlinks are not supported here: $part" }
        }
        $part=[IO.Path]::GetDirectoryName($part)
    }
    $p
}
function Test-FlowIdentity([string]$Path, [string]$MatchName) {
    # PiPL matchName is NUL terminated: GlowDarks must not match Glow.
    [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($Path)).Contains($MatchName+[char]0)
}
function Test-FlowScriptIdentity([string]$Path, [string]$Signature) {
    [IO.File]::ReadAllText($Path,[Text.Encoding]::UTF8).Contains($Signature)
}
function Get-FlowRunningHosts {
    @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.ProcessName -in @('AfterFX','aerender','aerendercore','Adobe Premiere Pro','Adobe Media Encoder')
    } | Select-Object -ExpandProperty ProcessName -Unique)
}
function Assert-FlowHostsClosed {
    $hosts=@(Get-FlowRunningHosts)
    if ($hosts.Count) { throw ('Close Adobe applications before installation or restore: '+($hosts -join ', ')+'. No files were changed.') }
}
function Get-FlowDefaultRoot {
    $pf=$env:ProgramW6432
    if (-not $pf) { $pf=[Environment]::GetFolderPath('ProgramFiles') }
    Join-Path $pf 'Adobe\Common\Plug-ins\7.0\MediaCore'
}
# Plug-in search roots (the shared MediaCore folder + each After Effects' own Plug-ins)
# and the ScriptUI Panels folder of each After Effects found next to it.
function Get-FlowRoots([string]$MediaCoreRoot) {
    $full=Get-FlowFullPath $MediaCoreRoot
    $plugins=@($full); $scripts=@()
    if ($full -match '\\Common\\Plug-ins\\7\.0\\MediaCore$') {
        $adobe=Split-Path (Split-Path (Split-Path (Split-Path $full)))
        if (Test-Path -LiteralPath $adobe) {
            foreach ($dir in Get-ChildItem -LiteralPath $adobe -Directory -Force) {
                if ($dir.Name -like 'Adobe After Effects*' -and -not ($dir.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                    $p=Join-Path $dir.FullName 'Support Files\Plug-ins'
                    if (Test-Path -LiteralPath $p) { $plugins+=Get-FlowFullPath $p }
                    $s=Join-Path $dir.FullName 'Support Files\Scripts\ScriptUI Panels'
                    if (Test-Path -LiteralPath $s) { $scripts+=Get-FlowFullPath $s }
                }
            }
        }
    }
    [pscustomobject]@{Plugins=@($plugins | Select-Object -Unique);Scripts=@($scripts | Select-Object -Unique)}
}
function Get-FlowAex([string]$Root) {
    if (-not (Test-Path -LiteralPath $Root)) { return }
    $null=Assert-FlowPath $Root @($Root)
    $todo=New-Object 'System.Collections.Generic.Stack[string]'
    $todo.Push($Root)
    while ($todo.Count) {
        foreach ($f in Get-ChildItem -LiteralPath $todo.Pop() -Force) {
            if ($f.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
            if ($f.PSIsContainer) { $todo.Push($f.FullName) }
            elseif ($f.Extension -ieq '.aex') { $f }
        }
    }
}
function Test-FlowPackage {
    param([Parameter(Mandatory)][string]$PackageRoot)
    $root=Get-FlowFullPath $PackageRoot
    $meta=Get-Content -LiteralPath (Join-Path $root 'bundle.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($meta.schema -ne 2 -or $meta.product -ne 'Flow' -or @($meta.plugins).Count -ne $meta.pluginCount -or $meta.pluginCount -lt 1 -or
        @($meta.scripts).Count -ne $meta.scriptCount) { throw 'Invalid Flow bundle metadata.' }
    $hashes=@{}
    foreach ($line in Get-Content -LiteralPath (Join-Path $root 'SHA256SUMS.txt')) {
        if ($line -notmatch '^([a-fA-F0-9]{64})  (dist/(?:F_[A-Za-z0-9]+\.aex|Flowtime\.aex|Flowcut\.jsx|FlowCurves\.jsx))$') { throw "Invalid checksum entry: $line" }
        if ($hashes.ContainsKey($Matches[2])) { throw 'Duplicate checksum entry.' }
        $hashes[$Matches[2]]=$Matches[1].ToLowerInvariant()
    }
    $seen=@{}; $ids=@{}; $files=@()
    foreach ($plugin in $meta.plugins) {
        if ($plugin.name -notmatch '^(?:F_[A-Za-z0-9]+|Flowtime)\.aex$' -or $plugin.matchName -notmatch $script:FlowPluginId) { throw 'Invalid plugin name/identity.' }
        if ($seen.ContainsKey($plugin.name) -or $ids.ContainsKey($plugin.matchName)) { throw 'Duplicate plugin identity in bundle.' }
        $seen[$plugin.name]=$true; $ids[$plugin.matchName]=$true
        $rel='dist/'+$plugin.name
        if (-not $hashes.ContainsKey($rel)) { throw "Missing checksum: $rel" }
        $src=Assert-FlowPath (Join-Path $root $rel) @($root)
        if ((Get-FlowHash $src) -ne $hashes[$rel]) { throw "Checksum mismatch: $rel" }
        if (-not (Test-FlowIdentity $src $plugin.matchName)) { throw "Wrong plugin identity: $rel" }
        $files+=[pscustomobject]@{Name=$plugin.name;MatchName=$plugin.matchName;Source=$src;Hash=$hashes[$rel]}
    }
    $panels=@(); $panelNames=@{}
    foreach ($name in $meta.scripts) {
        if (-not $script:FlowScripts.ContainsKey([string]$name) -or $panelNames.ContainsKey([string]$name)) { throw 'Invalid or duplicate panel in bundle.' }
        $panelNames[[string]$name]=$true
        $rel='dist/'+$name
        if (-not $hashes.ContainsKey($rel)) { throw "Missing checksum: $rel" }
        $src=Assert-FlowPath (Join-Path $root $rel) @($root)
        if ((Get-FlowHash $src) -ne $hashes[$rel]) { throw "Checksum mismatch: $rel" }
        if (-not (Test-FlowScriptIdentity $src $script:FlowScripts[[string]$name])) { throw "Wrong panel identity: $rel" }
        $panels+=[pscustomobject]@{Name=[string]$name;Signature=$script:FlowScripts[[string]$name];Source=$src;Hash=$hashes[$rel]}
    }
    if ($hashes.Count -ne $files.Count+$panels.Count) { throw 'Incomplete/extra binary checksum entries.' }
    if (@(Get-ChildItem -LiteralPath (Join-Path $root 'dist') -Filter '*.aex' -File).Count -ne $files.Count) { throw 'Unexpected/missing AEX files in dist.' }
    if (@(Get-ChildItem -LiteralPath (Join-Path $root 'dist') -Filter '*.jsx' -File).Count -ne $panels.Count) { throw 'Unexpected/missing panel files in dist.' }
    $assets=@()
    foreach ($asset in $meta.assets) {
        if ($asset.source -notin @('third_party/ijg/README','third_party/ijg/FLOW.md','installer/THIRD-PARTY-NOTICES.txt') -or
            $asset.destination -notin @('licenses/IJG-README.txt','licenses/IJG-FLOW.md','THIRD-PARTY-NOTICES.txt') -or $asset.sha256 -notmatch '^[a-f0-9]{64}$') { throw 'Invalid license asset.' }
        $src=Assert-FlowPath (Join-Path $root $asset.source) @($root)
        if ((Get-FlowHash $src) -ne $asset.sha256) { throw "Checksum mismatch: $($asset.source)" }
        $assets+=[pscustomobject]@{Name=$asset.destination;MatchName='';Source=$src;Hash=$asset.sha256}
    }
    if ($assets.Count -ne 3 -or @($assets.Name | Select-Object -Unique).Count -ne 3) { throw 'Missing license files.' }
    [pscustomobject]@{Root=$root;Version=$meta.version;Plugins=$files;Panels=$panels;Assets=$assets;PluginCount=$files.Count;PanelCount=$panels.Count}
}
function Get-FlowInstallPlan {
    param([Parameter(Mandatory)][string]$PackageRoot,[string]$MediaCoreRoot=(Get-FlowDefaultRoot))
    $package=Test-FlowPackage $PackageRoot
    $roots=Get-FlowRoots $MediaCoreRoot
    $all=@($roots.Plugins)+@($roots.Scripts)
    $target=Assert-FlowPath (Join-Path $MediaCoreRoot 'Flow') $roots.Plugins
    $ops=New-Object 'System.Collections.Generic.List[object]'
    $add={ param($Kind,$Class,$Dst,$Src,$Before,$After,$Match)
        $ops.Add([pscustomobject]@{Kind=$Kind;Class=$Class;Destination=$Dst;Source=$Src;BeforeHash=$Before;AfterHash=$After;MatchName=$Match;Backup='';Stage=''}) }
    $canonical=@{}; $known=@{}
    foreach ($p in $package.Plugins) {
        $canonical[(Join-Path $target $p.Name)]=$true; $known[$p.MatchName]=$true
        # The same effect under its Facet name: the former identity is retired.
        if ($p.MatchName -like 'B4DM Flow *') { $known['B4DM Facet '+$p.MatchName.Substring(10)]=$true }
    }
    foreach ($p in @($package.Plugins)+@($package.Assets)) {
        $dst=Assert-FlowPath (Join-Path $target $p.Name) @($target)
        $before=''
        if (Test-Path -LiteralPath $dst) {
            if ($p.MatchName -and -not (Test-FlowIdentity $dst $p.MatchName)) { throw "Unrelated file occupies the destination; refusing to overwrite: $dst" }
            $before=Get-FlowHash $dst
        }
        $class=if($p.MatchName){'plugin'}else{'doc'}
        if ($before -ne $p.Hash) { & $add 'Write' $class $dst $p.Source $before $p.Hash $p.MatchName }
    }
    foreach ($root in $roots.Scripts) {
        foreach ($p in $package.Panels) {
            $dst=Assert-FlowPath (Join-Path $root $p.Name) @($root)
            $before=''
            if (Test-Path -LiteralPath $dst) {
                if (-not (Test-FlowScriptIdentity $dst $p.Signature)) { throw "Unrelated file occupies the destination; refusing to overwrite: $dst" }
                $before=Get-FlowHash $dst
            }
            if ($before -ne $p.Hash) { & $add 'Write' 'script' $dst $p.Source $before $p.Hash '' }
        }
        foreach ($name in $script:FlowLegacyScripts.Keys) {
            $old=Assert-FlowPath (Join-Path $root $name) @($root)
            if ((Test-Path -LiteralPath $old -PathType Leaf) -and (Test-FlowScriptIdentity $old $script:FlowLegacyScripts[$name])) {
                & $add 'Remove' 'script' $old '' (Get-FlowHash $old) '' ''
            }
        }
    }
    foreach ($scan in $roots.Plugins) {
        foreach ($f in Get-FlowAex $scan) {
            if ($canonical.ContainsKey($f.FullName)) { continue }
            $text=[Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($f.FullName))
            foreach ($m in [regex]::Matches($text,'B4DM (?:Flow|Facet)[A-Za-z0-9 ]*(?=\x00)')) {
                if ($known.ContainsKey($m.Value)) {
                    $null=Assert-FlowPath $f.FullName $all
                    & $add 'Remove' 'plugin' $f.FullName '' (Get-FlowHash $f.FullName) '' $m.Value
                    break
                }
            }
        }
        $legacy=Join-Path $scan 'Facet'
        foreach ($rel in $script:FlowLegacyDocs.Keys) {
            $doc=Join-Path $legacy $rel
            if ((Test-Path -LiteralPath $doc -PathType Leaf) -and (Get-FlowHash $doc) -eq $script:FlowLegacyDocs[$rel]) {
                $null=Assert-FlowPath $doc $all
                & $add 'Remove' 'doc' $doc '' $script:FlowLegacyDocs[$rel] '' ''
            }
        }
    }
    [pscustomobject]@{Version=$package.Version;PluginCount=$package.PluginCount;PanelCount=$package.PanelCount;ScriptTargets=@($roots.Scripts).Count;
        Target=$target;PluginRoots=@($roots.Plugins);ScanRoots=@($all);Operations=@($ops.ToArray());
        Duplicates=@($ops | Where-Object { $_.Kind -eq 'Remove' }).Count}
}
function Write-FlowJournal($Journal,[string]$Directory) {
    [IO.File]::WriteAllText((Join-Path $Directory 'journal.json'),($Journal | ConvertTo-Json -Depth 8),(New-Object Text.UTF8Encoding($false)))
}
function Copy-FlowFile([string]$Source,[string]$Destination) { [IO.File]::Copy($Source,$Destination,$true) }
function Set-FlowFile([string]$Source,[string]$Destination) {
    $null=[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Destination))
    $tmp=$Destination+'.flow-'+[guid]::NewGuid().ToString('N')+'.tmp'
    try {
        Copy-FlowFile $Source $tmp
        if ([IO.File]::Exists($Destination)) { [IO.File]::Replace($tmp,$Destination,[NullString]::Value) }
        else { [IO.File]::Move($tmp,$Destination) }
    } finally { if ([IO.File]::Exists($tmp)) { [IO.File]::Delete($tmp) } }
}
# Folders left empty by retiring Facet / Flowtime. Never a folder that still holds anything.
function Remove-FlowEmptyLegacyDirs([string[]]$PluginRoots) {
    foreach ($root in $PluginRoots) {
        foreach ($rel in @('Facet\licenses','Facet','Flowtime')) {
            $dir=Join-Path $root $rel
            try {
                if (-not [IO.Directory]::Exists($dir)) { continue }
                if ([IO.File]::GetAttributes($dir) -band [IO.FileAttributes]::ReparsePoint) { continue }
                if (-not @(Get-ChildItem -LiteralPath $dir -Force).Count) { [IO.Directory]::Delete($dir) }
            } catch { }
        }
    }
}
function Restore-FlowInstall {
    param([Parameter(Mandatory)][string]$BackupPath,[switch]$Automatic)
    $mutex=$null; $owned=$false
    try {
    if (-not $Automatic) {
        $mutex=New-Object Threading.Mutex($false,'Global\Flow.AE.Installer')
        try { $owned=$mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $owned=$true }
        if (-not $owned) { throw 'Another Flow installer or restore is running.' }
    }
    if (-not $Automatic) { Assert-FlowHostsClosed }
    $backup=Get-FlowFullPath $BackupPath
    $null=Assert-FlowPath $backup @($backup)
    $j=Get-Content -LiteralPath (Join-Path $backup 'journal.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($j.Schema -ne 2 -or $j.Product -ne 'Flow' -or @($j.ScanRoots).Count -lt 1) { throw 'Invalid backup journal.' }
    if ($j.Status -in @('Restored','RolledBack')) { return [pscustomobject]@{Status=$j.Status;Backup=$backup;Changed=0} }
    # Verify every original/current file before the first restore write.
    foreach ($op in $j.Operations) {
        $null=Assert-FlowPath $op.Destination $j.ScanRoots
        $leaf=[IO.Path]::GetFileName($op.Destination)
        switch ($op.Class) {
            'plugin' { if ($op.MatchName -notmatch $script:FlowRestoreId -or [IO.Path]::GetExtension($op.Destination) -ine '.aex') { throw 'Invalid plugin restore entry.' } }
            'script' {
                if (-not ($script:FlowScripts.ContainsKey($leaf) -or $script:FlowLegacyScripts.ContainsKey($leaf)) -or
                    [IO.Path]::GetFileName([IO.Path]::GetDirectoryName($op.Destination)) -ine 'ScriptUI Panels') { throw 'Invalid panel restore entry.' }
            }
            'doc' { if ($leaf -notin $script:FlowDocLeaves) { throw 'Invalid document restore entry.' } }
            default { throw 'Invalid restore entry.' }
        }
        if ($op.BeforeHash) {
            $src=Assert-FlowPath (Join-Path $backup $op.Backup) @($backup)
            if ((Get-FlowHash $src) -ne $op.BeforeHash) { throw "Damaged backup: $src" }
            if ($op.Class -eq 'plugin' -and -not (Test-FlowIdentity $src $op.MatchName)) { throw 'Wrong backup plugin identity.' }
        }
        if (Test-Path -LiteralPath $op.Destination) {
            $current=Get-FlowHash $op.Destination
            if ($current -ne $op.BeforeHash -and $current -ne $op.AfterHash) { throw "File changed since installation; restore stopped: $($op.Destination)" }
        }
    }
    foreach ($op in @($j.Operations)[($j.Operations.Count-1)..0]) {
        if ($op.BeforeHash) { Set-FlowFile (Join-Path $backup $op.Backup) $op.Destination }
        elseif ([IO.File]::Exists($op.Destination)) { [IO.File]::Delete($op.Destination) }
    }
    foreach ($op in $j.Operations) {
        if ($op.BeforeHash) {
            if ((Get-FlowHash $op.Destination) -ne $op.BeforeHash) { throw 'Restored file checksum mismatch.' }
        } elseif ([IO.File]::Exists($op.Destination)) { throw 'Restore failed to remove a newly installed file.' }
    }
    $j.Status=if($Automatic){'RolledBack'}else{'Restored'}
    Write-FlowJournal $j $backup
    [pscustomobject]@{Status=$j.Status;Backup=$backup;Changed=$j.Operations.Count}
    } finally { if($owned){$mutex.ReleaseMutex()};if($mutex){$mutex.Dispose()} }
}
function Invoke-FlowInstallCore {
    param([Parameter(Mandatory)][string]$PackageRoot,[string]$MediaCoreRoot=(Get-FlowDefaultRoot),
          [string]$BackupRoot=(Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'Flow\Backups'))
    Assert-FlowHostsClosed
    $plan=Get-FlowInstallPlan $PackageRoot $MediaCoreRoot
    if (-not $plan.Operations.Count) {
        Remove-FlowEmptyLegacyDirs $plan.PluginRoots
        return [pscustomobject]@{Status='AlreadyInstalled';PluginCount=$plan.PluginCount;PanelCount=$plan.PanelCount;ScriptTargets=$plan.ScriptTargets;Target=$plan.Target;Backup='';Duplicates=0;Changed=0}
    }
    $bk=Get-FlowFullPath $BackupRoot
    foreach ($r in $plan.ScanRoots) { if ((Test-FlowUnder $bk $r) -or (Test-FlowUnder $r $bk)) { throw 'Backups must be outside all Adobe plugin search roots.' } }
    $null=Assert-FlowPath $bk @($bk)
    foreach ($op in $plan.Operations) {
        if ($op.BeforeHash) {
            $stream=[IO.File]::Open($op.Destination,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
            $stream.Dispose()
        }
    }
    $backup=Join-Path $bk ((Get-Date -Format 'yyyyMMdd-HHmmss')+'-'+[guid]::NewGuid().ToString('N'))
    $null=[IO.Directory]::CreateDirectory((Join-Path $backup 'before'))
    $null=[IO.Directory]::CreateDirectory((Join-Path $backup 'stage'))
    $i=0
    foreach ($op in $plan.Operations) {
        if ($op.BeforeHash) {
            $op.Backup='before/{0:D4}.bin' -f $i
            Copy-FlowFile $op.Destination (Join-Path $backup $op.Backup)
            if ((Get-FlowHash (Join-Path $backup $op.Backup)) -ne $op.BeforeHash) { throw 'Backup verification failed; original files untouched.' }
        }
        if ($op.Kind -eq 'Write') {
            $op.Stage='stage/{0:D4}.bin' -f $i
            Copy-FlowFile $op.Source (Join-Path $backup $op.Stage)
            if ((Get-FlowHash (Join-Path $backup $op.Stage)) -ne $op.AfterHash) { throw 'Staging verification failed; original files untouched.' }
        }
        $i++
    }
    $j=[pscustomobject]@{Schema=2;Product='Flow';Version=$plan.Version;Status='Prepared';Target=$plan.Target;ScanRoots=$plan.ScanRoots;Operations=$plan.Operations}
    Write-FlowJournal $j $backup
    try {
        Assert-FlowHostsClosed
        $j.Status='Applying'; Write-FlowJournal $j $backup
        foreach ($op in $plan.Operations) {
            $null=Assert-FlowPath $op.Destination $plan.ScanRoots
            $actual=if([IO.File]::Exists($op.Destination)){Get-FlowHash $op.Destination}else{''}
            if ($actual -ne $op.BeforeHash) { throw "Destination changed during installation: $($op.Destination)" }
            if ($op.Kind -eq 'Write') { Set-FlowFile (Join-Path $backup $op.Stage) $op.Destination }
            else { [IO.File]::Delete($op.Destination) }
        }
        foreach ($op in $plan.Operations) {
            if ($op.AfterHash) {
                if ((Get-FlowHash $op.Destination) -ne $op.AfterHash) { throw "Installed checksum mismatch: $($op.Destination)" }
            } elseif ([IO.File]::Exists($op.Destination)) { throw 'Removal verification failed.' }
        }
        $j.Status='Complete'; Write-FlowJournal $j $backup
    } catch {
        $failure=$_.Exception.Message
        try { $null=Restore-FlowInstall $backup -Automatic }
        catch { throw "Install failed: $failure. Automatic restore also failed: $($_.Exception.Message). Backup/journal: $backup" }
        throw "Install failed and was rolled back: $failure. Backup/journal: $backup"
    }
    Remove-FlowEmptyLegacyDirs $plan.PluginRoots
    [pscustomobject]@{Status='Installed';PluginCount=$plan.PluginCount;PanelCount=$plan.PanelCount;ScriptTargets=$plan.ScriptTargets;Target=$plan.Target;Backup=$backup;Duplicates=$plan.Duplicates;Changed=$plan.Operations.Count}
}
function Invoke-FlowInstall {
    param([Parameter(Mandatory)][string]$PackageRoot,[string]$MediaCoreRoot=(Get-FlowDefaultRoot),
          [string]$BackupRoot=(Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'Flow\Backups'))
    $mutex=New-Object Threading.Mutex($false,'Global\Flow.AE.Installer');$owned=$false
    try {
        try { $owned=$mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $owned=$true }
        if (-not $owned) { throw 'Another Flow installer or restore is running.' }
        Invoke-FlowInstallCore $PackageRoot $MediaCoreRoot $BackupRoot
    } finally { if($owned){$mutex.ReleaseMutex()};$mutex.Dispose() }
}
Export-ModuleMember -Function Test-FlowPackage,Get-FlowInstallPlan,Invoke-FlowInstall,Restore-FlowInstall,Get-FlowDefaultRoot
