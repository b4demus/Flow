include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../batch3");
    facet_plugin_ex(
        "F_Sharpen",
        "B4DM Flow Sharpen",
        "Flow Blur and Sharpen",
        false,
        false,
    );
}
