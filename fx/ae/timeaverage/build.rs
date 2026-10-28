include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_TimeAverage",
        "B4DM Flow Time Average",
        "Flow Time",
        true,
        true,
    );
}
