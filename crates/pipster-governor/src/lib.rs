//! Caps concurrent decode sessions and defines low-RAM mpv defaults.

use serde::{Deserialize, Serialize};

/// No practical cap — user can open as many windows as the machine allows.
pub const UNLIMITED_ACTIVE: usize = usize::MAX;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernorConfig {
    /// Maximum number of cores allowed in [`CoreState::Playing`].
    /// Default is [`UNLIMITED_ACTIVE`].
    #[serde(default = "unlimited_active")]
    pub max_active: usize,
    /// Mute newly spawned windows by default.
    pub mute_on_spawn: bool,
    /// Keep floating player windows always-on-top by default.
    pub always_on_top_default: bool,
    /// When over cap, pause the oldest playing core instead of rejecting spawn.
    #[serde(default)]
    pub pause_oldest_when_over_cap: bool,
}

fn unlimited_active() -> usize {
    UNLIMITED_ACTIVE
}

impl Default for GovernorConfig {
    fn default() -> Self {
        Self {
            max_active: UNLIMITED_ACTIVE,
            mute_on_spawn: true,
            always_on_top_default: true,
            pause_oldest_when_over_cap: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreState {
    /// Actively demuxing/decoding.
    Playing,
    /// Demux/decode stopped; last frame may still be shown.
    Paused,
    /// Decoder pools released; window shell may remain.
    Idle,
    /// Returned to the pool for reuse.
    Pooled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapDecision {
    /// Caller may start/resume decode.
    Allow,
    /// Pause `victim` first, then allow the new/active core.
    PauseOldest { victim_index_hint: usize },
    /// Reject starting another active decode.
    Reject,
}

/// Pure policy helper — no I/O.
#[derive(Debug, Clone)]
pub struct ResourceGovernor {
    config: GovernorConfig,
}

impl ResourceGovernor {
    pub fn new(config: GovernorConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &GovernorConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: GovernorConfig) {
        self.config = config;
    }

    /// `playing_count` is the number of cores currently in [`CoreState::Playing`].
    /// `oldest_playing_index` is an optional index into the caller's core list.
    pub fn request_play(
        &self,
        playing_count: usize,
        oldest_playing_index: Option<usize>,
    ) -> CapDecision {
        if self.config.max_active == UNLIMITED_ACTIVE || playing_count < self.config.max_active {
            return CapDecision::Allow;
        }
        if self.config.pause_oldest_when_over_cap {
            if let Some(idx) = oldest_playing_index {
                return CapDecision::PauseOldest {
                    victim_index_hint: idx,
                };
            }
        }
        CapDecision::Reject
    }

    pub fn mute_on_spawn(&self) -> bool {
        self.config.mute_on_spawn
    }

    pub fn always_on_top_default(&self) -> bool {
        self.config.always_on_top_default
    }
}

/// Low-RAM property profile for local-file playback.
///
/// Uses mpv's own OS window (`force-window`) instead of `vo=libmpv` embedding.
/// Embedding into Slint's OpenGL context SIGBUS'd on macOS + Homebrew mpv in this
/// project; native mpv windows keep multi-PiP and HW decode without sharing GL.
#[derive(Debug, Clone)]
pub struct MpvLowRamProfile {
    pub hwdec: &'static str,
    pub force_window: &'static str,
    pub demuxer_max_bytes: &'static str,
    pub demuxer_max_back_bytes: &'static str,
    pub cache: &'static str,
    pub vd_lavc_threads: &'static str,
}

impl Default for MpvLowRamProfile {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            hwdec: "videotoolbox",
            #[cfg(not(target_os = "macos"))]
            hwdec: "auto-safe",
            force_window: "yes",
            demuxer_max_bytes: "33554432",      // 32 MiB
            demuxer_max_back_bytes: "16777216", // 16 MiB
            cache: "no",
            vd_lavc_threads: "2",
        }
    }
}

impl MpvLowRamProfile {
    pub fn as_options(&self) -> [(&'static str, &'static str); 6] {
        [
            ("hwdec", self.hwdec),
            ("force-window", self.force_window),
            ("demuxer-max-bytes", self.demuxer_max_bytes),
            ("demuxer-max-back-bytes", self.demuxer_max_back_bytes),
            ("cache", self.cache),
            ("vd-lavc-threads", self.vd_lavc_threads),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_under_cap() {
        let g = ResourceGovernor::new(GovernorConfig {
            max_active: 2,
            ..Default::default()
        });
        assert_eq!(g.request_play(0, None), CapDecision::Allow);
        assert_eq!(g.request_play(1, Some(0)), CapDecision::Allow);
    }

    #[test]
    fn pauses_oldest_over_cap() {
        let g = ResourceGovernor::new(GovernorConfig {
            max_active: 2,
            pause_oldest_when_over_cap: true,
            ..Default::default()
        });
        assert_eq!(
            g.request_play(2, Some(3)),
            CapDecision::PauseOldest {
                victim_index_hint: 3
            }
        );
    }

    #[test]
    fn rejects_when_no_victim() {
        let g = ResourceGovernor::new(GovernorConfig {
            max_active: 1,
            pause_oldest_when_over_cap: false,
            ..Default::default()
        });
        assert_eq!(g.request_play(1, Some(0)), CapDecision::Reject);
    }
}
