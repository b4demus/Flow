include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../fx");
    facet_plugin_ex(
        "F_DuoTone",
        "B4DM Flow Duo Tone",
        "Flow Adjust",
        false,
        false,
    );
}
