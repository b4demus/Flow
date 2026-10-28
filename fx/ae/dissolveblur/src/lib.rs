//! F_DissolveBlur. Graph and native blur rules: spec/effects/dissolve-blur.md.
use after_effects as ae;
use facet_ae_util as u;
use facet_core::glow::AlphaMode;
use facet_core::transition::{dissolve_blur, DissolveBlurParams};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Background,
    Direction,
    Auto,
    Percent,
    Width,
    RelX,
    RelY,
    RelFrom,
    RelTo,
    Filter,
    Opacity,
}
type P<'a> = ae::Parameters<'a, Params>;
#[derive(Default)]
struct Plugin {}
ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, p: &mut P, _: InData, _: OutData) -> Result<(), Error> {
        use Params::*;
        p.add(Background, "Background", ae::LayerDef::new())?;
        p.add(
            Direction,
            "Transition Dir",
            u::popup(&["Dissolve Off to Bg", "Dissolve On from Bg"], 1),
        )?;
        p.add(
            Auto,
            "Auto Trans",
            u::popup(&["No: use Dissolve%", "Yes: ignore Dissolve%"], 1),
        )?;
        p.add(
            Percent,
            "Dissolve Percent",
            u::slider((0.0, 100.0), (0.0, 100.0), 0.0, 2),
        )?;
        p.add(
            Width,
            "Blur Amount",
            u::slider((0.0, 100000.0), (0.0, 480.0), 480.0, 2),
        )?;
        for (id, name, v) in [
            (RelX, "Blur Rel X", 1.0),
            (RelY, "Blur Rel Y", 0.0),
            (RelFrom, "Blur Rel From", 1.0),
            (RelTo, "Blur Rel To", 1.0),
        ] {
            p.add(id, name, u::slider((0.0, 100000.0), (0.0, 4.0), v, 3))?;
        }
        p.add(
            Filter,
            "Blur Filter",
            u::popup(&["Box", "Triangle", "Gauss"], 3),
        )?;
        p.add(
            Opacity,
            "Opacity",
            u::popup(&["All Opaque", "Normal", "As Premult"], 2),
        )?;
        Ok(())
    }
    fn handle_command(
        &mut self,
        cmd: ae::Command,
        input: InData,
        mut output: OutData,
        p: &mut P,
    ) -> Result<(), Error> {
        match cmd {
            ae::Command::About => output.set_return_msg(
                "F_DissolveBlur v0.1\rTransition with independent blur of both inputs.\rb4demus",
            ),
            ae::Command::SmartPreRender { mut extra } => u::guarded("pre_render", || {
                let side = [(u::layer_param_index(p, Params::Background), u::ID_BG)];
                u::pre_render(&input, &mut extra, (1e6, 1e6), &side)
            })?,
            ae::Command::SmartRender { extra } => u::guarded("smart_render", || {
                let setup = read(p, &input);
                u::smart_render(&extra, |src, side| {
                    dissolve_blur(&src.img, side[u::ID_BG as usize].as_ref(), &setup)
                })
            })?,
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => u::guarded("render", || {
                let setup = read(p, &input);
                u::legacy_render(&in_layer, &mut out_layer, |src| {
                    dissolve_blur(&src.img, None, &setup)
                })
            })?,
            _ => {}
        }
        Ok(())
    }
}
fn read(p: &P, input: &InData) -> DissolveBlurParams {
    use Params::*;
    let f = |id| facet_core::shake::fixed(u::get_float(p, id));
    let (dx, dy) = u::downsample(input);
    let amount = if u::get_popup(p, Auto) == 2 {
        let (n, _) = u::frame_number(input);
        let span = u::total_frames(input) - 1.0;
        if span > 0.0 {
            n / span
        } else {
            1.0
        }
    } else {
        f(Percent) / 100.0
    };
    DissolveBlurParams {
        amount,
        on_from_bg: u::get_popup(p, Direction) == 2,
        width: f(Width),
        rel_xy: [f(RelX) * dx, f(RelY) * dy],
        rel_from: f(RelFrom),
        rel_to: f(RelTo),
        passes: u::get_popup(p, Filter).clamp(1, 3) as usize,
        alpha_mode: match u::get_popup(p, Opacity) {
            1 => AlphaMode::AllOpaque,
            3 => AlphaMode::AsPremult,
            _ => AlphaMode::Normal,
        },
    }
}
