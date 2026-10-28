# Builds the facet AE effects and the CLI into dist\.
# Toolchain: see tools\env.ps1 (portable windows-gnu Rust from ..\.tools\rust-daw).
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'tools\env.ps1')

$effects = @(
    @{ crate = 'facet-glow'; dll = 'facet_glow.dll'; aex = 'F_Glow.aex' }
    @{ crate = 'facet-glowdarks'; dll = 'facet_glowdarks.dll'; aex = 'F_GlowDarks.aex' }
    @{ crate = 'facet-glowrainbow'; dll = 'facet_glowrainbow.dll'; aex = 'F_GlowRainbow.aex' }
    @{ crate = 'facet-glint'; dll = 'facet_glint.dll'; aex = 'F_Glint.aex' }
    @{ crate = 'facet-edgerays'; dll = 'facet_edgerays.dll'; aex = 'F_EdgeRays.aex' }
    @{ crate = 'facet-shake'; dll = 'facet_shake.dll'; aex = 'F_Shake.aex' }
    @{ crate = 'facet-dissolveshake'; dll = 'facet_dissolveshake.dll'; aex = 'F_DissolveShake.aex' }
    @{ crate = 'facet-flicker'; dll = 'facet_flicker.dll'; aex = 'F_Flicker.aex' }
    @{ crate = 'facet-flickerremove'; dll = 'facet_flickerremove.dll'; aex = 'F_FlickerRemove.aex' }
    @{ crate = 'facet-flickerremovecolor'; dll = 'facet_flickerremovecolor.dll'; aex = 'F_FlickerRemoveColor.aex' }
    @{ crate = 'facet-flickerremovematte'; dll = 'facet_flickerremovematte.dll'; aex = 'F_FlickerRemoveMatte.aex' }
    @{ crate = 'facet-flickerrmmattecolor'; dll = 'facet_flickerrmmattecolor.dll'; aex = 'F_FlickerRmMatteColor.aex' }
    @{ crate = 'facet-flickermatch'; dll = 'facet_flickermatch.dll'; aex = 'F_FlickerMatch.aex' }
    @{ crate = 'facet-flickermatchcolor'; dll = 'facet_flickermatchcolor.dll'; aex = 'F_FlickerMatchColor.aex' }
    @{ crate = 'facet-flickermatchmatte'; dll = 'facet_flickermatchmatte.dll'; aex = 'F_FlickerMatchMatte.aex' }
    @{ crate = 'facet-flickermchmattecolor'; dll = 'facet_flickermchmattecolor.dll'; aex = 'F_FlickerMchMatteColor.aex' }
    @{ crate = 'facet-freezeframe'; dll = 'facet_freezeframe.dll'; aex = 'F_FreezeFrame.aex' }
    @{ crate = 'facet-getframe'; dll = 'facet_getframe.dll'; aex = 'F_GetFrame.aex' }
    @{ crate = 'facet-jitterframes'; dll = 'facet_jitterframes.dll'; aex = 'F_JitterFrames.aex' }
    @{ crate = 'facet-randomedits'; dll = 'facet_randomedits.dll'; aex = 'F_RandomEdits.aex' }
    @{ crate = 'facet-reverseclip'; dll = 'facet_reverseclip.dll'; aex = 'F_ReverseClip.aex' }
    @{ crate = 'facet-reverseedits'; dll = 'facet_reverseedits.dll'; aex = 'F_ReverseEdits.aex' }
    @{ crate = 'facet-timewarprgb'; dll = 'facet_timewarprgb.dll'; aex = 'F_TimeWarpRGB.aex' }
    @{ crate = 'facet-timeaverage'; dll = 'facet_timeaverage.dll'; aex = 'F_TimeAverage.aex' }
    @{ crate = 'facet-duotone'; dll = 'facet_duotone.dll'; aex = 'F_DuoTone.aex' }
    @{ crate = 'facet-vignette'; dll = 'facet_vignette.dll'; aex = 'F_Vignette.aex' }
    @{ crate = 'facet-warptransform'; dll = 'facet_warptransform.dll'; aex = 'F_WarpTransform.aex' }
    @{ crate = 'facet-glowedges'; dll = 'facet_glowedges.dll'; aex = 'F_GlowEdges.aex' }
    @{ crate = 'facet-dissolveblur'; dll = 'facet_dissolveblur.dll'; aex = 'F_DissolveBlur.aex' }
    @{ crate = 'facet-distortrgb'; dll = 'facet_distortrgb.dll'; aex = 'F_DistortRGB.aex' }
    @{ crate = 'facet-warpchroma'; dll = 'facet_warpchroma.dll'; aex = 'F_WarpChroma.aex' }
    @{ crate = 'facet-sharpen'; dll = 'facet_sharpen.dll'; aex = 'F_Sharpen.aex' }
    @{ crate = 'facet-blurmocurves'; dll = 'facet_blurmocurves.dll'; aex = 'F_BlurMoCurves.aex' }
    @{ crate = 'facet-rays'; dll = 'facet_rays.dll'; aex = 'F_Rays.aex' }
    @{ crate = 'facet-filmdamage'; dll = 'facet_filmdamage.dll'; aex = 'F_FilmDamage.aex' }
    @{ crate = 'facet-digitaldamage'; dll = 'facet_digitaldamage.dll'; aex = 'F_DigitalDamage.aex' }
    @{ crate = 'facet-filmeffect'; dll = 'facet_filmeffect.dll'; aex = 'F_FilmEffect.aex' }
    @{ crate = 'facet-lensflare'; dll = 'facet_lensflare.dll'; aex = 'F_LensFlare.aex' }
    @{ crate = 'facet-hotspots'; dll = 'facet_hotspots.dll'; aex = 'F_Hotspots.aex' }
    @{ crate = 'facet-pixelsort'; dll = 'facet_pixelsort.dll'; aex = 'F_PixelSort.aex' }
    @{ crate = 'facet-pixelmosh'; dll = 'facet_pixelmosh.dll'; aex = 'F_PixelMosh.aex' }
)

Push-Location $PSScriptRoot
try {
    $pkgs = @('-p', 'facet-cli') + ($effects | ForEach-Object { '-p', $_.crate })
    cargo build --release @pkgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    New-Item -ItemType Directory -Force dist | Out-Null
    foreach ($e in $effects) { Copy-Item "target\release\$($e.dll)" "dist\$($e.aex)" -Force }
    Copy-Item target\release\facet.exe dist\facet.exe -Force
    Get-ChildItem dist | ForEach-Object { Write-Host "dist\$($_.Name)" }
} finally {
    Pop-Location
}
