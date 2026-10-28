include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_FlickerMatchColor",
        "B4DM Flow Flicker Match Color",
        "Flow Time",
        false,
        false,
    );
}
