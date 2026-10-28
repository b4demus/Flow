include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../batch4");
    facet_plugin_ex(
        "F_FilmEffect",
        "B4DM Flow Film Effect",
        "Flow Stylize",
        true,
        true,
    );
}
