//! Background image pyramid jobs → main-thread events (Send-safe).

#![cfg(target_os = "macos")]

use parking_lot::Mutex;
use pipster_image::PyramidMeta;
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Debug)]
pub enum ImageJobEvent {
    OverviewReady {
        source: PathBuf,
        root: PathBuf,
        meta: PyramidMeta,
        always_on_top: bool,
    },
    PyramidComplete {
        root: PathBuf,
        meta: PyramidMeta,
    },
    Failed {
        message: String,
    },
}

fn queue() -> &'static Mutex<Vec<ImageJobEvent>> {
    static Q: OnceLock<Mutex<Vec<ImageJobEvent>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn push(event: ImageJobEvent) {
    queue().lock().push(event);
}

pub fn take() -> Vec<ImageJobEvent> {
    std::mem::take(&mut *queue().lock())
}

pub fn start_open(source: PathBuf, always_on_top: bool) {
    std::thread::spawn(
        move || match pipster_image::ensure_pyramid_overview(&source) {
            Ok((root, meta)) => {
                let root_for_build = root.clone();
                let source_for_build = source.clone();
                push(ImageJobEvent::OverviewReady {
                    source,
                    root,
                    meta,
                    always_on_top,
                });
                match pipster_image::continue_pyramid_build(&source_for_build, &root_for_build) {
                    Ok(meta) => push(ImageJobEvent::PyramidComplete {
                        root: root_for_build,
                        meta,
                    }),
                    Err(err) => push(ImageJobEvent::Failed {
                        message: format!("Pyramid build failed: {err}"),
                    }),
                }
            }
            Err(err) => push(ImageJobEvent::Failed {
                message: format!("Image tiles failed: {err}"),
            }),
        },
    );
}
