include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_RandomEdits",
        "B4DM Flow Random Edits",
        "Flow Time",
        true,
        true,
    );
}
