include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../warpfx");
    facet_plugin_ex(
        "F_WarpChroma",
        "B4DM Flow Warp Chroma",
        "Flow Distort",
        false,
        false,
    );
}
