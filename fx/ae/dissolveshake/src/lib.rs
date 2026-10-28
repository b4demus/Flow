//! F_DissolveShake: transition that dissolves the layer into the
//! Background while both shake hardest at the midpoint.
//! Parameters and behaviour: spec/effects/shake.md; the plugin body is shared with F_Shake.

use after_effects as ae;

#[path = "../../shake/src/common.rs"]
mod common;
use common::{Params, P};

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut P, _: InData, _: OutData) -> Result<(), Error> {
        common::params_setup(params, true)
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        out_data: OutData,
        params: &mut P,
    ) -> Result<(), ae::Error> {
        common::handle_command(
            cmd,
            in_data,
            out_data,
            params,
            true,
            "F_DissolveShake v0.1\rShake transition to the Background.\rb4demus",
        )
    }
}
