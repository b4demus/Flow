# Состояние сборки / Build status

**Preview · 2026-10-21 · 41 AEX + CLI**

Наличие эффекта в наборе не означает завершённость всех его функций.
Этот документ описывает ограничения текущего релиза.

An included effect may still have unfinished features. This document describes
the current preview release, rather than promising production readiness.

| Область / Area | Текущий статус / Current status |
|---|---|
| Платформа / Platform | Windows x64. Проверено в AE 2023; другие версии отдельно не сертифицированы. / Tested in AE 2023; other host versions are not separately validated. |
| Рендер / Rendering | CPU, SmartFX. Собственного CUDA-ускорения нет. / CPU rendering with SmartFX; no CUDA acceleration. |
| PixelSort | Дорабатываются вариации, дробные маски и пересечения путей. / Variations, fractional masks and overlapping paths remain under development. |
| PixelMosh | Дорабатываются коллизии поля движения, границы и некоторые сочетания масок. / Flow collisions, borders and some mask combinations remain under development. |
| WarpTransform | Экспериментальный; уменьшение изображения и прозрачность требуют дальнейшей проверки. / Experimental; minification and transparency need further validation. |
| BlurMoCurves | Ограничения у Bias, Subpixel Off и Half/Quarter resolution. / Known limitations with Bias, Subpixel Off and Half/Quarter resolution. |
| Vignette | Размытие виньетки пока отсутствует. / Vignette blur is not implemented yet. |
| Rays | Shimmer требует доработки, Atmosphere отсутствует. / Shimmer needs further work; Atmosphere is not implemented. |
| FilmDamage / FilmEffect | Некоторые дефекты и проекция ограничены; плёночные профили процедурные. / Some defects and projection features are limited; film profiles are procedural. |
| LensFlare | Три процедурные линзы, ограниченный набор настроек. / Three procedural lenses with a limited control set. |
| Маски / Masks | Отдельная интеграция Mocha, маски из Path и служебные Crop-контролы не включены. / Dedicated Mocha integration, Path masks and auxiliary Crop controls are not included. |
| Производительность / Performance | На больших кадрах PixelSort/PixelMosh могут быть медленными и требовать значительную память. / PixelSort/PixelMosh may be slow and memory intensive at high resolutions. |

## Проверки / Validation

- **57** тестов ядра прошли. / Core unit tests passed.
- **13** сценариев установки прошли в Windows PowerShell 5.1. / Installer integration scenarios passed in Windows PowerShell 5.1.
- Проверены свежая установка, обновление, старые имена файлов, дубликаты,
  повторный запуск, повреждение файлов, блокировки, открытый AE и откат при
  ошибке посреди обновления. / Coverage includes clean installs, upgrades,
  legacy filenames, duplicate consolidation, repeat installs, corrupt files,
  locked files, running AE and rollback after a mid-update write failure.
- Проверка встроенного пакета EXE подтверждает 41 плагин и SHA-256 пакета. /
  The EXE package check verifies all 41 plugins and the embedded package hash.

Эти проверки не являются обещанием, что все эффекты и параметры подходят
для любого проекта. / These checks do not guarantee every effect and parameter
combination for every project.
