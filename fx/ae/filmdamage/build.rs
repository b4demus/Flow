include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../batch4");
    facet_plugin_ex(
        "F_FilmDamage",
        "B4DM Flow Film Damage",
        "Flow Stylize",
        true,
        false,
    );
}
