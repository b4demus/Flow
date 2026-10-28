#target aftereffects
#targetengine "FlowCurves"

/*
 * FlowCurves  -  Bezier easing editor for After Effects (single file, no dependencies)
 *
 * Install (optional, to get a dockable panel):
 *   copy this file to  <After Effects>/Support Files/Scripts/ScriptUI Panels/
 *   restart AE, then Window > FlowCurves.jsx
 * Run without installing:
 *   File > Scripts > Run Script File...  (opens a floating palette)
 *
 * Usage:
 *   1. Select 2+ keyframes (or one keyframe: it is eased to the next one).
 *   2. Drag the two handles or click a preset, then press Apply (or leave Auto on).
 *   - "Get" reads the easing of the selected keyframes back into the graph.
 *   - The text field accepts "0.25, 0.1, 0.25, 1" or "cubic-bezier(.4,0,.2,1)".
 *   - Shift while dragging snaps to a 0.05 grid.
 *   - "Save" stores the current curve as a custom preset; "Del" removes the selected one.
 *   - Custom presets live in  <user data>/FlowCurves/presets.txt
 */

(function (thisObj) {

    var NAME = "FlowCurves";
    var SETTINGS = "FlowCurves";

    // -------------------------------------------------------------- state
    var S = {
        b: [0.333, 0, 0.667, 1],   // x1, y1, x2, y2 (css cubic-bezier convention)
        applyOut: true,
        applyIn: true,
        auto: true,
        filter: "Popular",
        sel: ""
    };

    var VIEW = { lo: -0.4, hi: 1.4 };   // visible value range of the graph (refit when not dragging)
    var HYMIN = -0.6, HYMAX = 1.6;      // allowed range for handle y
    var PAD = 12;
    var HIT = 16;
    var TS = 36, GAP = 4;

    // choose a visible y-range that contains the whole curve and both handles
    function fitView(b) {
        var lo = -0.4, hi = 1.4, i, s;
        lo = Math.min(lo, b[1] - 0.15, b[3] - 0.15);
        hi = Math.max(hi, b[1] + 0.15, b[3] + 0.15);
        for (i = 0; i <= 40; i++) {
            s = sampleBez(b, i / 40);
            lo = Math.min(lo, s[1] - 0.1);
            hi = Math.max(hi, s[1] + 0.1);
        }
        VIEW.lo = Math.floor(lo * 10) / 10;
        VIEW.hi = Math.ceil(hi * 10) / 10;
    }

    var COL = {
        bg:     [0.105, 0.105, 0.115, 1],
        box:    [0.145, 0.145, 0.16, 1],
        grid:   [1, 1, 1, 0.06],
        edge:   [1, 1, 1, 0.22],
        diag:   [1, 1, 1, 0.10],
        curve:  [0.36, 0.66, 1.00, 1],
        hOut:   [1.00, 0.56, 0.24, 1],
        hIn:    [0.38, 0.86, 0.60, 1],
        stem:   [1, 1, 1, 0.38],
        white:  [1, 1, 1, 1],
        tbg:    [0.17, 0.17, 0.19, 1],
        tsel:   [0.36, 0.66, 1.00, 1]
    };

    // -------------------------------------------------------------- presets
    // [name, category, x1, y1, x2, y2, popular]
    var BUILTIN_SRC = [
        ["Linear",        "Basic", 0, 0, 1, 1, 1],
        ["Ease",          "Basic", 0.25, 0.1, 0.25, 1, 0],
        ["Ease In",       "Basic", 0.42, 0, 1, 1, 1],
        ["Ease Out",      "Basic", 0, 0, 0.58, 1, 1],
        ["Ease In Out",   "Basic", 0.42, 0, 0.58, 1, 0],
        ["AE Easy Ease",  "Basic", 0.333, 0, 0.667, 1, 1],
        ["Snap",          "Basic", 0.8, 0, 0.2, 1, 0],
        ["Sine In",       "In",    0.12, 0, 0.39, 0, 0],
        ["Quad In",       "In",    0.11, 0, 0.5, 0, 0],
        ["Cubic In",      "In",    0.32, 0, 0.67, 0, 1],
        ["Quart In",      "In",    0.5, 0, 0.75, 0, 0],
        ["Quint In",      "In",    0.64, 0, 0.78, 0, 0],
        ["Expo In",       "In",    0.7, 0, 0.84, 0, 1],
        ["Circ In",       "In",    0.55, 0, 1, 0.45, 0],
        ["Back In",       "In",    0.36, 0, 0.66, -0.56, 1],
        ["Sine Out",      "Out",   0.61, 1, 0.88, 1, 0],
        ["Quad Out",      "Out",   0.5, 1, 0.89, 1, 0],
        ["Cubic Out",     "Out",   0.33, 1, 0.68, 1, 1],
        ["Quart Out",     "Out",   0.25, 1, 0.5, 1, 0],
        ["Quint Out",     "Out",   0.22, 1, 0.36, 1, 1],
        ["Expo Out",      "Out",   0.16, 1, 0.3, 1, 1],
        ["Circ Out",      "Out",   0, 0.55, 0.45, 1, 0],
        ["Back Out",      "Out",   0.34, 1.56, 0.64, 1, 1],
        ["Sine InOut",    "InOut", 0.37, 0, 0.63, 1, 0],
        ["Quad InOut",    "InOut", 0.45, 0, 0.55, 1, 0],
        ["Cubic InOut",   "InOut", 0.65, 0, 0.35, 1, 1],
        ["Quart InOut",   "InOut", 0.76, 0, 0.24, 1, 0],
        ["Quint InOut",   "InOut", 0.83, 0, 0.17, 1, 0],
        ["Expo InOut",    "InOut", 0.87, 0, 0.13, 1, 1],
        ["Circ InOut",    "InOut", 0.85, 0, 0.15, 1, 0],
        ["Back InOut",    "InOut", 0.68, -0.6, 0.32, 1.6, 1]
    ];
    var FILTERS = ["Popular", "Basic", "In", "Out", "InOut", "All", "Custom", "Hidden"];

    var presets = [];

    function rebuildPresetList(custom) {
        presets = [];
        var i, s;
        for (i = 0; i < BUILTIN_SRC.length; i++) {
            s = BUILTIN_SRC[i];
            presets.push({ name: s[0], cat: s[1], b: [s[2], s[3], s[4], s[5]], pop: s[6] == 1, custom: false });
        }
        for (i = 0; i < custom.length; i++) {
            presets.push({ name: custom[i].name, cat: "Custom", b: custom[i].b, pop: false, custom: true });
        }
    }

    function presetFile() {
        var folder = new Folder(Folder.userData.fsName + "/FlowCurves");
        if (!folder.exists) folder.create();
        var f = new File(folder.fsName + "/presets.txt");
        // Presets saved under the former name (EZCurves) are carried over once.
        if (!f.exists) {
            var old = new File(Folder.userData.fsName + "/EZCurves/presets.txt");
            if (old.exists) old.copy(f.fsName);
        }
        return f;
    }

    function loadCustom() {
        var out = [];
        try {
            var f = presetFile();
            if (f.exists) {
                f.encoding = "UTF-8";
                f.open("r");
                var txt = f.read();
                f.close();
                var lines = txt.split(/\r?\n/);
                for (var i = 0; i < lines.length; i++) {
                    var parts = lines[i].split("|");
                    if (parts.length != 2) continue;
                    var n = parts[1].split(",");
                    if (n.length != 4) continue;
                    var b = [parseFloat(n[0]), parseFloat(n[1]), parseFloat(n[2]), parseFloat(n[3])];
                    if (isNaN(b[0]) || isNaN(b[1]) || isNaN(b[2]) || isNaN(b[3])) continue;
                    out.push({ name: parts[0], b: b });
                }
            }
        } catch (e) {}
        return out;
    }

    function customOnly() {
        var out = [];
        for (var i = 0; i < presets.length; i++) if (presets[i].custom) out.push(presets[i]);
        return out;
    }

    function saveCustom() {
        try {
            var list = customOnly();
            var f = presetFile();
            f.encoding = "UTF-8";
            if (!f.open("w")) throw new Error("cannot open " + f.fsName);
            for (var i = 0; i < list.length; i++) {
                f.writeln(list[i].name + "|" + list[i].b.join(","));
            }
            f.close();
            return true;
        } catch (e) {
            alert(NAME + ": could not save presets.\n" + e.toString() +
                  "\n\nEnable Preferences > Scripting & Expressions > \"Allow Scripts to Write Files and Access Network\".");
            return false;
        }
    }

    // -------------------------------------------------------------- helpers
    function clamp(v, lo, hi) { return v < lo ? lo : (v > hi ? hi : v); }
    function r3(v) { return Math.round(v * 1000) / 1000; }

    function fmtBez(b) {
        return r3(b[0]) + ", " + r3(b[1]) + ", " + r3(b[2]) + ", " + r3(b[3]);
    }

    function sampleBez(b, t) {
        var u = 1 - t;
        var x = 3 * u * u * t * b[0] + 3 * u * t * t * b[2] + t * t * t;
        var y = 3 * u * u * t * b[1] + 3 * u * t * t * b[3] + t * t * t;
        return [x, y];
    }

    // -------------------------------------------------------------- AE engine
    function getComp() {
        var c = app.project.activeItem;
        return (c && c instanceof CompItem) ? c : null;
    }

    function easeCapable(p) {
        var t = p.propertyValueType;
        return t == PropertyValueType.OneD || t == PropertyValueType.TwoD ||
               t == PropertyValueType.ThreeD || t == PropertyValueType.TwoD_SPATIAL ||
               t == PropertyValueType.ThreeD_SPATIAL || t == PropertyValueType.COLOR ||
               t == PropertyValueType.SHAPE;
    }

    // Selected keyframes -> list of {prop, i1, i2}. Adjacent selected keys form segments,
    // a lone selected key forms a segment with the next one (or previous for the last key).
    function collectPairs(comp) {
        var res = [];
        var props = comp.selectedProperties;
        for (var i = 0; i < props.length; i++) {
            var p = props[i];
            if (p.propertyType !== PropertyType.PROPERTY) continue;
            if (p.numKeys < 2 || !easeCapable(p)) continue;
            var sk = p.selectedKeys;
            if (!sk || sk.length == 0) continue;
            var set = {};
            var j;
            for (j = 0; j < sk.length; j++) set[sk[j]] = true;
            for (j = 0; j < sk.length; j++) {
                var k = sk[j];
                if (set[k + 1]) res.push({ prop: p, i1: k, i2: k + 1 });
                else if (!set[k - 1]) {
                    if (k < p.numKeys) res.push({ prop: p, i1: k, i2: k + 1 });
                    else if (k > 1) res.push({ prop: p, i1: k - 1, i2: k });
                }
            }
        }
        return res;
    }

    function dist(a, b) {
        var s = 0;
        for (var i = 0; i < a.length; i++) s += (b[i] - a[i]) * (b[i] - a[i]);
        return Math.sqrt(s);
    }

    // length of the spatial path between two keyframes
    function spatialLength(p, i1, i2, a, b) {
        var o, t;
        try {
            o = p.keyOutSpatialTangent(i1);
            t = p.keyInSpatialTangent(i2);
        } catch (e) { return dist(a, b); }
        var n = a.length, P = [[], [], [], []], d;
        for (d = 0; d < n; d++) {
            P[0][d] = a[d];
            P[1][d] = a[d] + (o[d] || 0);
            P[2][d] = b[d] + (t[d] || 0);
            P[3][d] = b[d];
        }
        var steps = 48, len = 0, prev = a, cur, s, u;
        for (s = 1; s <= steps; s++) {
            u = s / steps;
            cur = [];
            for (d = 0; d < n; d++) {
                cur[d] = Math.pow(1 - u, 3) * P[0][d] + 3 * Math.pow(1 - u, 2) * u * P[1][d] +
                         3 * (1 - u) * u * u * P[2][d] + u * u * u * P[3][d];
            }
            len += dist(prev, cur);
            prev = cur;
        }
        return len;
    }

    // value change per ease dimension for a segment
    function segmentDeltas(p, i1, i2, n) {
        var vt = p.propertyValueType, d = [], j;
        for (j = 0; j < n; j++) d[j] = 0;
        if (vt == PropertyValueType.SHAPE) return d;
        var a = p.keyValue(i1), b = p.keyValue(i2);
        if (vt == PropertyValueType.TwoD_SPATIAL || vt == PropertyValueType.ThreeD_SPATIAL) {
            d[0] = spatialLength(p, i1, i2, a, b);
        } else if (a instanceof Array) {
            for (j = 0; j < n; j++) d[j] = b[j] - a[j];
        } else {
            d[0] = b - a;
        }
        return d;
    }

    function prepKey(p, i, outSide, inSide) {
        var tin = p.keyInInterpolationType(i), tout = p.keyOutInterpolationType(i);
        var nin = inSide ? KeyframeInterpolationType.BEZIER : tin;
        var nout = outSide ? KeyframeInterpolationType.BEZIER : tout;
        if (nin != tin || nout != tout) p.setInterpolationTypeAtKey(i, nin, nout);
        try { if (p.keyTemporalAutoBezier(i)) p.setTemporalAutoBezierAtKey(i, false); } catch (e1) {}
        try { if (p.keyTemporalContinuous(i)) p.setTemporalContinuousAtKey(i, false); } catch (e2) {}
    }

    function outEase(b, delta, dt) {
        var frac = clamp(b[0], 0.001, 1);
        var infl = clamp(frac * 100, 0.1, 100);
        frac = infl / 100;
        return new KeyframeEase(b[1] * delta / (frac * dt), infl);
    }

    function inEase(b, delta, dt) {
        var frac = clamp(1 - b[2], 0.001, 1);
        var infl = clamp(frac * 100, 0.1, 100);
        frac = infl / 100;
        return new KeyframeEase((1 - b[3]) * delta / (frac * dt), infl);
    }

    function applyPair(p, i1, i2, b, doOut, doIn) {
        var dt = p.keyTime(i2) - p.keyTime(i1);
        if (dt <= 0) return false;
        prepKey(p, i1, doOut, false);
        prepKey(p, i2, false, doIn);
        var n = p.keyOutTemporalEase(i1).length;
        var d = segmentDeltas(p, i1, i2, n), j;
        if (doOut) {
            var keepIn = p.keyInTemporalEase(i1), no = [];
            for (j = 0; j < n; j++) no[j] = outEase(b, d[j], dt);
            p.setTemporalEaseAtKey(i1, keepIn, no);
        }
        if (doIn) {
            var keepOut = p.keyOutTemporalEase(i2), ni = [];
            for (j = 0; j < n; j++) ni[j] = inEase(b, d[j], dt);
            p.setTemporalEaseAtKey(i2, ni, keepOut);
        }
        // spatial speed is a magnitude in AE, so overshoot (negative speed) cannot be stored
        var vt = p.propertyValueType;
        var spatial = (vt == PropertyValueType.TwoD_SPATIAL || vt == PropertyValueType.ThreeD_SPATIAL);
        if (spatial && ((doOut && b[1] < 0) || (doIn && b[3] > 1))) return 2;
        return 1;
    }

    // reconstruct the bezier from the existing easing of a segment (null if impossible)
    function readPair(p, i1, i2) {
        var dt = p.keyTime(i2) - p.keyTime(i1);
        if (dt <= 0) return null;
        var LIN = KeyframeInterpolationType.LINEAR, BEZ = KeyframeInterpolationType.BEZIER;
        var ot = p.keyOutInterpolationType(i1), it = p.keyInInterpolationType(i2);
        if (ot == LIN && it == LIN) return [0, 0, 1, 1];
        if (ot != BEZ || it != BEZ) return null;
        var eo = p.keyOutTemporalEase(i1), ei = p.keyInTemporalEase(i2);
        var d = segmentDeltas(p, i1, i2, eo.length), best = 0, j;
        for (j = 1; j < d.length; j++) if (Math.abs(d[j]) > Math.abs(d[best])) best = j;
        if (Math.abs(d[best]) < 1e-9) return null;
        var fo = eo[best].influence / 100, fi = ei[best].influence / 100;
        return [
            fo,
            eo[best].speed * fo * dt / d[best],
            1 - fi,
            1 - ei[best].speed * fi * dt / d[best]
        ];
    }

    function applyToSelection() {
        var comp = getComp();
        if (!comp) return "Open a composition first.";
        var pairs = collectPairs(comp);
        if (!pairs.length) return "Select 2+ keyframes (or one) first.";
        var ok = 0, bad = 0, clipped = 0;
        app.beginUndoGroup(NAME + ": apply curve");
        try {
            for (var i = 0; i < pairs.length; i++) {
                var q = pairs[i];
                try {
                    var r = applyPair(q.prop, q.i1, q.i2, S.b, S.applyOut, S.applyIn);
                    if (r) ok++;
                    if (r == 2) clipped++;
                } catch (e) { bad++; }
            }
        } finally {
            app.endUndoGroup();
        }
        return "Applied to " + ok + " segment" + (ok == 1 ? "" : "s") +
               (bad ? ("  (" + bad + " failed)") : "") +
               (clipped ? "  - position can't overshoot, clipped" : "");
    }

    function readSelection() {
        var comp = getComp();
        if (!comp) return { msg: "Open a composition first." };
        var pairs = collectPairs(comp);
        for (var i = 0; i < pairs.length; i++) {
            var b = readPair(pairs[i].prop, pairs[i].i1, pairs[i].i2);
            if (b) return { b: b, msg: "Read from " + pairs[i].prop.name };
        }
        return { msg: pairs.length ? "Nothing to read (linear/hold or no motion)." : "Select keyframes first." };
    }

    function setInterp(kind) {
        var comp = getComp();
        if (!comp) return "Open a composition first.";
        var props = comp.selectedProperties, n = 0;
        app.beginUndoGroup(NAME + ": " + kind);
        try {
            for (var i = 0; i < props.length; i++) {
                var p = props[i];
                if (p.propertyType !== PropertyType.PROPERTY || p.numKeys < 1) continue;
                var sk = p.selectedKeys;
                if (!sk) continue;
                for (var j = 0; j < sk.length; j++) {
                    try {
                        var k = sk[j], T = KeyframeInterpolationType;
                        if (kind == "Linear") p.setInterpolationTypeAtKey(k, T.LINEAR, T.LINEAR);
                        else if (kind == "Hold") p.setInterpolationTypeAtKey(k, T.HOLD, T.HOLD);
                        else {
                            p.setInterpolationTypeAtKey(k, T.BEZIER, T.BEZIER);
                            try { p.setTemporalAutoBezierAtKey(k, true); } catch (e1) {}
                        }
                        n++;
                    } catch (e) {}
                }
            }
        } finally {
            app.endUndoGroup();
        }
        return n ? (kind + " set on " + n + " keyframe" + (n == 1 ? "" : "s")) : "Select keyframes first.";
    }

    // -------------------------------------------------------------- persistence of UI settings
    function saveSettings() {
        try {
            app.settings.saveSetting(SETTINGS, "bez", fmtBez(S.b));
            app.settings.saveSetting(SETTINGS, "auto", S.auto ? "1" : "0");
            app.settings.saveSetting(SETTINGS, "out", S.applyOut ? "1" : "0");
            app.settings.saveSetting(SETTINGS, "in", S.applyIn ? "1" : "0");
            app.settings.saveSetting(SETTINGS, "filter", S.filter);
        } catch (e) {}
    }

    function loadSettings() {
        try {
            // Panel state saved under the former name (EZCurves) is picked up once.
            var sec = SETTINGS;
            if (!app.settings.haveSetting(sec, "bez") && app.settings.haveSetting("EZCurves", "bez")) sec = "EZCurves";
            if (app.settings.haveSetting(sec, "bez")) {
                var n = app.settings.getSetting(sec, "bez").split(",");
                if (n.length == 4) {
                    var b = [parseFloat(n[0]), parseFloat(n[1]), parseFloat(n[2]), parseFloat(n[3])];
                    if (!isNaN(b[0] + b[1] + b[2] + b[3])) S.b = [clamp(b[0], 0, 1), clamp(b[1], HYMIN, HYMAX), clamp(b[2], 0, 1), clamp(b[3], HYMIN, HYMAX)];
                }
            }
            if (app.settings.haveSetting(sec, "auto")) S.auto = app.settings.getSetting(sec, "auto") == "1";
            if (app.settings.haveSetting(sec, "out")) S.applyOut = app.settings.getSetting(sec, "out") == "1";
            if (app.settings.haveSetting(sec, "in")) S.applyIn = app.settings.getSetting(sec, "in") == "1";
            if (app.settings.haveSetting(sec, "filter")) {
                var f = app.settings.getSetting(sec, "filter");
                for (var i = 0; i < FILTERS.length; i++) if (FILTERS[i] == f) S.filter = f;
            }
        } catch (e) {}
    }

    // -------------------------------------------------------------- drawing
    function brush(g, c) { return g.newBrush(g.BrushType.SOLID_COLOR, c); }
    function pen(g, c, w) { return g.newPen(g.PenType.SOLID_COLOR, c, w); }
    function line(g, p, x1, y1, x2, y2) {
        g.newPath(); g.moveTo(x1, y1); g.lineTo(x2, y2); g.strokePath(p);
    }
    function fillRect(g, c, x, y, w, h) {
        g.newPath(); g.rectPath(x, y, w, h); g.fillPath(brush(g, c));
    }
    function strokeRect(g, c, wd, x, y, w, h) {
        g.newPath(); g.rectPath(x, y, w, h); g.strokePath(pen(g, c, wd));
    }
    function dot(g, c, cx, cy, r) {
        g.newPath(); g.ellipsePath(cx - r, cy - r, 2 * r, 2 * r); g.fillPath(brush(g, c));
    }
    function ring(g, c, wd, cx, cy, r) {
        g.newPath(); g.ellipsePath(cx - r, cy - r, 2 * r, 2 * r); g.strokePath(pen(g, c, wd));
    }

    function gx(x, W) { return PAD + x * (W - 2 * PAD); }
    function gy(y, H) { return (H - PAD) - (y - VIEW.lo) / (VIEW.hi - VIEW.lo) * (H - 2 * PAD); }

    var dragging = -1;   // 0 = out handle, 1 = in handle

    function drawGraph(el) {
        var g = el.graphics, W = el.size.width, H = el.size.height, i;
        fillRect(g, COL.bg, 0, 0, W, H);
        // unit box
        fillRect(g, COL.box, gx(0, W), gy(1, H), gx(1, W) - gx(0, W), gy(0, H) - gy(1, H));
        // grid
        var pg = pen(g, COL.grid, 1);
        for (i = 1; i < 4; i++) line(g, pg, gx(i / 4, W), gy(VIEW.hi, H), gx(i / 4, W), gy(VIEW.lo, H));
        var yv;
        for (yv = Math.ceil(VIEW.lo * 4) / 4; yv <= VIEW.hi; yv += 0.25) {
            if (Math.abs(yv) < 0.01 || Math.abs(yv - 1) < 0.01) continue;
            line(g, pg, gx(0, W), gy(yv, H), gx(1, W), gy(yv, H));
        }
        var pe = pen(g, COL.edge, 1);
        line(g, pe, gx(0, W), gy(0, H), gx(1, W), gy(0, H));
        line(g, pe, gx(0, W), gy(1, H), gx(1, W), gy(1, H));
        line(g, pe, gx(0, W), gy(VIEW.hi, H), gx(0, W), gy(VIEW.lo, H));
        line(g, pe, gx(1, W), gy(VIEW.hi, H), gx(1, W), gy(VIEW.lo, H));
        line(g, pen(g, COL.diag, 1), gx(0, W), gy(0, H), gx(1, W), gy(1, H));

        var b = S.b;
        var x0 = gx(0, W), y0 = gy(0, H), x3 = gx(1, W), y3 = gy(1, H);
        var h1x = gx(b[0], W), h1y = gy(b[1], H), h2x = gx(b[2], W), h2y = gy(b[3], H);
        // stems
        var ps = pen(g, COL.stem, 1.5);
        line(g, ps, x0, y0, h1x, h1y);
        line(g, ps, x3, y3, h2x, h2y);
        // curve
        g.newPath();
        var s = sampleBez(b, 0);
        g.moveTo(gx(s[0], W), gy(s[1], H));
        for (i = 1; i <= 90; i++) {
            s = sampleBez(b, i / 90);
            g.lineTo(gx(s[0], W), gy(s[1], H));
        }
        g.strokePath(pen(g, COL.curve, 2.6));
        // end points
        dot(g, COL.white, x0, y0, 3.5);
        dot(g, COL.white, x3, y3, 3.5);
        // handles
        dot(g, COL.hOut, h1x, h1y, 6);
        dot(g, COL.hIn, h2x, h2y, 6);
        if (dragging == 0) ring(g, COL.white, 1.5, h1x, h1y, 9);
        if (dragging == 1) ring(g, COL.white, 1.5, h2x, h2y, 9);
    }

    function drawThumb(el, p) {
        var g = el.graphics, W = el.size.width, H = el.size.height, i;
        fillRect(g, COL.tbg, 0, 0, W, H);
        var b = p.b, s, lo = 0, hi = 1;
        for (i = 0; i <= 40; i++) {
            s = sampleBez(b, i / 40);
            if (s[1] < lo) lo = s[1];
            if (s[1] > hi) hi = s[1];
        }
        var m = 7, span = hi - lo;
        function tx(x) { return m + x * (W - 2 * m); }
        function ty(y) { return (H - m) - (y - lo) / span * (H - 2 * m); }
        var pg = pen(g, COL.grid, 1);
        line(g, pg, tx(0), ty(0), tx(1), ty(0));
        line(g, pg, tx(0), ty(1), tx(1), ty(1));
        g.newPath();
        s = sampleBez(b, 0);
        g.moveTo(tx(s[0]), ty(s[1]));
        for (i = 1; i <= 36; i++) {
            s = sampleBez(b, i / 36);
            g.lineTo(tx(s[0]), ty(s[1]));
        }
        g.strokePath(pen(g, p.name == S.sel ? COL.white : COL.curve, 2));
        if (p.name == S.sel) strokeRect(g, COL.tsel, 2, 1, 1, W - 2, H - 2);
    }

    // -------------------------------------------------------------- UI
    function buildUI(host) {
        var win = (host instanceof Panel) ? host : new Window("palette", NAME, undefined, { resizeable: true });
        win.orientation = "column";
        win.alignChildren = ["fill", "top"];
        win.spacing = 4;
        win.margins = 6;

        loadSettings();
        rebuildPresetList(loadCustom());

        // graph
        // Every child gets a tiny preferred width so the panel can be narrowed freely;
        // "fill" alignment stretches the rows back out when there is room.
        fitView(S.b);

        // graph: the only element that grows vertically, so it takes all spare height
        var cv = win.add("customview");
        cv.alignment = ["fill", "fill"];
        cv.preferredSize = [150, 120];   // small on purpose: spare height goes to the graph, controls never get squeezed
        cv.minimumSize = [60, 110];
        cv.onDraw = function () { drawGraph(this); };

        // value field
        var txt = win.add("edittext", undefined, fmtBez(S.b));
        txt.alignment = ["fill", "top"];
        txt.preferredSize = [20, 22];
        txt.helpTip = "x1, y1, x2, y2  -  type or paste cubic-bezier(...) and press Enter";

        // apply row
        var rowA = win.add("group");
        rowA.orientation = "row";
        rowA.alignment = ["fill", "top"];
        rowA.alignChildren = ["fill", "center"];
        rowA.spacing = 4;
        var btnApply = rowA.add("button", undefined, "Apply");
        btnApply.preferredSize = [20, 24];
        btnApply.helpTip = "Apply this curve to the selected keyframes";
        var btnGet = rowA.add("button", undefined, "Get");
        btnGet.preferredSize = [20, 24];
        btnGet.helpTip = "Read the easing of the selected keyframes into the graph";

        // options row
        var rowO = win.add("group");
        rowO.orientation = "row";
        rowO.alignment = ["fill", "top"];
        rowO.alignChildren = ["left", "center"];
        rowO.spacing = 6;
        var cbOut = rowO.add("checkbox", undefined, "Out");
        cbOut.value = S.applyOut;
        cbOut.helpTip = "Write the outgoing ease of the first keyframe of each segment";
        var cbIn = rowO.add("checkbox", undefined, "In");
        cbIn.value = S.applyIn;
        cbIn.helpTip = "Write the incoming ease of the second keyframe of each segment";
        var cbAuto = rowO.add("checkbox", undefined, "Auto");
        cbAuto.value = S.auto;
        cbAuto.helpTip = "Auto-apply: apply immediately when a handle is released or a preset is clicked";

        // interpolation row
        var rowI = win.add("group");
        rowI.orientation = "row";
        rowI.alignment = ["fill", "top"];
        rowI.alignChildren = ["fill", "center"];
        rowI.spacing = 4;
        var btnLin = rowI.add("button", undefined, "Lin");
        var btnHold = rowI.add("button", undefined, "Hold");
        var btnAuto = rowI.add("button", undefined, "Auto");
        btnLin.preferredSize = btnHold.preferredSize = btnAuto.preferredSize = [20, 24];
        btnLin.helpTip = "Set selected keyframes to linear";
        btnHold.helpTip = "Set selected keyframes to hold";
        btnAuto.helpTip = "Set selected keyframes to auto bezier";

        // preset header
        var rowP = win.add("group");
        rowP.orientation = "row";
        rowP.alignment = ["fill", "top"];
        rowP.alignChildren = ["fill", "center"];
        rowP.spacing = 4;
        var ddFilter = rowP.add("dropdownlist", undefined, FILTERS);
        ddFilter.preferredSize = [60, 24];
        ddFilter.helpTip = "Preset group (Hidden collapses the thumbnails to give the graph more room)";
        for (var fi = 0; fi < FILTERS.length; fi++) if (FILTERS[fi] == S.filter) ddFilter.selection = fi;
        var btnSave = rowP.add("button", undefined, "Save");
        btnSave.preferredSize = [20, 24];
        btnSave.helpTip = "Save the current curve as a custom preset";
        var btnDel = rowP.add("button", undefined, "Del");
        btnDel.preferredSize = [20, 24];
        btnDel.helpTip = "Delete the selected custom preset";

        // preset grid (columns follow the panel width)
        var grid = win.add("group");
        grid.orientation = "column";
        grid.alignChildren = ["left", "top"];
        grid.alignment = ["fill", "top"];
        grid.spacing = GAP;

        var status = win.add("statictext", undefined, "", { truncate: "end" });
        status.alignment = ["fill", "bottom"];
        status.preferredSize = [20, 16];
        status.graphics.foregroundColor = status.graphics.newPen(status.graphics.PenType.SOLID_COLOR, [0.62, 0.62, 0.66, 1], 1);

        var thumbs = [];
        var curCols = 0;

        function calcCols() {
            var avail = win.size ? win.size.width - 2 * M : 0;
            if (!avail || avail < 40) return 4;
            return Math.max(1, Math.floor((avail + GAP) / (TS + GAP)));
        }

        function say(m) { status.text = m; }

        function redraw() { cv.notify("onDraw"); }

        function redrawThumbs() {
            for (var i = 0; i < thumbs.length; i++) thumbs[i].notify("onDraw");
        }

        function setBez(b, silent) {
            S.b = [
                r3(clamp(b[0], 0, 1)), r3(clamp(b[1], HYMIN, HYMAX)),
                r3(clamp(b[2], 0, 1)), r3(clamp(b[3], HYMIN, HYMAX))
            ];
            txt.text = fmtBez(S.b);
            if (dragging < 0) fitView(S.b);
            redraw();
            if (!silent) saveSettings();
        }

        function doApply() {
            try { say(applyToSelection()); } catch (e) { say("Error: " + e.toString()); }
        }

        function matchesFilter(p) {
            var f = S.filter;
            if (f == "Hidden") return false;
            if (f == "All") return true;
            if (f == "Popular") return p.pop;
            if (f == "Custom") return p.custom;
            return p.cat == f;
        }

        function choose(p) {
            S.sel = p.name;
            setBez(p.b);
            redrawThumbs();
            if (S.auto) doApply();
        }

        function removePreset(p) {
            if (!p.custom) return;
            if (!confirm("Delete preset \"" + p.name + "\"?")) return;
            for (var i = 0; i < presets.length; i++) if (presets[i] === p) { presets.splice(i, 1); break; }
            saveCustom();
            if (S.sel == p.name) S.sel = "";
            buildGrid();
        }

        function makeThumb(parent, p) {
            var t = parent.add("customview");
            t.preferredSize = [TS, TS];
            t.minimumSize = [TS, TS];
            t.maximumSize = [TS, TS];
            t.helpTip = p.name + "   (" + fmtBez(p.b) + ")" + (p.custom ? "   - right click to delete" : "");
            t.onDraw = function () { drawThumb(this, p); };
            t.addEventListener("mousedown", function (ev) {
                if (ev.button == 2) { removePreset(p); return; }
                choose(p);
            });
            thumbs.push(t);
        }

        function buildGrid() {
            while (grid.children.length) grid.remove(grid.children[0]);
            thumbs = [];
            var row = null, n = 0;
            curCols = calcCols();
            for (var i = 0; i < presets.length; i++) {
                if (!matchesFilter(presets[i])) continue;
                if (n % curCols == 0) {
                    row = grid.add("group");
                    row.orientation = "row";
                    row.spacing = GAP;
                }
                makeThumb(row, presets[i]);
                n++;
            }
            if (n == 0 && S.filter != "Hidden") {
                var none = grid.add("statictext", undefined, S.filter == "Custom" ? "No custom presets yet - press Save." : "Nothing here.");
            }
            grid.layout.layout(true);   // arranges the nested thumbnail rows
            place();
        }

        // Manual layout. ScriptUI's automatic column layout spreads spare height over every
        // child (stretching buttons and the text field) and never gives it to the graph, so the
        // window is laid out by hand: fixed-height rows at the bottom, the graph takes the rest.
        var M = 6, SP = 4;
        function place() {
            var W = win.size ? win.size.width : 0, H = win.size ? win.size.height : 0;
            if (!W || !H || !cv) return;
            var w = W - 2 * M;
            var nrows = curCols > 0 ? Math.ceil(thumbs.length / curCols) : 0;
            var gh = nrows > 0 ? nrows * TS + (nrows - 1) * GAP : (S.filter == "Hidden" ? 0 : 16);
            var defs = [[txt, 22], [rowA, 24], [rowO, 20], [rowI, 24], [rowP, 24], [grid, gh], [status, 16]];
            var used = 2 * M + defs.length * SP, i;
            for (i = 0; i < defs.length; i++) used += defs[i][1];
            var ch = Math.max(110, H - used);
            cv.location = [M, M];
            cv.size = [w, ch];
            var y = M + ch + SP, c;
            for (i = 0; i < defs.length; i++) {
                c = defs[i][0];
                c.location = [M, y];
                c.size = [w, defs[i][1]];
                try { if (c.layout) c.layout.resize(); } catch (e) {}
                y += defs[i][1] + SP;
            }
            redraw();
        }

        // ---- graph mouse
        function handlePos(idx) {
            var W = cv.size.width, H = cv.size.height, b = S.b;
            return idx == 0 ? [gx(b[0], W), gy(b[1], H)] : [gx(b[2], W), gy(b[3], H)];
        }

        function moveHandle(idx, mx, my, snap) {
            var W = cv.size.width, H = cv.size.height;
            var x = (mx - PAD) / (W - 2 * PAD);
            var y = VIEW.lo + ((H - PAD) - my) / (H - 2 * PAD) * (VIEW.hi - VIEW.lo);
            y = clamp(y, VIEW.lo, VIEW.hi);   // view is frozen while dragging
            if (snap) { x = Math.round(x * 20) / 20; y = Math.round(y * 20) / 20; }
            var b = [S.b[0], S.b[1], S.b[2], S.b[3]];
            if (idx == 0) { b[0] = x; b[1] = y; } else { b[2] = x; b[3] = y; }
            setBez(b, true);
        }

        function endDrag() {
            if (dragging < 0) return;
            dragging = -1;
            S.sel = "";
            saveSettings();
            fitView(S.b);
            redraw();
            redrawThumbs();
            if (S.auto) doApply();
        }

        cv.addEventListener("mousedown", function (ev) {
            if (ev.button != 0) return;
            var best = -1, bd = HIT * HIT, i, p, dx, dy;
            for (i = 0; i < 2; i++) {
                p = handlePos(i);
                dx = ev.clientX - p[0]; dy = ev.clientY - p[1];
                if (dx * dx + dy * dy <= bd) { bd = dx * dx + dy * dy; best = i; }
            }
            if (best < 0) return;
            dragging = best;
            moveHandle(best, ev.clientX, ev.clientY, ev.shiftKey);
        });
        cv.addEventListener("mousemove", function (ev) {
            if (dragging < 0) return;
            moveHandle(dragging, ev.clientX, ev.clientY, ev.shiftKey);
        });
        cv.addEventListener("mouseup", function (ev) { endDrag(); });
        cv.addEventListener("mouseout", function (ev) { endDrag(); });

        // ---- other controls
        txt.onChange = function () {
            var m = this.text.match(/-?\d*\.?\d+(?:[eE]-?\d+)?/g);
            if (m && m.length >= 4) {
                S.sel = "";
                setBez([parseFloat(m[0]), parseFloat(m[1]), parseFloat(m[2]), parseFloat(m[3])]);
                redrawThumbs();
                if (S.auto) doApply();
            } else {
                txt.text = fmtBez(S.b);
                say("Need 4 numbers: x1, y1, x2, y2");
            }
        };
        btnApply.onClick = doApply;
        btnGet.onClick = function () {
            try {
                var r = readSelection();
                if (r.b) { S.sel = ""; setBez(r.b); redrawThumbs(); }
                say(r.msg);
            } catch (e) { say("Error: " + e.toString()); }
        };
        cbOut.onClick = function () { S.applyOut = this.value; saveSettings(); };
        cbIn.onClick = function () { S.applyIn = this.value; saveSettings(); };
        cbAuto.onClick = function () { S.auto = this.value; saveSettings(); };
        btnLin.onClick = function () { try { say(setInterp("Linear")); } catch (e) { say("Error: " + e.toString()); } };
        btnHold.onClick = function () { try { say(setInterp("Hold")); } catch (e) { say("Error: " + e.toString()); } };
        btnAuto.onClick = function () { try { say(setInterp("Auto Bezier")); } catch (e) { say("Error: " + e.toString()); } };
        ddFilter.onChange = function () {
            if (!this.selection) return;
            S.filter = FILTERS[this.selection.index];
            saveSettings();
            buildGrid();
        };
        btnSave.onClick = function () {
            var nm = prompt("Preset name:", "My curve", NAME);
            if (!nm) return;
            nm = nm.replace(/[|\r\n]/g, " ");
            var found = null;
            for (var i = 0; i < presets.length; i++) if (presets[i].custom && presets[i].name == nm) found = presets[i];
            if (found) found.b = [S.b[0], S.b[1], S.b[2], S.b[3]];
            else presets.push({ name: nm, cat: "Custom", b: [S.b[0], S.b[1], S.b[2], S.b[3]], pop: false, custom: true });
            if (saveCustom()) {
                S.sel = nm;
                S.filter = "Custom";
                for (var k = 0; k < FILTERS.length; k++) if (FILTERS[k] == "Custom") ddFilter.selection = k;
                buildGrid();
                say("Saved \"" + nm + "\"");
            }
        };
        btnDel.onClick = function () {
            for (var i = 0; i < presets.length; i++) {
                if (presets[i].custom && presets[i].name == S.sel) { removePreset(presets[i]); return; }
            }
            say("Click a custom preset first, then Del.");
        };

        var inResize = false;
        function relayout() {
            if (inResize) return;
            inResize = true;
            try {
                if (calcCols() != curCols) buildGrid();   // re-flow the preset thumbnails
                place();
            } finally {
                inResize = false;
            }
        }
        win.onResizing = win.onResize = relayout;

        // lay the rows' own children out once, then take over positioning from here
        rowA.layout.layout(true); rowO.layout.layout(true); rowI.layout.layout(true); rowP.layout.layout(true);
        win.layout = null;
        buildGrid();
        say("Select keyframes, shape the curve, press Apply.");

        // hooks for automated testing
        $.global.FlowCurvesAPI = {
            S: S, setBez: setBez, applyToSelection: applyToSelection, readSelection: readSelection,
            applyPair: applyPair, readPair: readPair, collectPairs: collectPairs, redraw: redraw,
            presets: function () { return presets; }, win: win, cv: cv, doApply: doApply,
            moveHandle: moveHandle, endDrag: endDrag
        };

        if (win instanceof Window) {
            win.preferredSize = [260, 520];
            win.size = [260, 520];
            win.center();
            win.show();
        }
        relayout();
        return win;
    }

    // re-running as a floating palette replaces the previous one
    try {
        if (!(thisObj instanceof Panel) && $.global.FlowCurvesWin && $.global.FlowCurvesWin.visible) {
            $.global.FlowCurvesWin.close();
        }
    } catch (e0) {}
    $.global.FlowCurvesWin = buildUI(thisObj);

})(this);
