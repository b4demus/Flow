include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../batch3");
    facet_plugin_ex(
        "F_BlurMoCurves",
        "B4DM Flow Blur Mo Curves",
        "Flow Blur and Sharpen",
        true,
        true,
    );
}
