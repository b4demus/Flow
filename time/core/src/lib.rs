//! flowtime: optical-flow retiming engine (clean-room, see docs/).

pub mod filters;
pub mod flow;
pub mod image;
pub mod interp;
pub mod prof;
pub mod retime;
pub mod timemap;

pub use filters::ImagePrep;
pub use flow::{compute_flow, FlowParams, Guide, Quality};
pub use image::{Flow, Gray, Image};
pub use interp::{FrameInterp, Warping};
pub use retime::{retime, Display, FgLayer, FlowCache, FlowPair, Frames, Settings};
pub use timemap::{pair, segment_override, source_from_frame_num, source_from_speed, Pair, PairOverride, RemapMode, Segment};
