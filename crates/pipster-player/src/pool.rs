use crate::core::PlayerCore;
use crate::session::MpvSessionError;
use crate::{CapDecision, CoreState, GovernorConfig, MpvLowRamProfile, PlayerId, ResourceGovernor};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

struct PoolInner {
    active: Vec<PlayerCore>,
    pooled: VecDeque<PlayerCore>,
    governor: ResourceGovernor,
    profile: MpvLowRamProfile,
}

#[derive(Clone)]
pub struct PlayerCorePool {
    inner: Arc<Mutex<PoolInner>>,
}

impl PlayerCorePool {
    pub fn new(governor: ResourceGovernor, profile: MpvLowRamProfile) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PoolInner {
                active: Vec::new(),
                pooled: VecDeque::new(),
                governor,
                profile,
            })),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(
            ResourceGovernor::new(GovernorConfig::default()),
            MpvLowRamProfile::default(),
        )
    }

    pub fn set_governor_config(&self, config: GovernorConfig) {
        self.inner.lock().governor.set_config(config);
    }

    pub fn governor_config(&self) -> GovernorConfig {
        self.inner.lock().governor.config().clone()
    }

    pub fn active_count(&self) -> usize {
        self.inner.lock().active.len()
    }

    pub fn playing_count(&self) -> usize {
        self.inner
            .lock()
            .active
            .iter()
            .filter(|c| c.state() == CoreState::Playing)
            .count()
    }

    pub fn acquire(&self) -> Result<(PlayerId, CapDecision), MpvSessionError> {
        let mut inner = self.inner.lock();
        let mute = inner.governor.mute_on_spawn();
        let aot = inner.governor.always_on_top_default();

        let playing: Vec<(usize, _)> = inner
            .active
            .iter()
            .enumerate()
            .filter(|(_, c)| c.state() == CoreState::Playing)
            .map(|(i, c)| (i, c.playing_since()))
            .collect();
        let playing_count = playing.len();
        let oldest = playing
            .into_iter()
            .min_by_key(|(_, since)| *since)
            .map(|(i, _)| i);

        let decision = inner.governor.request_play(playing_count, oldest);
        match decision {
            CapDecision::PauseOldest {
                victim_index_hint: idx,
            } => {
                if let Some(core) = inner.active.get_mut(idx) {
                    let _ = core.set_paused(true);
                }
            }
            CapDecision::Reject => return Ok((PlayerId::new(), CapDecision::Reject)),
            CapDecision::Allow => {}
        }

        let core = if let Some(mut recycled) = inner.pooled.pop_front() {
            recycled.revive_from_pool(mute, aot);
            recycled
        } else {
            PlayerCore::create(&inner.profile, mute, aot)?
        };

        let id = core.id();
        inner.active.push(core);
        Ok((id, decision))
    }

    pub fn acquire_and_load(
        &self,
        path: Option<&Path>,
    ) -> Result<(PlayerId, CapDecision), MpvSessionError> {
        let (id, decision) = self.acquire()?;
        if matches!(decision, CapDecision::Reject) {
            return Ok((id, decision));
        }
        if let Some(path) = path {
            let _ = self.with_core_mut(id, |c| c.load(path));
        }
        Ok((id, decision))
    }

    pub fn with_core_mut<R>(
        &self,
        id: PlayerId,
        f: impl FnOnce(&mut PlayerCore) -> R,
    ) -> Option<R> {
        let mut inner = self.inner.lock();
        let core = inner.active.iter_mut().find(|c| c.id() == id)?;
        Some(f(core))
    }

    pub fn with_core<R>(&self, id: PlayerId, f: impl FnOnce(&PlayerCore) -> R) -> Option<R> {
        let inner = self.inner.lock();
        let core = inner.active.iter().find(|c| c.id() == id)?;
        Some(f(core))
    }

    pub fn ids(&self) -> Vec<PlayerId> {
        self.inner.lock().active.iter().map(|c| c.id()).collect()
    }

    pub fn release(&self, id: PlayerId) -> bool {
        let mut inner = self.inner.lock();
        let Some(pos) = inner.active.iter().position(|c| c.id() == id) else {
            return false;
        };
        let mut core = inner.active.remove(pos);
        let _ = core.enter_idle();
        core.mark_pooled();
        if inner.pooled.len() < 4 {
            inner.pooled.push_back(core);
        }
        true
    }

    pub fn pause_all(&self) {
        let mut inner = self.inner.lock();
        for core in &mut inner.active {
            let _ = core.set_paused(true);
        }
    }

    pub fn enforce_cap(&self) {
        let mut inner = self.inner.lock();
        let max = inner.governor.config().max_active;
        loop {
            let mut playing: Vec<(usize, _)> = inner
                .active
                .iter()
                .enumerate()
                .filter(|(_, c)| c.state() == CoreState::Playing)
                .map(|(i, c)| (i, c.playing_since()))
                .collect();
            if playing.len() <= max {
                break;
            }
            playing.sort_by_key(|(_, since)| *since);
            if let Some((idx, _)) = playing.first() {
                let _ = inner.active[*idx].set_paused(true);
            } else {
                break;
            }
        }
    }

    /// Reap players whose mpv process exited.
    pub fn reap_dead(&self) -> Vec<PlayerId> {
        let mut inner = self.inner.lock();
        let mut dead = Vec::new();
        inner.active.retain_mut(|c| {
            if c.is_running() {
                true
            } else {
                dead.push(c.id());
                false
            }
        });
        dead
    }
}
