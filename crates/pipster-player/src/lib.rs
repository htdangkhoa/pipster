//! Player cores backed by **mpv subprocesses** + IPC (avoids in-process libmpv+Slint SIGBUS on macOS).

mod commands;
mod core;
mod pool;
mod session;

pub use commands::{PlayerCommand, PlayerEvent};
pub use core::{PlayerCore, PlayerId};
pub use pipster_governor::{
    CapDecision, CoreState, GovernorConfig, MpvLowRamProfile, ResourceGovernor,
};
pub use pool::PlayerCorePool;
pub use session::MpvSessionError;
