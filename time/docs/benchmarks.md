# Flowtime · Benchmarks / Замеры

[Русский README](../README.md) · [English README](../README.en.md)

## Русский

Локальные результаты из исходной документации проекта, записанные 18 октября 2026 года. Это замеры на двух отдельных клипах: время зависит от системы и настроек, а качество — от материала. Они не являются общей оценкой превосходства одного движка над другим.

### Методика

В After Effects из исходной TIFF-последовательности оставляется каждый N-й кадр. Прореженная последовательность замедляется до исходной длительности с помощью Flowtime и Twixtor Pro. Синтезированные кадры сравниваются с настоящими по PSNR и SSIM; исходные опорные кадры исключаются из среднего качества. Для эффектов используются `Best`, `Blend`, `Inverse`.

### Записанные результаты

| Материал | Движок | Рендер | PSNR ↑ | SSIM ↑ |
| :--- | :--- | ---: | ---: | ---: |
| Аниме · 40 кадров · 1920 × 800 · шаг 4 | Flowtime 0.1 | 3.2 с | 20.08 dB | 0.769 |
| | Twixtor Pro 7 | 4.0 с | 19.49 dB | 0.756 |
| Тёмная сцена с дымом · 71 кадр · 1276 × 720 · шаг 8 | Flowtime 0.1 | 2.3 с | 34.56 dB | 0.906 |
| | Twixtor Pro 7 | 5.5 с | 34.44 dB | 0.906 |

### Повторение сравнения

Нужны After Effects, лицензированный Twixtor Pro, собранный Flowtime и собственная TIFF-последовательность в `compare/src/`. Исходные клипы и рендеры не входят в репозиторий. Скрипты настроены на After Effects 2023; путь к хосту указан в `tools/ae/restart_ae.ps1`.

**Скрипт закрывает и перезапускает After Effects, не сохраняя текущий проект. Сохраните работу перед запуском.**

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\ae\run_compare.ps1 -Step 4
```

Добавьте `-Install`, чтобы перед сравнением обновить установленный плагин из `dist/`. Для второго сценария используется `-Step 8`.

## English

These local results were recorded in the original project documentation on October 18, 2026. They cover two individual clips: timings depend on the system and settings, and quality depends on the footage. They do not establish general superiority of either engine.

### Method

Inside After Effects, retain every Nth frame of a source TIFF sequence, then slow the reduced sequence back to its original duration using Flowtime and Twixtor Pro. Compare synthesized frames against the real frames using PSNR and SSIM; exclude retained source frames from the quality averages. Both effects use `Best`, `Blend`, and `Inverse`.

### Recorded results

| Footage | Engine | Render | PSNR ↑ | SSIM ↑ |
| :--- | :--- | ---: | ---: | ---: |
| Anime · 40 frames · 1920 × 800 · step 4 | Flowtime 0.1 | 3.2 s | 20.08 dB | 0.769 |
| | Twixtor Pro 7 | 4.0 s | 19.49 dB | 0.756 |
| Dark, smoky live action · 71 frames · 1276 × 720 · step 8 | Flowtime 0.1 | 2.3 s | 34.56 dB | 0.906 |
| | Twixtor Pro 7 | 5.5 s | 34.44 dB | 0.906 |

### Reproduce the comparison

You need After Effects, a licensed Twixtor Pro installation, a built Flowtime plugin, and your own TIFF sequence in `compare/src/`. Source footage and renders are excluded from the repository. Scripts target After Effects 2023; the host path is set in `tools/ae/restart_ae.ps1`.

**The script closes and restarts After Effects, discarding the current project without saving. Save your work before running it.**

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\ae\run_compare.ps1 -Step 4
```

Add `-Install` to update the installed plugin from `dist/` before comparison. Use `-Step 8` for the second scenario.
