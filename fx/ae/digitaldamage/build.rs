include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../batch4");
    facet_plugin_ex(
        "F_DigitalDamage",
        "B4DM Flow Digital Damage",
        "Flow Stylize",
        true,
        true,
    );
}
