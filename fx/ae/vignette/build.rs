include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../fx");
    facet_plugin_ex(
        "F_Vignette",
        "B4DM Flow Vignette",
        "Flow Stylize",
        false,
        false,
    );
}
