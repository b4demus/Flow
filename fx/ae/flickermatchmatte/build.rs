include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_FlickerMatchMatte",
        "B4DM Flow Flicker Match Matte",
        "Flow Time",
        false,
        false,
    );
}
