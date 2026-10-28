# Stages the built Flow payload, writes the manifest and checksums, packs the ZIP
# and compiles the single-file Flow-Setup.exe around it.
# Needs fx\dist, time\dist (run build.ps1 first) and the two panels in cut\ and curves\.
param([string]$Version='2026.10.28.1', [string]$OutputDir='')
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $OutputDir) { $OutputDir=$root }
$OutputDir=[IO.Path]::GetFullPath($OutputDir)
$utf8=New-Object Text.UTF8Encoding($false)
function Sha([string]$p) { (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() }

# --- identities (name, matchName, category) straight from the build scripts
$ids=@{}
foreach ($b in Get-ChildItem (Join-Path $root 'fx\ae') -Recurse -Filter build.rs) {
    $m=[regex]::Matches([IO.File]::ReadAllText($b.FullName),'facet_plugin(?:_ex)?\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,\s*"([^"]+)"')
    foreach ($x in $m) { $ids[$x.Groups[1].Value+'.aex']=@{Match=$x.Groups[2].Value;Category=$x.Groups[3].Value} }
}
$ids['Flowtime.aex']=@{Match='B4DM Flowtime';Category='Flow Time'}

# --- stage dist
$dist=Join-Path $root 'dist'
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory $dist | Out-Null
foreach ($f in Get-ChildItem (Join-Path $root 'fx\dist') -Filter 'F_*.aex') { Copy-Item $f.FullName $dist }
Copy-Item (Join-Path $root 'time\dist\Flowtime.aex') $dist
Copy-Item (Join-Path $root 'cut\Flowcut.jsx') $dist
Copy-Item (Join-Path $root 'curves\FlowCurves.jsx') $dist

# --- every plug-in must carry exactly its Flow identity and category
$plugins=@()
foreach ($p in Get-ChildItem $dist -Filter '*.aex' | Sort-Object Name) {
    if (-not $ids.ContainsKey($p.Name)) { throw "No identity found for $($p.Name)" }
    $text=[Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($p.FullName))
    $id=$ids[$p.Name]
    if (-not $text.Contains($id.Match+[char]0)) { throw "$($p.Name): matchName '$($id.Match)' missing" }
    if (-not $text.Contains($id.Category)) { throw "$($p.Name): category '$($id.Category)' missing" }
    if ($text -match 'B4DM Facet|Facet [A-Z]') { throw "$($p.Name): still carries a Facet identity" }
    $plugins+=[ordered]@{name=$p.Name;matchName=$id.Match}
}
foreach ($n in $ids.Keys) { if (-not (Test-Path (Join-Path $dist $n))) { throw "Missing build output: $n" } }
$panels=@(Get-ChildItem $dist -Filter '*.jsx' | Sort-Object Name | ForEach-Object { $_.Name })

# --- licence assets
$assets=@()
foreach ($a in @(@('third_party/ijg/README','licenses/IJG-README.txt'),@('third_party/ijg/FLOW.md','licenses/IJG-FLOW.md'),@('installer/THIRD-PARTY-NOTICES.txt','THIRD-PARTY-NOTICES.txt'))) {
    $assets+=[ordered]@{source=$a[0];destination=$a[1];sha256=(Sha (Join-Path $root $a[0]))}
}
$meta=[ordered]@{schema=2;product='Flow';version=$Version;pluginCount=$plugins.Count;plugins=$plugins;scriptCount=$panels.Count;scripts=$panels;assets=$assets}
[IO.File]::WriteAllText((Join-Path $root 'bundle.json'),(($meta | ConvertTo-Json -Depth 6)+"`n"),$utf8)
$sums=foreach ($f in Get-ChildItem $dist -File | Sort-Object Name) { (Sha $f.FullName)+'  dist/'+$f.Name }
[IO.File]::WriteAllText((Join-Path $root 'SHA256SUMS.txt'),(($sums -join "`n")+"`n"),$utf8)

# --- ZIP (forward-slash entry names)
$files=@('bundle.json','SHA256SUMS.txt','install.ps1','install.cmd','LICENSE','README.md','README.en.md','docs/assets/flow-banner.svg',
         'installer/Flow.Install.psm1','installer/THIRD-PARTY-NOTICES.txt','installer/Setup.cs','installer/setup.manifest',
         'third_party/ijg/README','third_party/ijg/FLOW.md','tools/package.ps1')
$files+=Get-ChildItem $dist -File | Sort-Object Name | ForEach-Object { 'dist/'+$_.Name }
$workDir=Join-Path $root 'work'
New-Item -ItemType Directory -Force $workDir | Out-Null
$zip=Join-Path $workDir "Flow-$Version.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$za=[IO.Compression.ZipFile]::Open($zip,'Create')
try {
    foreach ($rel in $files) {
        $null=[IO.Compression.ZipFileExtensions]::CreateEntryFromFile($za,(Join-Path $root $rel),$rel,[IO.Compression.CompressionLevel]::Optimal)
    }
    $note="Flow - Windows x64 - $($plugins.Count) After Effects plugins + $($panels.Count) panels`r`n`r`nRun Flow-Setup.exe, or extract this ZIP completely and run install.cmd.`r`nSave your project and close Adobe applications first.`r`n`r`nThis software is based in part on the work of the Independent JPEG Group.`r`n"
    $e=$za.CreateEntry('README.txt',[IO.Compression.CompressionLevel]::Optimal)
    $w=New-Object IO.StreamWriter($e.Open(),$utf8); $w.Write($note); $w.Dispose()
} finally { $za.Dispose() }

# --- Setup.exe with the ZIP embedded
$setupWork=Join-Path $workDir ('setup-build\'+[guid]::NewGuid().ToString('N'))
$null=New-Item -ItemType Directory -Force $setupWork
$hash=Sha $zip
$code='using System.Reflection; [assembly: AssemblyTitle("Flow Setup")] [assembly: AssemblyVersion("'+$Version+'")] internal static class PayloadInfo { public const string Sha256="'+$hash+'"; public const string Version="'+$Version+'"; public const int Count='+$plugins.Count+'; public const int Panels='+$panels.Count+'; }'
$info=Join-Path $setupWork 'PayloadInfo.cs'; [IO.File]::WriteAllText($info,$code)
$csc=Join-Path ([Environment]::GetFolderPath('Windows')) 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path $csc)) { throw 'Windows .NET Framework C# compiler not found.' }
$exe=Join-Path $OutputDir 'Flow-Setup.exe'
& $csc /nologo /target:winexe /platform:x64 /optimize+ /utf8output /codepage:65001 "/out:$exe" "/win32manifest:$root\installer\setup.manifest" "/resource:$zip,Flow.Bundle.zip" `
    /reference:System.Windows.Forms.dll /reference:System.Drawing.dll /reference:System.IO.Compression.dll /reference:System.IO.Compression.FileSystem.dll /reference:System.Web.Extensions.dll `
    (Join-Path $root 'installer\Setup.cs') $info
if ($LASTEXITCODE -ne 0) { throw 'Setup compilation failed.' }
$sha=Sha $exe
[IO.File]::WriteAllText($exe+'.sha256',$sha+'  Flow-Setup.exe'+"`n")
Remove-Item $setupWork -Recurse -Force
Write-Host "Flow-Setup.exe: $((Get-Item $exe).Length) bytes, SHA256 $sha"
Write-Host "$($plugins.Count) plugins + $($panels.Count) panels, version $Version"
