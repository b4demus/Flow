include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_ReverseClip",
        "B4DM Flow Reverse Clip",
        "Flow Time",
        true,
        true,
    );
}
