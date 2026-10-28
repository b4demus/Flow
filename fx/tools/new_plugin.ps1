# Creates a thin plugin crate ae/<dir> that includes a shared body
# (ae/<family>/common.rs) with its Kind, and registers it in Cargo.toml and build.ps1.
# Usage: new_plugin.ps1 -Dir duotone -Name F_DuoTone -Kind DuoTone -Category "Facet Adjust"
#        [-Family fx] [-TimeVarying] [-WideTime] [-About "..."]
param([Parameter(Mandatory)][string]$Dir, [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string]$Kind,
      [Parameter(Mandatory)][string]$Category, [string]$Family = 'fx', [switch]$TimeVarying, [switch]$WideTime, [string]$About = '')
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$d = Join-Path $root "ae\$Dir"
New-Item -ItemType Directory -Force "$d\src" | Out-Null
$crate = "facet-$Dir"
$utf8 = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText("$d\Cargo.toml", @"
[package]
name = "$crate"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["cdylib"]

[dependencies]
after-effects = "0.4"
facet-core = { path = "../../core" }
facet-ae-util = { path = "../util" }

[build-dependencies]
pipl = "0.1"
"@, $utf8)
# matchName keeps the old "B4DM Facet <words>" form
$words = ($Name -replace '^F_', '') -creplace '([a-z0-9])([A-Z])', '$1 $2'
$tv = if ($TimeVarying) { 'true' } else { 'false' }
$wide = if ($WideTime) { 'true' } else { 'false' }
[IO.File]::WriteAllText("$d\build.rs", @"
include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../$Family");
    facet_plugin_ex("$Name", "B4DM Facet $words", "$Category", $tv, $wide);
}
"@, $utf8)
[IO.File]::WriteAllText("$d\src\lib.rs", @"
//! $Name. Shared body: ae/$Family/common.rs.

use after_effects as ae;

#[path = "../../$Family/common.rs"]
mod common;
use common::{Kind, Params, P};

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut P, _: InData, _: OutData) -> Result<(), Error> {
        common::params_setup(params, Kind::$Kind)
    }

    fn handle_command(&mut self, cmd: ae::Command, in_data: InData, out_data: OutData, params: &mut P) -> Result<(), ae::Error> {
        common::handle_command(cmd, in_data, out_data, params, Kind::$Kind, "$Name v0.1\r$About\rb4demus")
    }
}
"@, $utf8)
$cargo = Join-Path $root 'Cargo.toml'
$t = [IO.File]::ReadAllText($cargo)
if ($t -notmatch "`"ae/$Dir`"") { $t = $t -replace '\]\s*\r?\n\r?\n\[workspace.package\]', ", `"ae/$Dir`"]`r`n`r`n[workspace.package]"; [IO.File]::WriteAllText($cargo, $t, $utf8) }
$bp = Join-Path $root 'build.ps1'
$b = [IO.File]::ReadAllText($bp)
if ($b -notmatch "'$crate'") {
    $line = "    @{ crate = '$crate'; dll = 'facet_$($Dir).dll'; aex = '$Name.aex' }"
    $b = [regex]::Replace($b, "(@\{ crate = 'facet-[a-z0-9]+'; dll = '[^']+'; aex = '[^']+' \})(?![\s\S]*@\{ crate = 'facet-)", "`$1`r`n$line")
    [IO.File]::WriteAllText($bp, $b, $utf8)
}
"created $crate ($Name, Kind::$Kind)"
