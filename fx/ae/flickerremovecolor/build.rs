include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_FlickerRemoveColor",
        "B4DM Flow Flicker Remove Color",
        "Flow Time",
        false,
        false,
    );
}
