// Renders frames of a movie to a TIFF sequence. Settings come from
// %TEMP%\flowtime_extract.json:
// {"clip": "...", "out": "dir", "start": seconds, "count": frames, "step": source frames per output frame, "scale": 0.25}
(function () {
    var cfgFile = new File(Folder.temp.fsName + "/flowtime_extract.json");
    cfgFile.open("r");
    var cfg = eval("(" + cfgFile.read() + ")");
    cfgFile.close();
    var log = [];
    try {
        new Folder(cfg.out).create();
        var footage = app.project.importFile(new ImportOptions(new File(cfg.clip)));
        var fps = footage.frameRate;
        var w = Math.round(footage.width * cfg.scale / 2) * 2;
        var h = Math.round(footage.height * cfg.scale / 2) * 2;
        var comp = app.project.items.addComp("FT_extract", w, h, 1, cfg.count / fps, fps);
        var l = comp.layers.add(footage);
        l.stretch = 100 / cfg.step;
        l.startTime = -cfg.start / cfg.step;
        l.frameBlendingType = FrameBlendingType.NO_FRAME_BLEND;
        l.property("ADBE Transform Group").property("ADBE Scale").setValue([cfg.scale * 100, cfg.scale * 100]);
        l.property("ADBE Transform Group").property("ADBE Position").setValue([w / 2, h / 2]);
        var it = app.project.renderQueue.items.add(comp);
        var om = it.outputModule(1);
        om.applyTemplate("TIFF Sequence with Alpha");
        om.file = new File(cfg.out + "/frame_[####].tif");
        app.project.renderQueue.render();
        log.push('"fps":' + fps + ',"duration":' + footage.duration + ',"width":' + footage.width + ',"height":' + footage.height + ',"status":' + it.status);
    } catch (e) {
        log.push('"error":"' + String(e).replace(/"/g, "'") + '"');
    }
    var f = new File(cfg.out + "/extract_log.json");
    f.open("w");
    f.write("{" + log.join(",") + "}");
    f.close();
})();
