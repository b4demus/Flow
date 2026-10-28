// Ground-truth retiming test inside After Effects. Run it through run_compare.ps1,
// which renders each comp in its own AE session.
// A clip segment is decimated to every 4th frame, then Flowtime and Twixtor Pro
// slow it back down to 25 % with the same settings. Both renders and the real
// frames go to PNG sequences for scoring outside AE.
(function () {
    // Source: the clip's frames as a TIFF sequence (seeking the MP4 directly made exports fail).
    var SEQ_FIRST = "D:/Files/Temp/Codex/flowtime/compare/src/frame_0000.tif";
    var SEQ_FPS = 23.976;
    var OUT = "D:/Files/Temp/Codex/flowtime/compare";
    var MAX_SRC_FRAMES = 120; // length of the tested segment in source frames
    var STEP = 4;             // keep every STEP-th frame
    var log = [];
    function note(s) { log.push(s); }
    function flush(status) {
        var f = new File(OUT + "/ae_log.json");
        f.encoding = "UTF-8";
        f.open("w");
        f.write('{"status":"' + status + '","log":[' + log.join(",") + "]}");
        f.close();
    }
    function q(s) { return '"' + String(s).replace(/\\/g, "/").replace(/"/g, "'") + '"'; }

    try {
        new Folder(OUT).create();
        var proj = app.project;
        app.beginUndoGroup("Flowtime compare");
        // Leftovers from an earlier run.
        for (var r = proj.renderQueue.items.length; r >= 1; r--) {
            var ri = proj.renderQueue.items[r];
            if (/^FT_/.test(ri.comp.name)) { ri.remove(); }
        }
        for (var n = proj.items.length; n >= 1; n--) {
            var pi = proj.items[n];
            if (pi instanceof FolderItem && pi.name === "Flowtime Compare") { pi.remove(); }
        }
        var folder = proj.items.addFolder("Flowtime Compare");
        var io = new ImportOptions(new File(SEQ_FIRST));
        io.sequence = true;
        var footage = proj.importFile(io);
        footage.mainSource.conformFrameRate = SEQ_FPS;
        footage.parentFolder = folder;
        var fps = footage.frameRate;
        var total = Math.floor(footage.duration * fps + 0.5);
        var len = Math.min(MAX_SRC_FRAMES, total);
        len -= len % STEP;
        var start = Math.max(0, Math.floor((total - len) / 2));
        var dur = len / fps;
        note(q("clip " + footage.width + "x" + footage.height + " @" + fps + " fps, " + total + " frames; segment " + start + ".." + (start + len)));

        // Decimated source: layer stretched to 25 % so comp frame k = source frame start + STEP*k.
        var sub = proj.items.addComp("FT_sub", footage.width, footage.height, footage.pixelAspect, dur, fps);
        sub.parentFolder = folder;
        var sl = sub.layers.add(footage);
        sl.stretch = 100 / STEP;
        sl.startTime = -start / fps / STEP;
        sl.frameBlendingType = FrameBlendingType.NO_FRAME_BLEND;

        // Ground truth: the untouched segment.
        var ref = proj.items.addComp("FT_ref", footage.width, footage.height, footage.pixelAspect, dur, fps);
        ref.parentFolder = folder;
        var rl = ref.layers.add(footage);
        rl.startTime = -start / fps;

        function retimeComp(name, matchName, setup) {
            var c = proj.items.addComp(name, footage.width, footage.height, footage.pixelAspect, dur, fps);
            c.parentFolder = folder;
            var l = c.layers.add(sub);
            l.outPoint = dur;
            var fx = l.property("ADBE Effect Parade").addProperty(matchName);
            setup(fx);
            return c;
        }
        function setByName(fx, name, value) {
            var p = fx.property(name);
            if (!p) { note(q("missing param " + name + " on " + fx.name)); return; }
            p.setValue(value);
        }
        var common = function (fx) {
            setByName(fx, "Input: Frame Rate", fps);
            setByName(fx, "Speed %", 100 / STEP);
            setByName(fx, "Motion Vectors", 5);  // Best
            setByName(fx, "Frame Interp", 2);    // Blend
            setByName(fx, "Warping", 1);         // Inverse
        };
        var ft = retimeComp("FT_flowtime", "B4DM Flowtime", common);
        var tw = retimeComp("FT_twixtor", "Twixtor 45", common);

        var rq = proj.renderQueue;
        var jobs = [["ref", ref], ["twixtor", tw], ["flowtime", ft]];
        // All items in one render() call; per-item time comes from status changes.
        var started = {}, took = {};
        for (var i = 0; i < jobs.length; i++) {
            new Folder(OUT + "/" + jobs[i][0]).create();
            var it = rq.items.add(jobs[i][1]);
            var om = it.outputModule(1);
            // Format is read-only via setSettings in this AE: pick a still-sequence template.
            var tpl = null, ext = "png";
            for (var t = 0; t < om.templates.length; t++) { if (/png/i.test(om.templates[t])) { tpl = om.templates[t]; break; } }
            if (!tpl) {
                for (t = 0; t < om.templates.length; t++) { if (/tiff/i.test(om.templates[t])) { tpl = om.templates[t]; ext = "tif"; break; } }
            }
            if (i === 0) { note(q("templates: " + om.templates.join(" | "))); }
            if (!tpl) { throw new Error("no PNG/TIFF output template"); }
            om.applyTemplate(tpl);
            note(q(jobs[i][0] + " template " + tpl));
            om.file = new File(OUT + "/" + jobs[i][0] + "/frame_[####]." + ext);
            (function (label, item) {
                item.onStatusChanged = function () {
                    if (item.status === RQItemStatus.RENDERING) { started[label] = new Date().getTime(); }
                    if (item.status === RQItemStatus.DONE && started[label]) { took[label] = (new Date().getTime() - started[label]) / 1000; }
                };
            })(jobs[i][0], it);
        }
        app.onError = function (err, severity) { note(q("onError[" + severity + "]: " + err)); };
        rq.render();
        for (var s = 1; s <= rq.numItems; s++) { note(q(rq.item(s).comp.name + " status " + rq.item(s).status)); }
        for (var j = 0; j < jobs.length; j++) { note(q(jobs[j][0] + " render " + took[jobs[j][0]] + " s")); }
        app.endUndoGroup();
        flush("done");
    } catch (e) {
        note(q("error: " + e.toString() + " line " + e.line));
        flush("error");
    }
})();
