include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../fx");
    facet_plugin_ex(
        "F_WarpTransform",
        "B4DM Flow Warp Transform",
        "Flow Distort",
        false,
        false,
    );
}
