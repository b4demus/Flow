include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex("F_Flicker", "B4DM Flow Flicker", "Flow Time", true, false);
}
