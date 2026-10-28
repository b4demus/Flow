//! F_Shake: camera shake (Normal / Twitchy / Jumpy), optionally per RGB channel.
//! Parameters and behaviour: spec/effects/shake.md; the plugin body is in common.rs.

use after_effects as ae;

mod common;
use common::{Params, P};

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut P, _: InData, _: OutData) -> Result<(), Error> {
        common::params_setup(params, false)
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
            false,
            "F_Shake v0.1\rCamera shake: Normal, Twitchy, Jumpy.\rb4demus",
        )
    }
}
