use crate::PlayerId;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum PlayerCommand {
    Load(PathBuf),
    Pause(bool),
    Mute(bool),
    SeekRelative(f64),
    SeekAbsolute(f64),
    SetVolume(i64),
    Stop,
    Close,
    SetAlwaysOnTop(bool),
}

#[derive(Debug, Clone)]
pub enum PlayerEvent {
    Ready(PlayerId),
    Closed(PlayerId),
    Error { id: PlayerId, message: String },
}
