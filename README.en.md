<p align="center">
  <img src="docs/assets/flow-banner.svg" alt="Flow — light, motion, color, time. 42 effects and 2 panels for After Effects." width="100%" />
</p>

<p align="center">
  <a href="README.md">Русский</a> · <b>English</b>
</p>

<p align="center">
  <a href="https://github.com/b4demus/Flow/releases/latest"><img src="https://img.shields.io/github/v/release/b4demus/Flow?include_prereleases&amp;style=flat-square&amp;color=77efcf&amp;label=release" alt="GitHub release" /></a>
  <img src="https://img.shields.io/badge/Windows-x64-8fb5ff?style=flat-square" alt="Windows x64" />
  <img src="https://img.shields.io/badge/Adobe-After%20Effects-baa0ff?style=flat-square" alt="Adobe After Effects" />
  <img src="https://img.shields.io/badge/Rust_%2B_ExtendScript-toolkit-eab18b?style=flat-square" alt="Rust and ExtendScript" />
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-77efcf?style=flat-square" alt="MIT License" /></a>
</p>

<p align="center">
  <a href="https://github.com/b4demus/Flow/releases/latest/download/Flow-Setup.exe"><b>Download installer</b></a> ·
  <a href="https://github.com/b4demus/Flow/releases/latest">Release & ZIP</a> ·
  <a href="#installation">Installation</a> ·
  <a href="#the-collection">The collection</a> ·
  <a href="#build-from-source">Build</a>
</p>

# Flow

**42 effects, two panels, one toolkit for Adobe After Effects.**

Glows and rays, camera shake, color distortion, film damage, retiming, and animation curves. Flow brings native `.aex` plugins and ScriptUI panels together in one Windows x64 installer.

| Tool | What it does |
| :--- | :--- |
| **Flow FX** · 41 effects | Light, motion, color, styling, and transitions. Effect names start with `F_`. |
| **Flowtime** · 1 effect | Optical-flow slow motion, speed changes, and in-between frames. |
| **Flowcut** · panel | Removes repeated frames from anime and limited animation before retiming. |
| **FlowCurves** · panel | Bézier easing editor with draggable handles, built-in and custom presets, and keyframe application. |

> **Status: Windows x64 preview.** Rendering runs on the CPU. PixelSort and PixelMosh are still in development; WarpTransform is experimental. Flowtime supports 8/16 bpc; GPU processing and 32 bpc are not implemented yet. See [effect status](fx/docs/status.md) and [Flowtime limitations](time/README.en.md#limitations-in-v01).

## Installation

1. Download [**Flow-Setup.exe**](https://github.com/b4demus/Flow/releases/latest/download/Flow-Setup.exe).
2. Save your project and close After Effects and other Adobe applications.
3. Run setup, click **Установить** (*Install*), and accept the administrator prompt.
4. Open After Effects: effects appear under **Effect → Flow …**; panels are under **Window → Flowcut.jsx / FlowCurves.jsx**.

The EXE contains the complete toolkit; no separate ZIP is required. Rust and build tools are not needed to install it. The setup interface is in Russian.

To save custom FlowCurves presets, enable **Preferences → Scripting & Expressions → Allow Scripts to Write Files and Access Network**.

<details>
<summary><b>ZIP installation, file locations, and package verification</b></summary>

Download the ZIP from the [release page](https://github.com/b4demus/Flow/releases/latest), extract it **completely**, and run `install.cmd`.

Verify the package and preview the installation plan:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Check
```

| Component | Destination |
| :--- | :--- |
| All 42 `.aex` plugins | `C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\Flow` |
| Both `.jsx` panels | `Support Files\Scripts\ScriptUI Panels` in every detected After Effects installation |
| Backups of replaced files | `C:\ProgramData\Flow\Backups` |

Before making changes, setup verifies SHA-256 hashes and internal plugin identities, backs up replaced files, and rolls back if installation fails. Installing the same package again creates no duplicates or extra backups.

Release checksums include `Flow-Setup.exe.sha256` for the EXE and `SHA256SUMS.txt` for the files in `dist/`.

</details>

## The collection

| After Effects category | Count | Includes |
| :--- | ---: | :--- |
| **Flow Adjust** | 2 | Duotone mapping and highlight extraction |
| **Flow Blur and Sharpen** | 2 | Motion blur and sharpening |
| **Flow Distort** | 4 | Camera shake, geometry, and chromatic distortion |
| **Flow Lighting** | 8 | Glows, rays, glints, and illuminated edges |
| **Flow Stylize** | 6 | Film looks, digital damage, PixelSort, PixelMosh, and vignette |
| **Flow Time** | 18 | Flowtime, flicker, freeze frames, reverse playback, and time offsets |
| **Flow Transitions** | 2 | Blur and shake transitions |

<details>
<summary><b>All 42 effects by name</b></summary>

| Category | Effects |
| :--- | :--- |
| Adjust | `F_DuoTone`, `F_Hotspots` |
| Blur and Sharpen | `F_BlurMoCurves`, `F_Sharpen` |
| Distort | `F_DistortRGB`, `F_Shake`, `F_WarpChroma`, `F_WarpTransform` |
| Lighting | `F_EdgeRays`, `F_Glint`, `F_Glow`, `F_GlowDarks`, `F_GlowEdges`, `F_GlowRainbow`, `F_LensFlare`, `F_Rays` |
| Stylize | `F_DigitalDamage`, `F_FilmDamage`, `F_FilmEffect`, `F_PixelMosh`, `F_PixelSort`, `F_Vignette` |
| Time | `Flowtime`, `F_Flicker`, `F_FlickerMatch`, `F_FlickerMatchColor`, `F_FlickerMatchMatte`, `F_FlickerMchMatteColor`, `F_FlickerRemove`, `F_FlickerRemoveColor`, `F_FlickerRemoveMatte`, `F_FlickerRmMatteColor`, `F_FreezeFrame`, `F_GetFrame`, `F_JitterFrames`, `F_RandomEdits`, `F_ReverseClip`, `F_ReverseEdits`, `F_TimeAverage`, `F_TimeWarpRGB` |
| Transitions | `F_DissolveBlur`, `F_DissolveShake` |

</details>

## Quick start

- **Light and style:** apply `F_Glow`, `F_Rays`, or `F_FilmEffect` to a layer from **Effect → Flow …**.
- **Slow motion:** add **Flow Time → Flowtime**, set `Input: Frame Rate` to the source frame rate, and set `Speed % = 25` for four-times slow motion.
- **Remove duplicates:** select a video layer, open **Flowcut**, start at threshold `6`, and click **Вырезать дубли** (*Remove duplicates*). The panel collapses repeated drawings and shortens the layer; retime the result afterward.
- **Shape easing:** select keyframes, open **FlowCurves**, choose a preset or edit the curve, and click **Apply**.

## Migrating from the earlier projects

| Earlier project | In Flow |
| :--- | :--- |
| Facet | Flow FX: the same `F_…` names, under `Flow …` categories |
| Flowtime | Flowtime under **Flow Time** |
| Dedupe | Flowcut |
| ezcurves | FlowCurves |

**Projects using old Facet effects need manual migration.** Internal identifiers changed from `B4DM Facet …` to `B4DM Flow …`, so AE reports the old effects as missing. Save a separate copy of your project before updating: you need to re-apply the effects and transfer their settings.

Setup removes recognized legacy Facet, Flowtime, Dedupe, and ezcurves files after backing them up. The Dedupe threshold and ezcurves state and presets migrate automatically. Flowcut recognizes layers processed by Dedupe and can restore them.

## Build from source

Requirements: **Windows x64**, **Rust stable with the `x86_64-pc-windows-gnu` host**, **MinGW-w64** including `windres`, and **.NET Framework 4.x** with its C# compiler for setup.

```powershell
git clone https://github.com/b4demus/Flow.git
cd Flow
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\build.ps1
```

Scripts use `cargo` from `PATH`. For a portable environment, set `PETAL_TOOLCHAIN`; the expected layout is `cargo/`, `rustup/`, and `gcc/w64devkit/`. Scripts also check `../.tools/rust-daw` relative to the Flow root.

The build creates plugins and panels in `dist/`, `bundle.json`, checksums, `work/Flow-<version>.zip`, and `Flow-Setup.exe`. The CLIs remain in `fx/dist/` and `time/dist/`. To repackage existing binaries:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\build.ps1 -SkipRust
```

Validate setup against isolated copies of the Adobe directory layout:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\test_installer.ps1
```

Tests use `work/installer-tests/`. Additional core test commands are documented in [Flow FX](fx/README.en.md) and [Flowtime](time/README.en.md).

## Source and documentation

```text
Flow/
├── fx/           41 effects, Rust core, AE integration, and CLI
├── time/         Flowtime: optical flow, retiming, and CLI
├── cut/          Flowcut: duplicate frame analysis and removal
├── curves/       FlowCurves: Bézier easing and presets
├── installer/    shared PowerShell and C# installer
├── tools/        packaging and installer tests
├── docs/assets/  README artwork
└── build.ps1     build the complete toolkit
```

[Flow FX](fx/README.en.md) · [Flowtime](time/README.en.md) · [Flowcut](cut/README.md) · [FlowCurves](curves/README.md) · [Report an issue](https://github.com/b4demus/Flow/issues)

Component documentation retains the history of the earlier projects. This README contains the current installation instructions for the complete toolkit.

## License

[MIT](LICENSE) · © 2026 b4demus. Third-party components retain their own terms: [Independent JPEG Group](third_party/ijg/README), [adaptation notes](third_party/ijg/FLOW.md), and [notices](installer/THIRD-PARTY-NOTICES.txt).

This software is based in part on the work of the Independent JPEG Group.
