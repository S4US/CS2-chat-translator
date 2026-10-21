//! Captures only the game's audio instead of a whole output device, so music,
//! Discord and other apps are not transcribed.

#[cfg(target_os = "linux")]
mod pulse;
#[cfg(target_os = "windows")]
mod wasapi;

#[cfg(target_os = "linux")]
use pulse as platform;
#[cfg(target_os = "windows")]
use wasapi as platform;

use super::capture::CaptureEvent;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Audio source id that selects game audio instead of a device.
pub const SOURCE_ID: &str = "app:cs2";
pub const DISPLAY_NAME: &str = "CS2";
/// Names the game uses for its audio stream (Linux) or process (Windows).
const GAME_NAMES: [&str; 2] = ["cs2", "cs2.exe"];
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(100);
const MAX_ATTACH_FAILURES: u32 = 3;

/// Follows the game's audio while it runs. Capture stops when this is dropped.
pub struct GameCapture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for GameCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Connects to the audio system and starts waiting for the game. Samples
/// arrive as mono f32 at the returned sample rate.
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn open(sender: SyncSender<CaptureEvent>) -> Result<(GameCapture, u32), String> {
    let backend = platform::Backend::connect()?;
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let thread = thread::Builder::new()
        .name("game-audio".to_string())
        .spawn(move || follow_game(backend, sender, thread_stop))
        .map_err(|err| err.to_string())?;

    Ok((
        GameCapture {
            stop,
            thread: Some(thread),
        },
        platform::SAMPLE_RATE,
    ))
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn open(_sender: SyncSender<CaptureEvent>) -> Result<(GameCapture, u32), String> {
    Err("Capturing only CS2 audio is not supported on this platform".to_string())
}

/// Attaches to the game's audio when it appears and reattaches when the game
/// restarts or recreates its stream.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn follow_game(
    mut backend: platform::Backend,
    sender: SyncSender<CaptureEvent>,
    stop: Arc<AtomicBool>,
) {
    let mut attached: Option<(u32, platform::Stream)> = None;
    let mut reported_waiting = None;
    let mut attach_failures = 0;

    while !stop.load(Ordering::Relaxed) {
        let target = match backend.find_game() {
            Ok(target) => target,
            Err(err) => {
                let _ = sender.send(CaptureEvent::Failed(err));
                return;
            }
        };

        if attached.as_ref().map(|(id, _)| *id) != target {
            attached = None;
            if let Some(id) = target {
                match backend.attach(id, sender.clone()) {
                    Ok(stream) => {
                        attached = Some((id, stream));
                        attach_failures = 0;
                    }
                    Err(err) => {
                        attach_failures += 1;
                        if attach_failures >= MAX_ATTACH_FAILURES {
                            let _ = sender.send(CaptureEvent::Failed(format!(
                                "Could not capture {DISPLAY_NAME} audio: {err}"
                            )));
                            return;
                        }
                    }
                }
            }
        }

        let waiting = attached.is_none();
        if reported_waiting != Some(waiting) {
            // A closed channel means the pipeline has stopped.
            if sender.send(CaptureEvent::Waiting(waiting)).is_err() {
                return;
            }
            reported_waiting = Some(waiting);
        }

        let polled = Instant::now();
        while polled.elapsed() < POLL_INTERVAL && !stop.load(Ordering::Relaxed) {
            thread::sleep(STOP_CHECK_INTERVAL);
        }
    }
}

fn is_game_name(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    GAME_NAMES.contains(&name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_game_names_only() {
        assert!(is_game_name("cs2"));
        assert!(is_game_name("CS2.exe"));
        assert!(!is_game_name("cs2-translator"));
        assert!(!is_game_name("Firefox"));
        assert!(!is_game_name(""));
    }
}
