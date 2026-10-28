include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_FlickerMatch",
        "B4DM Flow Flicker Match",
        "Flow Time",
        false,
        false,
    );
}
