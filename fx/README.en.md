<p align="center">
  <img src="docs/assets/facet-banner.svg" alt="Facet — Light, Motion, Color, Time" width="100%" />
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Windows-x64-8CC8FF?style=flat-square" alt="Windows x64" />
  <img src="https://img.shields.io/badge/After_Effects-AEX-B8A4FF?style=flat-square" alt="After Effects AEX" />
  <img src="https://img.shields.io/badge/Effects-41-A6F5E0?style=flat-square" alt="41 effects" />
  <img src="https://img.shields.io/badge/Core-Rust-E5AD6C?style=flat-square" alt="Rust core" />
  <img src="https://img.shields.io/badge/Release-Preview-CCD5E8?style=flat-square" alt="Preview release" />
</p>

<p align="center">
  <a href="README.md">Русский</a> · <b>English</b><br />
  <a href="https://github.com/b4demus/facet/releases/download/v0.1.0-preview.20261021/Facet-Setup-2026-10-21.exe">Download setup</a> · <a href="https://github.com/b4demus/facet/releases/tag/v0.1.0-preview.20261021">Release & ZIP</a> · <a href="INSTALL.en.md">Installation</a> · <a href="docs/status.md">Build status</a>
</p>

# Facet

**41 effects for After Effects — light, motion, color and time in one collection.**

Facet brings together glows and rays, camera shake and geometry, time effects,
film styling and digital distortion. Effects appear in the **Facet ...** menus
with the `F_` prefix. The collection includes native `.aex` plugins, an image
processing CLI, and an installer with upgrade and restore support.

## Get started

1. Download [**Facet Setup**](https://github.com/b4demus/facet/releases/download/v0.1.0-preview.20261021/Facet-Setup-2026-10-21.exe) from the [release](https://github.com/b4demus/facet/releases/tag/v0.1.0-preview.20261021).
2. Save your project and close After Effects.
3. Run the EXE, click **Установить / Install**, and accept the Windows prompt.
4. Start AE and find **Facet Lighting**, **Facet Time**, **Facet Stylize**, and the other categories.

The EXE contains the complete package; no adjacent ZIP is needed.
The current setup interface is in Russian. Alternatively, extract the ZIP
completely and run `install.cmd`. See [installation and restore](INSTALL.en.md).

## In the collection

| Area | What it covers |
|---|---|
| **Light** | Glows, colored halos, glints, rays and illuminated edges |
| **Motion** | Camera shake, motion blur, RGB displacement and geometry |
| **Color** | Duotone mapping, highlight extraction and color processing |
| **Time** | Freeze frames, reverse playback, channel time offsets and flicker tools |
| **Texture** | Film defects, grain, digital damage, PixelSort and PixelMosh |
| **Transitions** | Shake and blur transitions |

<details>
<summary><b>Full catalog — 41 effects</b></summary>

| AE category | Count | Effects |
|---|---:|---|
| Facet Adjust | 2 | `F_DuoTone`, `F_Hotspots` |
| Facet Blur and Sharpen | 2 | `F_BlurMoCurves`, `F_Sharpen` |
| Facet Distort | 4 | `F_DistortRGB`, `F_Shake`, `F_WarpChroma`, `F_WarpTransform` |
| Facet Lighting | 8 | `F_EdgeRays`, `F_Glint`, `F_Glow`, `F_GlowDarks`, `F_GlowEdges`, `F_GlowRainbow`, `F_LensFlare`, `F_Rays` |
| Facet Stylize | 6 | `F_DigitalDamage`, `F_FilmDamage`, `F_FilmEffect`, `F_PixelMosh`, `F_PixelSort`, `F_Vignette` |
| Facet Time | 17 | `F_Flicker`, `F_FlickerMatch`, `F_FlickerMatchColor`, `F_FlickerMatchMatte`, `F_FlickerMchMatteColor`, `F_FlickerRemove`, `F_FlickerRemoveColor`, `F_FlickerRemoveMatte`, `F_FlickerRmMatteColor`, `F_FreezeFrame`, `F_GetFrame`, `F_JitterFrames`, `F_RandomEdits`, `F_ReverseClip`, `F_ReverseEdits`, `F_TimeAverage`, `F_TimeWarpRGB` |
| Facet Transitions | 2 | `F_DissolveBlur`, `F_DissolveShake` |

</details>

## Installation without duplicates

Setup verifies file hashes and internal plugin identities, consolidates updates
into one directory, and backs up the original files for restore. Legacy filenames
are recognized. Reinstalling the current version does not create extra copies.
Open Adobe applications are never closed automatically.

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\Facet
```

Backups are stored in `C:\ProgramData\Facet\Backups`.
Step-by-step instructions: [INSTALL.en.md](INSTALL.en.md).

## Current status

This is a **Windows x64 preview**, tested in **AE 2023**.
Rendering runs on the CPU through SmartFX; some effects can be slow at
high resolutions.

- **57** core unit tests passed.
- **13** installer and restore scenarios passed in Windows PowerShell 5.1.
- PixelSort and PixelMosh remain under development; WarpTransform is experimental.
- Lens options, some film features and certain blur modes are currently limited.

For the details, see [effect status](docs/status.md).

## Development

The processing core is written in **Rust**. AE integration uses SmartFX;
the installer uses PowerShell and C#/.NET Framework. The validated build
environment is Windows x64 with Rust GNU and GCC/MinGW-w64, including `windres`.

```powershell
# Build 41 AEX plugins and the CLI into dist/
powershell.exe -NoProfile -ExecutionPolicy Bypass -File build.ps1

# Test the core
cargo test --locked -p facet-core

# Generate package metadata and test setup against isolated plugin trees
python tools/write_installer_manifest.py
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/test_installer.ps1
```

`tools/env.ps1` uses a toolchain on PATH or a local portable toolchain configured
through `PETAL_TOOLCHAIN`. Rust is not required to run the packaged installer.

| Directory | Contents |
|---|---|
| `core/` | Image, color, geometry and time processing |
| `ae/` | Plugins and shared After Effects integration |
| `cli/` | Image processing, comparison and diagnostic commands |
| `installer/` | Package checks, upgrades, backups and setup UI |
| `tools/` | Build, packaging and installer validation tools |
| `third_party/` | Third-party notices and license terms |

## Licenses

Project code is [MIT-licensed](LICENSE). Third-party components retain their
own terms: [IJG](third_party/ijg/README),
[adaptation notes](third_party/ijg/FACET.md).

This software is based in part on the work of the Independent JPEG Group.
