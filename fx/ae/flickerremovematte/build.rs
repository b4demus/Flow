include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_FlickerRemoveMatte",
        "B4DM Flow Flicker Remove Matte",
        "Flow Time",
        false,
        false,
    );
}
