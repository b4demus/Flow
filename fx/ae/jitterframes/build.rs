include!("../build_common.rs");

fn main() {
    println!("cargo:rerun-if-changed=../timefx");
    facet_plugin_ex(
        "F_JitterFrames",
        "B4DM Flow Jitter Frames",
        "Flow Time",
        true,
        true,
    );
}
