# Checks the <core> part of Flowcut.jsx outside After Effects: JScript (cscript) runs
# the same ES3 code as ExtendScript. Signatures are built from a TIFF/PNG sequence
# the way the panel's AE probe sees them (Mosaic block averages, 32x18).
# Usage: run_tests.ps1 [-Frames <folder with an anime frame sequence>]
param([string]$Frames = (Join-Path $PSScriptRoot '..\..\..\flowtime_compare_data\src_anime'))
$ErrorActionPreference = 'Stop'

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System; using System.Drawing; using System.Drawing.Imaging; using System.Runtime.InteropServices; using System.Text;
public static class FlowcutSig {
  public static string Sig(string path, int gx, int gy) {
    using (var b = new Bitmap(path)) {
      int w = b.Width, h = b.Height;
      var d = b.LockBits(new Rectangle(0, 0, w, h), ImageLockMode.ReadOnly, PixelFormat.Format24bppRgb);
      var buf = new byte[d.Stride * h]; Marshal.Copy(d.Scan0, buf, 0, buf.Length); b.UnlockBits(d);
      var s = new double[gx * gy * 3]; var n = new int[gx * gy];
      for (int y = 0; y < h; y++) { int by = y * gy / h;
        for (int x = 0; x < w; x++) { int k = by * gx + x * gx / w, o = y * d.Stride + x * 3;
          s[k * 3] += buf[o + 2]; s[k * 3 + 1] += buf[o + 1]; s[k * 3 + 2] += buf[o]; n[k]++; } }
      var sb = new StringBuilder();
      for (int k = 0; k < n.Length; k++) {
        for (int c = 0; c < 3; c++) sb.Append(((int)Math.Round(s[k * 3 + c] / n[k])).ToString("x2"));
        sb.Append("ff");
      }
      return sb.ToString();
    }
  }
}
'@

$jsx = [IO.File]::ReadAllText((Join-Path $PSScriptRoot '..\Flowcut.jsx'))
$core = [regex]::Match($jsx, '(?s)// <core>[^\n]*\n(.*?)// </core>').Groups[1].Value
if (-not $core) { throw 'core markers not found in Flowcut.jsx' }

$files = Get-ChildItem $Frames -File | Where-Object { $_.Extension -match '^\.(tif|tiff|png|jpg)$' } | Sort-Object Name
if ($files.Count -lt 4) { throw "no frames in $Frames" }
$sigs = $files | ForEach-Object { '"' + [FlowcutSig]::Sig($_.FullName, 32, 18) + '"' }

$test = @"
var FULL_SRC = $(ConvertTo-Json -Compress $jsx);
$core
var REAL = [$($sigs -join ",`n")];
var failures = 0;
function check(name, ok, info) { WScript.Echo((ok ? "ok   " : "FAIL ") + name + (info ? "  (" + info + ")" : "")); if (!ok) failures++; }

// Whole panel file must at least compile as ES3.
var compiled = true; try { new Function(FULL_SRC); } catch (e) { compiled = e.message; }
check("Flowcut.jsx compiles", compiled === true, compiled === true ? "" : compiled);

function decode(hex) { var a = []; for (var i = 0; i < hex.length; i += 2) a.push(parseInt(hex.substr(i, 2), 16)); return a; }
function keptBy(sigs, threshold) {
    return pickKept(sigs.length, function (i, ref) { return frameDiff(sigs[i], sigs[ref]) > threshold; });
}
var real = []; for (var i = 0; i < REAL.length; i++) real.push(decode(REAL[i]));
check("signatures have one RGBA per block", real[0].length === GX * GY * 4);

// Real clip: camera moves every frame, nothing is a duplicate.
var keptReal = keptBy(real, 6);
check("real clip keeps every frame at threshold 6", keptReal.length === real.length, keptReal.length + "/" + real.length);

// Synthetic anime on 2s/3s with long holds, built from real drawings plus noise
// (compression/grain stand-in, +-3 levels per block).
var seed = 12345; function rnd() { seed = (seed * 1103515245 + 12345) % 2147483648; return seed / 2147483648; }
function noisy(s, amp) { var o = []; for (var i = 0; i < s.length; i++) { var v = s[i] + Math.round((rnd() * 2 - 1) * amp); o.push(v < 0 ? 0 : v > 255 ? 255 : v); } return o; }
var holds = [2, 2, 3, 2, 3, 3, 1, 2, 12, 2, 2, 3, 1, 1, 2, 3, 2, 2, 2, 4];
var seq = [], expect = [];
for (var d = 0; d < holds.length; d++) {
    expect.push(seq.length);
    for (var h = 0; h < holds[d]; h++) seq.push(noisy(real[d * 2], 3));
}
var kept = keptBy(seq, 6);
check("synthetic 2s/3s: keeps one frame per drawing", kept.join(",") === expect.join(","), "kept " + kept.length + ", drawings " + expect.length + ", frames " + seq.length);

// Slow fade below the threshold per frame must not collapse into one frame.
var fade = []; for (var f = 0; f < 30; f++) { var s = []; for (var q = 0; q < real[0].length; q++) s.push(Math.min(255, real[0][q] + f)); fade.push(s); }
var keptFade = keptBy(fade, 5);
check("slow fade is still kept periodically", keptFade.length === 5, "kept " + keptFade.join(","));

WScript.Quit(failures);
"@
$tmp = Join-Path $env:TEMP 'flowcut_core_test.js'
[IO.File]::WriteAllText($tmp, $test, [Text.Encoding]::Unicode)
cscript //nologo //E:jscript $tmp
$code = $LASTEXITCODE
if ($code -ne 0) { throw "$code test(s) failed" }
