# FlowCurves

Bezier easing curve editor for Adobe After Effects. One self-contained ScriptUI file, no dependencies.
Works as a dockable panel or a floating palette.

![](https://img.shields.io/badge/After%20Effects-2023%2B-9999ff)

## Features

- Drag two bezier handles on a graph that grows with the panel, apply to selected keyframes
- Works on position (spatial), scale, rotation, opacity, color, mask paths
- Reads the existing easing of selected keyframes back into the graph (**Get**)
- Presets (Sine / Quad / Cubic / Quart / Quint / Expo / Circ / Back + CSS ease) shown as thumbnails
- Save your own presets, right-click a custom preset to delete it
- Type or paste `0.25, 0.1, 0.25, 1` or `cubic-bezier(.4,0,.2,1)` into the field
- Out / In toggles, Auto-apply on release, Linear / Hold / Auto-bezier buttons
- Shift while dragging snaps to a 0.05 grid
- One undo step per apply

## Install (dockable panel)

FlowCurves ships with the **Flow** set and is installed by its installer (`Flow-Setup.exe` at the set root):
the panel is copied into `ScriptUI Panels` of every After Effects found and the former `ezcurves.jsx` is retired.
Restart After Effects, open **Window > FlowCurves.jsx**, drag the tab where you want it.
Manually: copy `FlowCurves.jsx` to
`C:\Program Files\Adobe\Adobe After Effects <version>\Support Files\Scripts\ScriptUI Panels\`.

Presets saved by ezcurves (`%APPDATA%\EZCurves\presets.txt`) are copied over on first start.

Without installing: **File > Scripts > Run Script File...** opens it as a floating palette.

Enable **Preferences > Scripting & Expressions > Allow Scripts to Write Files and Access Network**
to be able to save custom presets (stored in `%APPDATA%\FlowCurves\presets.txt`).

## Usage

1. Select 2+ keyframes (a single selected keyframe is eased towards the next one).
2. Shape the curve or click a preset, press **Apply** (or leave **Auto** on).

## Notes

- Spatial speed in After Effects is a magnitude, so overshoot with negative speed (e.g. Back In)
  can't be fully stored on Position; the panel tells you when that happens.
- Tested on After Effects 2023 (23.4).
