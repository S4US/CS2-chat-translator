//! Windows records a single process tree through WASAPI process loopback
//! (Windows 11 or a recent Windows 10).

use super::{is_game_name, CaptureEvent};
use ::wasapi::{initialize_mta, AudioClient, Direction, SampleType, StreamMode, WaveFormat};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
};
use windows::Win32::{
    Foundation::CloseHandle,
    System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    },
};

pub const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const BYTES_PER_FRAME: usize = CHANNELS * 4;
const EVENT_TIMEOUT_MS: u32 = 100;

pub struct Backend;

impl Backend {
    pub fn connect() -> Result<Self, String> {
        Ok(Self)
    }

    /// Returns the process id of the running game.
    pub fn find_game(&mut self) -> Result<Option<u32>, String> {
        find_process()
    }

    pub fn attach(&self, process_id: u32, sender: SyncSender<CaptureEvent>) -> Result<Stream, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("game-audio-capture".to_string())
            .spawn(move || capture_process(process_id, sender, thread_stop, ready_tx))
            .map_err(|err| err.to_string())?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Stream {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(err)) => {
                let _ = thread.join();
                Err(err)
            }
            Err(_) => Err("The capture thread stopped during startup".to_string()),
        }
    }
}

/// A running process loopback capture. Capture stops when this is dropped.
pub struct Stream {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn capture_process(
    process_id: u32,
    sender: SyncSender<CaptureEvent>,
    stop: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<(), String>>,
) {
    let setup = || -> Result<_, String> {
        initialize_mta().ok().map_err(|err| err.to_string())?;
        // Include child processes, as the game may play audio from helpers.
        let mut client = AudioClient::new_application_loopback_client(process_id, true)
            .map_err(|err| err.to_string())?;
        let format = WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE as usize, CHANNELS, None);
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: 0,
        };
        client
            .initialize_client(&format, &Direction::Capture, &mode)
            .map_err(|err| err.to_string())?;
        let event = client.set_get_eventhandle().map_err(|err| err.to_string())?;
        let capture = client.get_audiocaptureclient().map_err(|err| err.to_string())?;
        client.start_stream().map_err(|err| err.to_string())?;
        Ok((client, capture, event))
    };

    let (client, capture, event) = match setup() {
        Ok(parts) => {
            let _ = ready.send(Ok(()));
            parts
        }
        Err(err) => {
            let _ = ready.send(Err(err));
            return;
        }
    };

    let mut queue = VecDeque::new();
    while !stop.load(Ordering::Relaxed) {
        // The event does not fire while the game is silent.
        if event.wait_for_event(EVENT_TIMEOUT_MS).is_err() {
            continue;
        }

        loop {
            match capture.get_next_packet_size() {
                Ok(Some(frames)) if frames > 0 => {}
                Ok(_) => break,
                Err(err) => {
                    let _ = sender.try_send(CaptureEvent::Failed(err.to_string()));
                    return;
                }
            }
            if let Err(err) = capture.read_from_device_to_deque(&mut queue) {
                let _ = sender.try_send(CaptureEvent::Failed(err.to_string()));
                return;
            }
        }

        let complete = queue.len() - queue.len() % BYTES_PER_FRAME;
        let bytes: Vec<u8> = queue.drain(..complete).collect();
        let mono = bytes
            .chunks_exact(BYTES_PER_FRAME)
            .map(|frame| {
                let sum: f32 = frame
                    .chunks_exact(4)
                    .map(|sample| f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]))
                    .sum();
                sum / CHANNELS as f32
            })
            .collect();
        // Dropping audio is better than blocking capture when the pipeline lags.
        let _ = sender.try_send(CaptureEvent::Samples(mono));
    }

    let _ = client.stop_stream();
}

fn find_process() -> Result<Option<u32>, String> {
    // SAFETY: the snapshot handle is closed before returning, and the entry
    // size is set as the API requires.
    unsafe {
        let snapshot =
            CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|err| err.to_string())?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let mut found = None;
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let name_len = entry
                .szExeFile
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(entry.szExeFile.len());
            if is_game_name(&String::from_utf16_lossy(&entry.szExeFile[..name_len])) {
                found = Some(entry.th32ProcessID);
                break;
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }

        let _ = CloseHandle(snapshot);
        Ok(found)
    }
}
