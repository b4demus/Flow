// Flowcut — вырезает повторяющиеся кадры (аниме «на двойках/тройках») из выделенных слоёв.
// Оставшиеся уникальные кадры идут подряд через Time Remap с hold-ключами,
// слой укорачивается. Поверх можно накидывать Twixtor.
// Установка: положить в Support Files\Scripts\ScriptUI Panels и открыть через Window > Flowcut.jsx,
// либо запустить разово: File > Scripts > Run Script File.
(function (thisObj) {
    var SETTINGS = "Flowcut";
    var DEFAULT_THRESHOLD = 6;

    // <core> — без зависимостей от AE, проверяется тестом tests/run_tests.ps1
    var GX = 32, GY = 18; // сетка блоков, по которым сравниваются кадры

    // Эталон того, что считает AE-зонд: наибольшее расхождение средних по блокам
    // среди блоков и каналов, в уровнях 0..255.
    function frameDiff(a, b) {
        var m = 0;
        for (var i = 0; i < a.length; i++) {
            var v = a[i] - b[i];
            if (v < 0) v = -v;
            if (v > m) m = v;
        }
        return m;
    }

    // Индексы уникальных кадров из count. isNew(i, ref) — отличается ли кадр i от кадра ref.
    // Каждый кадр сравнивается с последним оставленным, поэтому медленный дрейф
    // (наезд, фейд) не теряется.
    function pickKept(count, isNew, onProgress) {
        var kept = [0], ref = 0;
        for (var i = 1; i < count; i++) {
            if (isNew(i, ref)) {
                kept.push(i);
                ref = i;
            }
            if (onProgress) onProgress((i + 1) / count);
        }
        return kept;
    }
    // </core>

    // Слои, обработанные под прежним именем панели (Dedupe), помечены [dedupe out=…]: узнаём оба тега.
    var TAG_RE = /\s*\[(?:flowcut|dedupe) out=([0-9.eE+-]+)\]/;

    function isCut(layer) {
        return layer.timeRemapEnabled && TAG_RE.test(layer.comment);
    }

    function restoreLayer(layer) {
        var m = TAG_RE.exec(layer.comment);
        layer.timeRemapEnabled = false;
        layer.outPoint = parseFloat(m[1]);
        layer.comment = layer.comment.replace(TAG_RE, "");
    }

    // Зонд сравнения кадров исходника силами AE, одним sampleImage на кадр:
    // Mosaic GX×GY усредняет блоки кадра A и эталона B, Difference даёт их разницу,
    // Levels гасит блоки с разницей <= порога и выбеливает остальные.
    // Если в среднем по кадру что-то осталось, кадр новый.
    function makeProbe(src, fps, threshold) {
        var w = src.width, h = src.height;
        var diff = app.project.items.addComp("Flowcut diff", w, h, 1, src.duration, fps);
        var reader = null;
        try {
            // Непрозрачная подложка: альфа 1 значит, что AE уже дорендерил кадр.
            var bg = diff.layers.addShape();
            var vectors = bg.property("ADBE Root Vectors Group");
            vectors.addProperty("ADBE Vector Shape - Rect").property("ADBE Vector Rect Size").setValue([w * 2, h * 2]);
            vectors.addProperty("ADBE Vector Graphic - Fill").property("ADBE Vector Fill Color").setValue([0, 0, 0, 1]);
            function mosaic(layer) {
                var m = layer.property("ADBE Effect Parade").addProperty("ADBE Mosaic");
                m.property(1).setValue(GX); // Horizontal Blocks
                m.property(2).setValue(GY); // Vertical Blocks
                return layer;
            }
            var b = mosaic(diff.layers.add(src));
            var a = mosaic(diff.layers.add(src));
            a.blendingMode = BlendingMode.DIFFERENCE;

            reader = app.project.items.addComp("Flowcut probe", w, h, 1, src.duration, fps);
            var d = reader.layers.add(diff);
            d.name = "DD_DIFF";
            var levels = d.property("ADBE Effect Parade").addProperty("ADBE Easy Levels2");
            levels.property("ADBE Easy Levels2-0003").setValue(threshold / 255);       // Input Black
            levels.property("ADBE Easy Levels2-0004").setValue((threshold + 1) / 255); // Input White
            var st = reader.layers.addText("").property("ADBE Text Properties").property("ADBE Text Document");
            st.expression = 'var L=thisComp.layer("DD_DIFF"),' +
                'c=L.sampleImage([L.width/2,L.height/2],[L.width/2,L.height/2],true,time);c.join(",")';
        } catch (e) {
            if (reader) reader.remove();
            diff.remove();
            throw e;
        }

        return {
            // Кадр i против кадра ref (номера кадров исходника).
            isNew: function (i, ref) {
                var t = (i + 0.25) / fps;
                b.startTime = (i - ref) / fps;
                reader.time = t;
                // sampleImage отдаёт нули, пока AE рендерит кадр в фоне.
                var started = new Date().getTime();
                while (true) {
                    var c = String(st.valueAtTime(t, false).text).split(",");
                    if (parseFloat(c[3]) > 0.99)
                        return Math.max(parseFloat(c[0]), parseFloat(c[1]), parseFloat(c[2])) > 0.5 / (GX * GY);
                    if (new Date().getTime() - started > 15000)
                        throw new Error("AE не отрендерил кадр " + i + " за 15 с.");
                    $.sleep(2);
                }
            },
            remove: function () {
                reader.remove();
                diff.remove();
            }
        };
    }

    function cutLayer(comp, layer, threshold, onProgress) {
        if (!(layer instanceof AVLayer) || !layer.source || !(layer.source.frameRate > 0))
            return "пропущен: нужен видеослой";
        if (layer.timeRemapEnabled) {
            if (!isCut(layer)) return "пропущен: уже включён Time Remap";
            restoreLayer(layer);
        }
        if (Math.abs(layer.stretch - 100) > 0.001) return "пропущен: у слоя изменён Stretch";

        var src = layer.source, fps = src.frameRate;
        var total = Math.round(src.duration * fps);
        var j0 = Math.max(0, Math.round((layer.inPoint - layer.startTime) * fps));
        var j1 = Math.min(total, Math.round((layer.outPoint - layer.startTime) * fps));
        if (j1 - j0 < 2) return "пропущен: меньше двух кадров";

        var probe = makeProbe(src, fps, threshold), kept;
        try {
            kept = pickKept(j1 - j0, function (i, ref) { return probe.isNew(j0 + i, j0 + ref); }, onProgress);
        } finally {
            probe.remove();
        }

        var origOut = layer.outPoint;
        var t0 = layer.inPoint, fd = comp.frameDuration;
        layer.timeRemapEnabled = true;
        var tr = layer.property("ADBE Time Remapping");
        // Удаление последнего ключа выключает Time Remap, поэтому один старый ключ живёт,
        // пока не поставлены новые.
        while (tr.numKeys > 1) tr.removeKey(tr.numKeys);
        var times = [], values = [];
        for (var k = 0; k < kept.length; k++) {
            times.push(t0 + k * fd);
            values.push((j0 + kept[k] + 0.25) / fps);
        }
        tr.setValuesAtTimes(times, values);
        if (Math.abs(tr.keyTime(1) - t0) > fd / 2) tr.removeKey(1);
        for (var n = 1; n <= tr.numKeys; n++)
            tr.setInterpolationTypeAtKey(n, KeyframeInterpolationType.HOLD, KeyframeInterpolationType.HOLD);
        layer.outPoint = t0 + kept.length * fd;
        layer.comment = (layer.comment ? layer.comment + " " : "") + "[flowcut out=" + origOut + "]";

        var count = j1 - j0;
        return "вырезано " + (count - kept.length) + " из " + count + " кадров";
    }

    function selectedLayers() {
        var comp = app.project.activeItem;
        if (!(comp instanceof CompItem)) throw new Error("Откройте композицию.");
        if (comp.selectedLayers.length === 0) throw new Error("Выделите слои с клипами.");
        return { comp: comp, layers: comp.selectedLayers };
    }

    function buildUI(thisObj) {
        var win = (thisObj instanceof Panel) ? thisObj
            : new Window("palette", "Flowcut", undefined, { resizeable: true });
        win.orientation = "column";
        win.alignChildren = ["fill", "top"];
        win.spacing = 6;
        win.margins = 8;

        // Порог, сохранённый прежней панелью Dedupe, подхватываем.
        var cfg = app.settings.haveSetting(SETTINGS, "threshold") ? SETTINGS
            : (app.settings.haveSetting("Dedupe", "threshold") ? "Dedupe" : null);
        var saved = cfg ? parseFloat(app.settings.getSetting(cfg, "threshold")) : DEFAULT_THRESHOLD;
        if (!(saved >= 1 && saved <= 40)) saved = DEFAULT_THRESHOLD;

        var row = win.add("group");
        row.alignChildren = ["left", "center"];
        row.add("statictext", undefined, "Порог:");
        var slider = row.add("slider", undefined, saved, 1, 40);
        slider.alignment = ["fill", "center"];
        slider.preferredSize.width = 120;
        var field = row.add("edittext", undefined, String(saved));
        field.characters = 3;
        slider.helpTip = field.helpTip =
            "Насколько должен измениться кадр, чтобы считаться новым (уровни 0..255).\n" +
            "Режет живые кадры — уменьшить порог, оставляет дубли — увеличить.";

        var buttons = win.add("group");
        buttons.alignChildren = ["fill", "center"];
        var runBtn = buttons.add("button", undefined, "Вырезать дубли");
        var undoBtn = buttons.add("button", undefined, "Вернуть");
        runBtn.alignment = undoBtn.alignment = ["fill", "center"];

        var bar = win.add("progressbar", undefined, 0, 1);
        bar.preferredSize.height = 6;
        var status = win.add("statictext", undefined, "Выделите слои и нажмите «Вырезать дубли».", { multiline: true });
        status.preferredSize = [220, 48];

        function threshold() {
            return Math.round(slider.value);
        }
        slider.onChanging = function () { field.text = String(threshold()); };
        slider.onChange = function () {
            field.text = String(threshold());
            app.settings.saveSetting(SETTINGS, "threshold", String(threshold()));
        };
        field.onChange = function () {
            var v = parseFloat(field.text);
            if (!(v >= 1)) v = DEFAULT_THRESHOLD;
            slider.value = Math.min(40, v);
            field.text = String(threshold());
            app.settings.saveSetting(SETTINGS, "threshold", String(threshold()));
        };

        function refresh() {
            if (typeof win.update === "function") win.update();
        }

        runBtn.onClick = function () {
            var sel;
            try { sel = selectedLayers(); } catch (e) { status.text = e.message; return; }
            var lines = [], started = new Date().getTime();
            app.beginUndoGroup("Flowcut");
            try {
                for (var i = 0; i < sel.layers.length; i++) {
                    var layer = sel.layers[i];
                    status.text = "Анализ «" + layer.name + "»…";
                    refresh();
                    var base = i / sel.layers.length, span = 1 / sel.layers.length;
                    var msg = cutLayer(sel.comp, layer, threshold(), function (p) {
                        bar.value = base + p * span;
                        refresh();
                    });
                    lines.push("«" + layer.name + "»: " + msg);
                }
                lines.push("Готово за " + ((new Date().getTime() - started) / 1000).toFixed(1) + " с.");
            } catch (e) {
                lines.push("Ошибка: " + e.message);
            } finally {
                app.endUndoGroup();
            }
            bar.value = 1;
            status.text = lines.join("\n");
        };

        undoBtn.onClick = function () {
            var sel;
            try { sel = selectedLayers(); } catch (e) { status.text = e.message; return; }
            var count = 0;
            app.beginUndoGroup("Flowcut: вернуть");
            for (var i = 0; i < sel.layers.length; i++) {
                if (isCut(sel.layers[i])) { restoreLayer(sel.layers[i]); count++; }
            }
            app.endUndoGroup();
            bar.value = 0;
            status.text = count ? "Возвращено слоёв: " + count + "." : "Среди выделенных нет обработанных слоёв.";
        };

        win.onResizing = win.onResize = function () { this.layout.resize(); };
        if (win instanceof Window) {
            win.center();
            win.show();
        } else {
            win.layout.layout(true);
        }
    }

    // tests/ae_test.jsx загружает файл без интерфейса и вызывает функции напрямую.
    if ($.global.FLOWCUT_NO_UI) $.global.Flowcut = { cutLayer: cutLayer, restoreLayer: restoreLayer };
    else buildUI(thisObj);
})(this);
