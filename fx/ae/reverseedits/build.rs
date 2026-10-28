include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_ReverseEdits",
        "B4DM Flow Reverse Edits",
        "Flow Time",
        true,
        true,
    );
}
