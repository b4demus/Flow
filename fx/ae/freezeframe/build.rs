include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_FreezeFrame",
        "B4DM Flow Freeze Frame",
        "Flow Time",
        true,
        true,
    );
}
