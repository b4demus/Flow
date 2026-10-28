// Lists the effects After Effects registered that match Flowtime / Twixtor and
// writes them to check_effects.json next to this script.
(function () {
    var out = [];
    for (var i = 0; i < app.effects.length; i++) {
        var e = app.effects[i];
        if (/flowtime|twixtor/i.test(e.displayName + " " + e.matchName)) {
            out.push('{"name":"' + e.displayName + '","match":"' + e.matchName + '","category":"' + e.category + '"}');
        }
    }
    var f = new File(new File($.fileName).parent.fsName + "/check_effects.json");
    f.encoding = "UTF-8";
    f.open("w");
    f.write('{"version":"' + app.version + '","effects":[' + out.join(",") + "]}");
    f.close();
})();
