//! F_DigitalDamage. Shared body: ae/batch4/common.rs.

use after_effects as ae;

#[path = "../../batch4/common.rs"]
mod common;
use common::{Kind, Params, P};

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut P, _: InData, _: OutData) -> Result<(), Error> {
        common::params_setup(params, Kind::DigitalDamage)
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        out_data: OutData,
        params: &mut P,
    ) -> Result<(), ae::Error> {
        common::handle_command(cmd, in_data, out_data, params, Kind::DigitalDamage, "F_DigitalDamage v0.1\rSeven digital damage stages with temporal freeze and flow.\rb4demus")
    }
}
