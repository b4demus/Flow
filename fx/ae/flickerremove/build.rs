include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_FlickerRemove",
        "B4DM Flow Flicker Remove",
        "Flow Time",
        false,
        false,
    );
}
