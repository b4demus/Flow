//! F_Sharpen. Shared body: ae/batch3/common.rs.

use after_effects as ae;

#[path = "../../batch3/common.rs"]
mod common;
use common::{Kind, Params, P};

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut P, _: InData, _: OutData) -> Result<(), Error> {
        common::params_setup(params, Kind::Sharpen)
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
            Kind::Sharpen,
            "F_Sharpen v0.1\rMultiscale edge-aware sharpening and fast unsharp mask.\rb4demus",
        )
    }
}
