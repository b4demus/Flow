include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../batch3");
    facet_plugin_ex("F_Rays", "B4DM Flow Rays", "Flow Lighting", true, false);
}
