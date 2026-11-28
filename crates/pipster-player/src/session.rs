use pipster_governor::MpvLowRamProfile;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;
use uuid::Uuid;

fn pipster_osc_script() -> PathBuf {
    let path = std::env::temp_dir().join("pipster-osc.lua");
    let _ = fs::write(&path, include_str!("../scripts/osc.lua"));
    path
}

#[derive(Debug, Error)]
pub enum MpvSessionError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("mpv binary not found (install with: brew install mpv)")]
    MpvMissing,
    #[error("mpv ipc timeout")]
    IpcTimeout,
    #[error("mpv exited early")]
    Exited,
}

pub struct MpvProcess {
    child: Child,
    ipc_path: PathBuf,
}

impl MpvProcess {
    pub fn spawn(
        profile: &MpvLowRamProfile,
        ontop: bool,
        mute: bool,
    ) -> Result<Self, MpvSessionError> {
        let mpv = which_mpv()?;
        let ipc_path = std::env::temp_dir().join(format!("pipster-mpv-{}.sock", Uuid::new_v4()));
        let _ = fs::remove_file(&ipc_path);

        let mut cmd = Command::new(&mpv);
        cmd.arg("--idle=yes")
            .arg("--keep-open=yes")
            .arg("--force-window=yes")
            .arg(format!("--hwdec={}", profile.hwdec))
            .arg(format!("--demuxer-max-bytes={}", profile.demuxer_max_bytes))
            .arg(format!(
                "--demuxer-max-back-bytes={}",
                profile.demuxer_max_back_bytes
            ))
            .arg(format!("--cache={}", profile.cache))
            .arg(format!("--vd-lavc-threads={}", profile.vd_lavc_threads))
            .arg(format!("--input-ipc-server={}", ipc_path.display()))
            // Size window to the video aspect (no letterbox/pillarbox from a
            // forced geometry). Cap initial size so PiP stays manageable.
            .arg("--autofit=480")
            .arg("--keepaspect-window=yes")
            // Safari-like PiP: 20pt from the top-right of the screen.
            .arg("--geometry=-20+20")
            // Hide each mpv subprocess from the Dock — Pipster is the app;
            // mpv windows are just player surfaces.
            .arg("--macos-app-activation-policy=accessory")
            .arg("--title=Pipster")
            .arg("--osd-scale-by-window=no")
            .arg("--osd-scale=1.35")
            .arg("--no-osc")
            .arg(format!("--script={}", pipster_osc_script().display()))
            .arg("--script-opts=osc-vidscale=no,osc-scalewindowed=1.35,osc-layout=bottombar,osc-visibility=auto,osc-hidetimeout=2000")
            .arg("--no-terminal")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        if ontop {
            cmd.arg("--ontop");
        }
        if mute {
            cmd.arg("--mute=yes");
        }

        let child = cmd.spawn().map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                MpvSessionError::MpvMissing
            } else {
                MpvSessionError::Io(err)
            }
        })?;

        wait_for_socket(&ipc_path, Duration::from_secs(3))?;

        Ok(Self { child, ipc_path })
    }

    pub fn ipc_command(&self, command: &[&str]) -> Result<(), MpvSessionError> {
        let mut stream = UnixStream::connect(&self.ipc_path)?;
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        stream.set_write_timeout(Some(Duration::from_millis(500)))?;
        let payload = serde_json::json!({ "command": command });
        writeln!(stream, "{payload}")?;
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf);
        Ok(())
    }

    pub fn loadfile(&self, path: &Path) -> Result<(), MpvSessionError> {
        let path_str = path.to_string_lossy();
        self.ipc_command(&["loadfile", path_str.as_ref(), "replace"])
    }

    pub fn set_property(&self, name: &str, value: &str) -> Result<(), MpvSessionError> {
        self.ipc_command(&["set_property", name, value])
    }

    #[allow(dead_code)]
    pub fn quit(&mut self) {
        let _ = self.ipc_command(&["quit"]);
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.ipc_path);
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for MpvProcess {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = fs::remove_file(&self.ipc_path);
    }
}

fn which_mpv() -> Result<PathBuf, MpvSessionError> {
    for candidate in ["/opt/homebrew/bin/mpv", "/usr/local/bin/mpv", "mpv"] {
        let path = PathBuf::from(candidate);
        if candidate == "mpv" {
            // Rely on PATH via Command; existence checked at spawn.
            return Ok(path);
        }
        if path.exists() {
            return Ok(path);
        }
    }
    Err(MpvSessionError::MpvMissing)
}

fn wait_for_socket(path: &Path, timeout: Duration) -> Result<(), MpvSessionError> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            // Brief settle for listen().
            thread::sleep(Duration::from_millis(30));
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(MpvSessionError::IpcTimeout)
}
