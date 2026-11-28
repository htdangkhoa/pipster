use crate::session::{MpvProcess, MpvSessionError};
use crate::{CoreState, MpvLowRamProfile, PlayerCommand};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerId(Uuid);

impl PlayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PlayerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PlayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct PlayerCore {
    id: PlayerId,
    proc: MpvProcess,
    state: CoreState,
    path: Option<PathBuf>,
    muted: bool,
    always_on_top: bool,
    playing_since: Option<Instant>,
    last_activity: Instant,
}

impl PlayerCore {
    pub fn create(
        profile: &MpvLowRamProfile,
        mute: bool,
        always_on_top: bool,
    ) -> Result<Self, MpvSessionError> {
        let proc = MpvProcess::spawn(profile, always_on_top, mute)?;
        Ok(Self {
            id: PlayerId::new(),
            proc,
            state: CoreState::Idle,
            path: None,
            muted: mute,
            always_on_top,
            playing_since: None,
            last_activity: Instant::now(),
        })
    }

    pub fn id(&self) -> PlayerId {
        self.id
    }

    pub fn state(&self) -> CoreState {
        self.state
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn always_on_top(&self) -> bool {
        self.always_on_top
    }

    pub fn set_always_on_top(&mut self, on: bool) {
        self.always_on_top = on;
        let _ = self
            .proc
            .set_property("ontop", if on { "yes" } else { "no" });
    }

    pub fn playing_since(&self) -> Option<Instant> {
        self.playing_since
    }

    pub fn last_activity(&self) -> Instant {
        self.last_activity
    }

    pub fn is_running(&mut self) -> bool {
        self.proc.is_running()
    }

    pub fn apply(&mut self, cmd: PlayerCommand) -> Result<(), MpvSessionError> {
        self.last_activity = Instant::now();
        match cmd {
            PlayerCommand::Load(path) => self.load(&path)?,
            PlayerCommand::Pause(paused) => self.set_paused(paused)?,
            PlayerCommand::Mute(muted) => self.set_muted(muted)?,
            PlayerCommand::SeekRelative(delta) => {
                self.proc
                    .ipc_command(&["seek", &delta.to_string(), "relative"])?;
            }
            PlayerCommand::SeekAbsolute(pos) => {
                self.proc
                    .ipc_command(&["seek", &pos.to_string(), "absolute"])?;
            }
            PlayerCommand::SetVolume(vol) => {
                self.proc.set_property("volume", &vol.to_string())?;
            }
            PlayerCommand::Stop => self.stop()?,
            PlayerCommand::Close => self.stop()?,
            PlayerCommand::SetAlwaysOnTop(on) => self.set_always_on_top(on),
        }
        Ok(())
    }

    pub fn load(&mut self, path: &Path) -> Result<(), MpvSessionError> {
        self.load_inner(path, false)
    }

    /// Load a still image: keep the frame on screen (mpv defaults to ~1s).
    pub fn load_still(&mut self, path: &Path) -> Result<(), MpvSessionError> {
        self.load_inner(path, true)
    }

    fn load_inner(&mut self, path: &Path, still: bool) -> Result<(), MpvSessionError> {
        if still {
            let _ = self.proc.set_property("loop-file", "inf");
            let _ = self.proc.set_property("image-display-duration", "inf");
        }
        self.proc.loadfile(path)?;
        self.path = Some(path.to_path_buf());
        self.enter_playing();
        Ok(())
    }

    pub fn set_paused(&mut self, paused: bool) -> Result<(), MpvSessionError> {
        self.proc
            .set_property("pause", if paused { "yes" } else { "no" })?;
        if paused {
            self.state = CoreState::Paused;
            self.playing_since = None;
        } else {
            self.enter_playing();
        }
        Ok(())
    }

    pub fn set_muted(&mut self, muted: bool) -> Result<(), MpvSessionError> {
        self.proc
            .set_property("mute", if muted { "yes" } else { "no" })?;
        self.muted = muted;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), MpvSessionError> {
        let _ = self.proc.ipc_command(&["stop"]);
        self.state = CoreState::Idle;
        self.playing_since = None;
        self.path = None;
        Ok(())
    }

    pub fn enter_idle(&mut self) -> Result<(), MpvSessionError> {
        self.stop()
    }

    pub fn mark_pooled(&mut self) {
        self.state = CoreState::Pooled;
        self.playing_since = None;
        self.path = None;
    }

    /// Recycled cores keep the same OS process; just reset metadata + mute/ontop.
    pub fn revive_from_pool(&mut self, mute: bool, always_on_top: bool) {
        self.id = PlayerId::new();
        self.state = CoreState::Idle;
        self.muted = mute;
        self.always_on_top = always_on_top;
        self.playing_since = None;
        self.last_activity = Instant::now();
        let _ = self.set_muted(mute);
        self.set_always_on_top(always_on_top);
    }

    pub fn idle_for(&self) -> Duration {
        self.last_activity.elapsed()
    }

    fn enter_playing(&mut self) {
        self.state = CoreState::Playing;
        self.playing_since = Some(Instant::now());
    }
}
