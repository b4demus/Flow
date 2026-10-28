include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_GetFrame",
        "B4DM Flow Get Frame",
        "Flow Time",
        true,
        true,
    );
}
