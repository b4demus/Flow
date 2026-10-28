> **Часть набора Flow.** Flowtime входит в набор Flow (категория Flow Time) и ставится общим установщиком из корня набора
> (см. [README](../README.md)); отдельный `install.cmd` из этой папки убран: он клал плагин в папку `Flowtime`.

<div align="center">

![Flowtime — optical-flow retiming](docs/assets/flowtime-banner.svg)

**Плавное замедление, управление скоростью и промежуточные кадры.**<br>
Плагин для After Effects и CLI на одном движке оптического потока, написанном на Rust.

[![Version](https://img.shields.io/badge/version-0.1.0-66e3c4?style=flat-square)](Cargo.toml)
[![Rust](https://img.shields.io/badge/built_with-Rust-ea9b6b?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Windows](https://img.shields.io/badge/platform-Windows_x64-8cb4ff?style=flat-square)](#сборка)
[![After Effects](https://img.shields.io/badge/host-After_Effects-b6a3ff?style=flat-square)](#установка-в-after-effects)
[![MIT License](https://img.shields.io/badge/license-MIT-66e3c4?style=flat-square)](LICENSE)

**Русский** · [English](README.en.md)

[Возможности](#возможности) · [Сборка](#сборка) · [Установка](#установка-в-after-effects) · [CLI](#cli) · [Документация](#документация)

</div>

---

Flowtime создаёт промежуточные кадры по движению между исходными изображениями. Используйте его для slow motion, анимированных изменений скорости и преобразования частоты кадров — внутри After Effects или при обработке последовательностей изображений из командной строки.

**Статус: v0.1.0, ранняя версия.** Основной путь сборки — Windows x64. Движок работает на CPU; актуальные ограничения перечислены ниже.

## Возможности

| | Что умеет Flowtime |
| :--- | :--- |
| **Ретайминг** | Режимы `Speed %` и `Frame Num`, анимация скорости в AE, раздельная входная и выходная частота кадров. |
| **Оптический поток** | Пять уровней качества, включая отключение векторов, подготовка изображения и кэширование рассчитанного движения. |
| **Интерполяция** | `Nearest`, `Blend`, `Motion Weighted Blend`; четыре режима warping, в том числе `Inverse w/ Smart Blend`. |
| **Работа со слоями** | Фон + до трёх передних планов по маттам, чувствительность для каждого слоя, `Inv Matte Shrink`. |
| **Контроль движения** | До 12 трек-точек на слой, альтернативные источники цвета и движения, маркировка монтажных склеек. |
| **After Effects** | 8 и 16 bpc, multi-frame rendering, просмотр исходника и отдельных слоёв, отображение трек-точек. |
| **CLI** | Входные PNG, JPEG и TIFF, выходные PNG; генерация тестовых кадров, контактных листов и оценка PSNR/SSIM. |

Компенсация motion blur реализована для ускорения. Вычислительное ядро использует DIS optical flow, двунаправленную оценку движения и параллельную обработку через Rayon.

## Сборка

Понадобятся Git, **Rust stable с GNU-хостом `x86_64-pc-windows-gnu`** и **MinGW-w64** (например, w64devkit). Команды ниже выполняются в PowerShell.

```powershell
git clone https://github.com/b4demus/flowtime.git
cd flowtime
powershell -ExecutionPolicy Bypass -File .\build.ps1
```

Скрипт использует `cargo` из `PATH`. Для portable toolchain он также проверяет переменную `PETAL_TOOLCHAIN`, затем папку `..\.tools\rust-daw`. Ожидаемая структура portable toolchain: `cargo\`, `rustup\`, `gcc\w64devkit\`.

Результат:

```text
dist/
├── Flowtime.aex   # плагин After Effects
└── flowtime.exe   # утилита командной строки
```

## Установка в After Effects

После сборки запустите из корня проекта:

```powershell
.\install.cmd
```

Установщик запросит права администратора и скопирует `dist\Flowtime.aex` в общую папку плагинов Adobe:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\Flowtime
```

Перезапустите After Effects. Эффект доступен в меню **Effect → Time → Flowtime**.

Чтобы замедлить клип в четыре раза, примените эффект, укажите частоту исходника в `Input: Frame Rate` и задайте `Speed % = 25`. Для переменной скорости анимируйте `Speed %`; для прямого управления позицией выберите `Time Remap Mode → Frame Number`.

## CLI

### Замедление до 25 %

```powershell
.\dist\flowtime.exe .\input .\output --speed 25 --quality best --warp smart
```

Входная папка должна содержать как минимум два кадра одинакового размера. Файлы сортируются по имени: используйте нумерацию с ведущими нулями, например `frame_00001.png`. Результат сохраняется в PNG с глубиной 8 бит на канал.

### Преобразование 24 → 60 fps

```powershell
.\dist\flowtime.exe .\input .\output-60fps --speed 100 --in-fps 24 --out-fps 60
```

### Пример без исходного видео

```powershell
.\dist\flowtime.exe synth .\work\demo-src 12 320 180
.\dist\flowtime.exe .\work\demo-src .\work\demo-slow --speed 25 --warp smart
```

<details>
<summary><strong>Параметры CLI</strong></summary>

| Параметр | Значения | По умолчанию |
| :--- | :--- | :--- |
| `--speed` | Скорость в процентах | `50` |
| `--in-fps` / `--out-fps` | Входная / выходная частота кадров | `24` / `24` |
| `--quality` | `none`, `sloppy`, `medium`, `high`, `best` | `best` |
| `--interp` | `nearest`, `blend`, `mwb` | `blend` |
| `--warp` | `inverse`, `smart`, `forward`, `forward-nosmart` | `inverse` |
| `--prep` | `none`, `contrast`, `delin` | `none` |
| `--sens` | Чувствительность движения | `70` |
| `--mb` | Компенсация motion blur при ускорении | `0` |
| `--frames` | Число выходных кадров | По длительности и скорости |
| `--prof` | Включить профилирование; значение не требуется | Выключено |

Дополнительные команды:

```text
flowtime synth <dir> <frames> <width> <height>
flowtime sheet <dir> <out.png> <first> <count> [cols]
flowtime score <reference> <run-a> <run-b> <step> [crops-dir]
flowtime darken <input> <output> <gain> <noise>
```

</details>

## Ограничения v0.1

- 32 bpc / SmartFX и ускорение на GPU пока не реализованы.
- Обработка полей (`Input/Output Fields`) и 360°-видео пока не реализована.
- Маски AE как направляющая геометрия (`First/Last Mask`) пока не поддерживаются.
- Вывод векторов движения, `Vectors In` и удаление motion blur при замедлении пока не реализованы.

## Структура проекта

```text
flowtime/
├── core/       # оптический поток, интерполяция, слои и отображение времени
├── ae/         # плагин After Effects и ресурс PiPL
├── cli/        # обработка последовательностей и инструменты сравнения
├── docs/       # спецификация, замеры и оформление
├── tools/ae/   # сценарии сравнения внутри After Effects
├── build.ps1   # сборка плагина и CLI
└── install.cmd # установка плагина
```

## Разработка и диагностика

Тесты ядра и CLI запускаются с тем же Rust/MinGW toolchain, что используется для сборки:

```powershell
cargo test --locked -p flowtime-core -p flowtime-cli
```

Для профилирования в AE создайте файл `%TEMP%\flowtime_prof.on`: сводка этапов появится в `%TEMP%\flowtime_prof.log`. Чтобы отключить профилирование, удалите файл-маркер. Ошибки рендеринга записываются в `%TEMP%\flowtime_errors.log`. В CLI используется флаг `--prof`.

## Документация

- [Замеры и сравнение с Twixtor Pro / Benchmarks](docs/benchmarks.md) — методика, результаты и команды воспроизведения.
- [Функциональная спецификация](docs/twixtor-functional-spec.md) — поведение референса и устройство собственного движка (на русском).
- [English README](README.en.md) — описание и инструкции на английском.

Интерфейс параметров ориентирован на Twixtor Pro; движок оптического потока и интерполяции реализован самостоятельно. Flowtime не связан с RE:Vision Effects. Twixtor — товарный знак соответствующего правообладателя.

## Лицензия

[MIT](LICENSE) · © 2026 b4demus
