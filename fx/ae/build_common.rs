// Shared build script body for the facet AE effects: include!("../build_common.rs")
// and call `facet_plugin(name, match_name, category, time_varying)` from main().
// `time_varying`: the output changes over time with constant parameters
// (animated phase or noise), so AE must not reuse one frame for every time.

#[allow(dead_code)]
fn facet_plugin(name: &'static str, match_name: &'static str, category: &'static str, time_varying: bool) {
    facet_plugin_ex(name, match_name, category, time_varying, false);
}

/// `wide_time`: the effect reads other frames of its layer.
#[allow(dead_code)]
fn facet_plugin_ex(name: &'static str, match_name: &'static str, category: &'static str, time_varying: bool, wide_time: bool) {
    use pipl::*;
    println!("cargo:rustc-cfg=catch_panics");
    // The PiPL build number changes with every build: AE keeps frames of an
    // effect on disk across sessions and would otherwise reuse frames rendered
    // by an older build.
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=../build_common.rs");
    println!("cargo:rerun-if-changed=../util/src");
    println!("cargo:rerun-if-changed=../../core/src");
    let build = ((id / 60) % 511 + 1) as u32;
    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name(name),
        Property::Category(category),
        Property::CodeWin64X86("EffectMain"),
        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version { major: 13, minor: 28 },
        Property::AE_Effect_Version { version: 0, subversion: 1, bugversion: 0, stage: Stage::Develop, build },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags({
            let mut f = OutFlags::DeepColorAware;
            if time_varying {
                f |= OutFlags::NonParamVary;
            }
            if wide_time {
                f |= OutFlags::WideTimeInput;
            }
            f
        }),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::SupportsSmartRender | OutFlags2::FloatColorAware | OutFlags2::SupportsThreadedRendering | OutFlags2::ParamGroupStartCollapsedFlag,
        ),
        Property::AE_Effect_Match_Name(match_name),
        Property::AE_Reserved_Info(0),
        Property::AE_Effect_Support_URL("https://github.com/b4demus"),
    ]);
    // winres hands the PiPL to the GNU linker as a static library; nothing
    // references it, so it gets dropped. Link the object itself instead.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
        let out = std::env::var("OUT_DIR").unwrap();
        println!("cargo:rustc-cdylib-link-arg={out}/resource.o");
    }
}
