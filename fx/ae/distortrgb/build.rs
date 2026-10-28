include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../warpfx");
    facet_plugin_ex(
        "F_DistortRGB",
        "B4DM Flow Distort RGB",
        "Flow Distort",
        false,
        false,
    );
}
