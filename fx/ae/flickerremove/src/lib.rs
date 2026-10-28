//! F_FlickerRemove. Shared body: ae/timefx/common.rs; behaviour: spec/effects/time.md.

use after_effects as ae;

#[path = "../../timefx/common.rs"]
mod common;
use common::{Kind, Params, P};

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut P, _: InData, _: OutData) -> Result<(), Error> {
        common::params_setup(params, Kind::Remove)
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        out_data: OutData,
        params: &mut P,
    ) -> Result<(), ae::Error> {
        common::handle_command(cmd, in_data, out_data, params, Kind::Remove, "F_FlickerRemove v0.1\rFlicker Remove: holds the average level in a rectangle.\rb4demus")
    }
}
