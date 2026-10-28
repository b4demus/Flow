<div align="center">

![Flowcut — unique drawings, ready for retiming](docs/assets/banner.svg)

**English** · [Русский](README.ru.md)

![After Effects](https://img.shields.io/badge/Adobe-After%20Effects-9999FF?style=flat-square)
![ExtendScript](https://img.shields.io/badge/ExtendScript-ES3-F0DB4F?style=flat-square)
![Installer](https://img.shields.io/badge/Installer-Windows-58A6FF?style=flat-square)

**Remove repeated frames. Keep the drawings. Get ready to retime.**

A small ScriptUI panel for Adobe After Effects that collapses repeated frames in anime and limited animation, preparing unique drawings for optical-flow tools such as Twixtor.

[Installation](#installation) · [Workflow](#workflow) · [How it works](#how-it-works)

</div>

## What it does

Animation on twos or threes repeats each drawing across several video frames. Flowcut compares each frame with the last retained frame, removes near-duplicates, and places the retained source frames on consecutive composition frames using hold Time Remap keys.

```text
Before   A A B B B C C D D D
After    A   B     C   D
Output   A B C D              → ready for retiming
```

| Feature | Behavior |
| --- | --- |
| Batch processing | Process the selected video layers in one run. |
| Adjustable threshold | Tune sensitivity from **1–40**, with **6** as the default. |
| Restore | Disable Flowcut's Time Remap and restore the original layer out point. |
| Repeatable workflow | Running Flowcut again on a processed layer restores it before analyzing it again. |
| One-step Undo | Each processing run is grouped into a single After Effects Undo action. |
| Native processing | Frame analysis uses After Effects effects and expressions; no external frame-analysis tool is required. |

## Installation

Flowcut ships with the **Flow** set and is installed by its installer (`Flow-Setup.exe` at the set root): the
panel is copied into `ScriptUI Panels` of every After Effects found, and the former `Flowcut.jsx` is retired.
After restarting AE: **Window → Flowcut.jsx**. Manually: copy `Flowcut.jsx` to
`C:\Program Files\Adobe\Adobe After Effects <version>\Support Files\Scripts\ScriptUI Panels\`
or run it once through **File → Scripts → Run Script File…**.

Layers processed by the former Flowcut panel (`[flowcut out=…]` comment) are recognised by Flowcut and can be restored.

## Workflow

1. Open a composition and select one or more video layers.
2. Start with threshold **6** and click **Вырезать дубли** (*Remove duplicates*).
3. Flowcut analyzes each layer's visible source range, creates hold Time Remap keys, and shortens the layer to the retained frame count.
4. Apply Twixtor or another retiming tool to the result, or precompose the result first.
5. To restore selected processed layers, click **Вернуть** (*Restore*).

The current panel interface is in Russian:

| Control | English meaning |
| --- | --- |
| **Порог** | Threshold |
| **Вырезать дубли** | Remove duplicates |
| **Вернуть** | Restore |

### Choosing a threshold

The threshold measures a change in block-average color on a **0–255** scale. A frame is retained when the largest detected block/channel difference exceeds the threshold.

| Result | Adjustment |
| --- | --- |
| Distinct drawings are being removed | **Lower** the threshold to retain smaller changes. |
| Repeated drawings survive because of noise or compression | **Raise** the threshold to ignore more small changes. |

Settings persist between sessions. Compare a short sample before processing a longer clip.

## How it works

1. Temporary **Flowcut diff** and **Flowcut probe** compositions compare a candidate frame with the last retained source frame.
2. **Mosaic** reduces the comparison to a **32 × 18** grid of block averages.
3. **Difference** and **Levels** isolate changes above the threshold.
4. A `sampleImage` expression reads the result. Comparing against the last retained frame lets gradual changes accumulate instead of losing them entirely.
5. Retained frames become consecutive hold Time Remap keys at the destination composition's frame rate. Temporary analysis compositions are removed afterward.

Source footage is left intact. Flowcut changes the selected layer's Time Remap, out point, and comment; the comment stores a `[flowcut out=…]` restoration marker.

## Limits to keep in mind

- **Timing changes.** Long static holds collapse to one frame. Flowcut shortens the layer; it does not preserve the original timing, retime audio, or change the composition duration.
- **Camera motion counts as change.** A camera pan can make every frame distinct even when the character drawing repeats.
- **Layer effects and masks are excluded.** Analysis reads the layer's source before that layer's effects and masks. For a precomposition source, its internal rendered content is part of the source.
- **Existing retiming is protected.** Layers with unrelated Time Remap or Stretch other than 100% are skipped.
- **Restore depends on the marker.** Keep the `[flowcut out=…]` layer comment if you want to use the Restore button.
- **Processing takes time.** Analysis runs inside After Effects, so speed depends on source size, duration, and the host system.

Flowcut removes duplicates; use a separate retiming tool to generate interpolated frames.

## Tests

### Core logic · Windows

The existing test runner extracts the ES3 core from `Flowcut.jsx`, checks that the whole script compiles under JScript, and tests frame selection, noisy repeated drawings, and gradual fades.

It requires **Windows PowerShell 5.1**, `cscript`, and a local reference sequence of at least **40** consecutive TIFF, PNG, or JPG frames. The real-footage check expects every frame to be distinct at threshold 6. The original reference footage is not distributed in this repository.

From the repository root:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tests\run_tests.ps1 -Frames "C:\path\to\frames"
```

### Inside After Effects

`tests/ae_test.jsx` checks Time Remap keys, layer duration, probe cleanup, reprocessing, and restoration on generated animation.

> **Save your work first:** this test closes the current After Effects project **without saving** and creates a new test project. Enable **Allow Scripts to Write Files and Access Network** so it can write its log.

Run it through **File → Scripts → Run Script File…**. Results are written to `tests/ae_test_log.txt`. Additional real-footage checks run only when the sibling directory `../flowtime_compare_data/src_anime/frame_0000.tif` exists.

```text
cut/
├── Flowcut.jsx            # Panel, frame analysis, and Time Remap
├── README.md  README.ru.md
├── docs/assets/banner.svg # README artwork
└── tests/
    ├── run_tests.ps1      # Core logic checks with external frames
    └── ae_test.jsx        # After Effects integration checks
```
