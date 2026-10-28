//! F_FlickerMatchColor. Shared body: ae/timefx/common.rs; behaviour: spec/effects/time.md.

use after_effects as ae;

#[path = "../../timefx/common.rs"]
mod common;
use common::{Kind, Params, P};

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut P, _: InData, _: OutData) -> Result<(), Error> {
        common::params_setup(params, Kind::MatchColor)
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        out_data: OutData,
        params: &mut P,
    ) -> Result<(), ae::Error> {
        common::handle_command(cmd, in_data, out_data, params, Kind::MatchColor, "F_FlickerMatchColor v0.1\rFlicker Match Color: copies the colour changes of another clip.\rb4demus")
    }
}
