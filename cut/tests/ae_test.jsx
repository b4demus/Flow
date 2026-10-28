// End-to-end check of Flowcut.jsx inside After Effects (closes the current project
// without saving). Run: AfterFX.exe -r <path>\ae_test.jsx  (path without quotes),
// needs "Allow Scripts to Write Files". Result: ae_test_log.txt next to this file.
(function () {
    var here = File($.fileName).parent;
    var log = [];
    function out(s) {
        log.push(s);
        var f = new File(here.fsName + "/ae_test_log.txt");
        f.encoding = "UTF-8";
        f.open("w"); f.write(log.join("\n")); f.close();
    }
    function check(name, ok, info) { out((ok ? "ok   " : "FAIL ") + name + (info ? "  (" + info + ")" : "")); }
    try {
        $.global.FLOWCUT_NO_UI = true;
        $.evalFile(new File(here.parent.fsName + "/Flowcut.jsx"));
        app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES);
        app.newProject();

        // Source: a white square on 2s, a one-second hold, then 2s again, plus per-frame noise.
        var fps = 24;
        var anim = app.project.items.addComp("anim", 1920, 1080, 1, 4, fps);
        anim.layers.addSolid([0.1, 0.1, 0.15], "bg", 1920, 1080, 1);
        var sq = anim.layers.addSolid([1, 1, 1], "square", 100, 100, 1);
        sq.property("ADBE Transform Group").property("ADBE Position").expression =
            "posterizeTime(12); var t = time; [t < 2 ? 200 + t * 200 : t < 3 ? 600 : 600 + (t - 3) * 200, 540]";
        var adj = anim.layers.addSolid([1, 1, 1], "grain", 1920, 1080, 1);
        adj.adjustmentLayer = true;
        var noise = adj.property("ADBE Effect Parade").addProperty("ADBE Noise");
        noise.property(1).setValue(3);
        // 24 drawings in 0..2 s, 1 hold, 11 new drawings in 3..4 s (the first one equals the hold).
        var EXPECTED = 36;

        var main = app.project.items.addComp("main", 1920, 1080, 1, 4, fps);
        var layer = main.layers.add(anim);
        var started = new Date().getTime();
        var msg = $.global.Flowcut.cutLayer(main, layer, 6, function () {});
        var secs = (new Date().getTime() - started) / 1000;
        out("cutLayer: " + msg + ", " + secs.toFixed(1) + " s (" + (secs / 96 * 1000).toFixed(0) + " ms/frame)");

        var tr = layer.property("ADBE Time Remapping");
        check("time remap enabled", layer.timeRemapEnabled);
        check("one key per unique frame", tr.numKeys === EXPECTED, tr.numKeys + " keys, expected " + EXPECTED);
        check("layer shortened", Math.abs(layer.outPoint - EXPECTED / fps) < 1e-6, "out " + layer.outPoint);
        var increasing = true, holds = true;
        for (var k = 1; k <= tr.numKeys; k++) {
            if (k > 1 && !(tr.keyValue(k) > tr.keyValue(k - 1))) increasing = false;
            if (tr.keyOutInterpolationType(k) !== KeyframeInterpolationType.HOLD) holds = false;
        }
        check("key values increase", increasing);
        check("keys are hold", holds);
        var probes = 0;
        for (var n = 1; n <= app.project.numItems; n++) if (/^Flowcut /.test(app.project.item(n).name)) probes++;
        check("probe comps removed", probes === 0, probes + " left");
        var again = $.global.Flowcut.cutLayer(main, layer, 6, function () {});
        check("re-run restores and cuts again", tr.numKeys === EXPECTED, again);

        $.global.Flowcut.restoreLayer(layer);
        check("restore turns time remap off", !layer.timeRemapEnabled);
        check("restore brings back out point", Math.abs(layer.outPoint - 4) < 1e-6, "out " + layer.outPoint);
        check("restore clears tag", layer.comment === "", "comment '" + layer.comment + "'");

        // Real anime footage (camera moves every frame): nothing to cut.
        var seqDir = here.parent.parent.parent.fsName + "/flowtime_compare_data/src_anime";
        var first = new File(seqDir + "/frame_0000.tif");
        if (first.exists) {
            var io = new ImportOptions(first);
            io.sequence = true; io.forceAlphabetical = true;
            var foot = app.project.importFile(io);
            foot.mainSource.conformFrameRate = 24;
            var frames = Math.round(foot.duration * 24);
            var realComp = app.project.items.addComp("real", foot.width, foot.height, 1, foot.duration, 24);
            var realLayer = realComp.layers.add(foot);
            started = new Date().getTime();
            msg = $.global.Flowcut.cutLayer(realComp, realLayer, 6, function () {});
            secs = (new Date().getTime() - started) / 1000;
            out("real footage: " + msg + ", " + (secs / frames * 1000).toFixed(0) + " ms/frame");
            check("real footage keeps every frame", realLayer.property("ADBE Time Remapping").numKeys === frames);

            // The same footage held on 3s: one frame per drawing must survive.
            var threes = app.project.items.addComp("threes", foot.width, foot.height, 1, foot.duration, 24);
            var held = threes.layers.add(foot);
            held.timeRemapEnabled = true;
            held.property("ADBE Time Remapping").expression = "posterizeTime(8); time";
            var outer = app.project.items.addComp("outer", foot.width, foot.height, 1, foot.duration, 24);
            var threesLayer = outer.layers.add(threes);
            msg = $.global.Flowcut.cutLayer(outer, threesLayer, 6, function () {});
            var want = Math.ceil(frames / 3), got = threesLayer.property("ADBE Time Remapping").numKeys;
            check("footage on 3s keeps one frame per drawing", got === want, msg + ", keys " + got + ", expected " + want);
            var picks = [], onDrawings = true, ttr = threesLayer.property("ADBE Time Remapping");
            for (var q = 1; q <= ttr.numKeys; q++) {
                var frame = Math.floor(ttr.keyValue(q) * 24 + 1e-6);
                picks.push(frame);
                if (frame !== (q - 1) * 3) onDrawings = false;
            }
            check("kept frames are the first of each hold", onDrawings, picks.join(","));
        } else {
            out("skip real footage: " + first.fsName + " not found");
        }
        out("done");
    } catch (e) {
        out("ERR " + e.toString() + " line " + e.line);
    }
})();
