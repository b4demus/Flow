include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../batch4");
    facet_plugin_ex(
        "F_LensFlare",
        "B4DM Flow Lens Flare",
        "Flow Lighting",
        true,
        false,
    );
}
