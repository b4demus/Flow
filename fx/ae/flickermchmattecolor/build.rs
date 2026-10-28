include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_FlickerMchMatteColor",
        "B4DM Flow Flicker Mch Matte Color",
        "Flow Time",
        false,
        false,
    );
}
