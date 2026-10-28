include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_TimeWarpRGB",
        "B4DM Flow Time Warp RGB",
        "Flow Time",
        true,
        true,
    );
}
