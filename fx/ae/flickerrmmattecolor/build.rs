include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_FlickerRmMatteColor",
        "B4DM Flow Flicker Rm Matte Color",
        "Flow Time",
        false,
        false,
    );
}
