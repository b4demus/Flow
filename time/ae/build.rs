use pipl::*;

fn main() {
    println!("cargo:rustc-cfg=catch_panics");
    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("Flowtime"),
        Property::Category("Flow Time"),
        Property::CodeWin64X86("EffectMain"),
        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version { major: 13, minor: 28 },
        Property::AE_Effect_Version {
            version: 0,
            subversion: 1,
            bugversion: 0,
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(OutFlags::WideTimeInput | OutFlags::DeepColorAware),
        Property::AE_Effect_Global_OutFlags_2(OutFlags2::ParamGroupStartCollapsedFlag | OutFlags2::SupportsThreadedRendering),
        Property::AE_Effect_Match_Name("B4DM Flowtime"),
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
