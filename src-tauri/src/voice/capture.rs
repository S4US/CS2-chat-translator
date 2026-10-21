use super::game_audio;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, DeviceId, ErrorKind, FromSample, Host, Sample, SampleFormat, SizedSample, Stream,
    StreamConfig,
};
use serde::Serialize;
use std::sync::mpsc::SyncSender;
use std::time::Duration;

const STREAM_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioDeviceKind {
    Input,
    Monitor,
    Loopback,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub kind: AudioDeviceKind,
}

pub enum CaptureEvent {
    Samples(Vec<f32>),
    /// Game audio only: whether the game's audio is missing, for example
    /// because the game is not running.
    Waiting(bool),
    Failed(String),
}

/// Holds the stream so audio stops when the capture is dropped.
#[allow(dead_code)]
enum Source {
    Device(Stream),
    Game(game_audio::GameCapture),
}

/// A running capture stream. Audio stops when this is dropped.
pub struct Capture {
    _source: Source,
    pub sample_rate: u32,
    pub device_name: String,
}

/// Lists devices that can carry game audio. Linux exposes sink monitors as
/// sources. On Windows, output devices are captured through WASAPI loopback.
pub fn list_devices() -> Result<Vec<AudioDevice>, String> {
    let host = cpal::default_host();
    let mut devices: Vec<AudioDevice> = host
        .input_devices()
        .map_err(|err| err.to_string())?
        .filter_map(|device| describe(&device, false))
        .collect();

    if cfg!(target_os = "windows") {
        devices.extend(
            host.output_devices()
                .map_err(|err| err.to_string())?
                .filter_map(|device| describe(&device, true)),
        );
    }

    devices.sort_by_key(|device| match device.kind {
        AudioDeviceKind::Monitor | AudioDeviceKind::Loopback => 0,
        AudioDeviceKind::Input => 1,
    });
    Ok(devices)
}

pub fn open(device_id: Option<&str>, sender: SyncSender<CaptureEvent>) -> Result<Capture, String> {
    if device_id == Some(game_audio::SOURCE_ID) {
        let (game, sample_rate) = game_audio::open(sender)?;
        return Ok(Capture {
            _source: Source::Game(game),
            sample_rate,
            device_name: game_audio::DISPLAY_NAME.to_string(),
        });
    }

    let host = cpal::default_host();
    let device = match device_id.filter(|id| !id.is_empty()) {
        Some(id) => find_device(&host, id)?,
        None => default_game_audio_device(&host)
            .ok_or_else(|| "No audio device found for game audio".to_string())?,
    };
    let device_name = display_name(&device);

    let loopback = !device.supports_input();
    let supported = if loopback {
        device.default_output_config()
    } else {
        device.default_input_config()
    }
    .map_err(|err| format!("{device_name}: {err}"))?;

    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(&device, &config, sender),
        SampleFormat::I16 => build_stream::<i16>(&device, &config, sender),
        SampleFormat::I32 => build_stream::<i32>(&device, &config, sender),
        SampleFormat::U16 => build_stream::<u16>(&device, &config, sender),
        other => Err(format!("Unsupported sample format {other}")),
    }
    .map_err(|err| format!("{device_name}: {err}"))?;
    stream
        .play()
        .map_err(|err| format!("{device_name}: {err}"))?;

    Ok(Capture {
        _source: Source::Device(stream),
        sample_rate: config.sample_rate,
        device_name,
    })
}

fn build_stream<T>(
    device: &Device,
    config: &StreamConfig,
    sender: SyncSender<CaptureEvent>,
) -> Result<Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels).max(1);
    let error_sender = sender.clone();

    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| {
                let mono = data
                    .chunks(channels)
                    .map(|frame| {
                        let sum: f32 = frame.iter().map(|sample| f32::from_sample(*sample)).sum();
                        sum / frame.len() as f32
                    })
                    .collect();
                // Dropping audio is better than blocking the audio thread when the pipeline lags.
                let _ = sender.try_send(CaptureEvent::Samples(mono));
            },
            move |err: cpal::Error| {
                let fatal = matches!(
                    err.kind(),
                    ErrorKind::DeviceNotAvailable
                        | ErrorKind::HostUnavailable
                        | ErrorKind::StreamInvalidated
                        | ErrorKind::PermissionDenied
                );
                if fatal {
                    let _ = error_sender.try_send(CaptureEvent::Failed(err.to_string()));
                }
            },
            Some(STREAM_TIMEOUT),
        )
        .map_err(|err| err.to_string())
}

fn find_device(host: &Host, id: &str) -> Result<Device, String> {
    let device_id: DeviceId = id
        .parse()
        .map_err(|err: cpal::Error| format!("Invalid audio device id: {err}"))?;
    host.device_by_id(&device_id).ok_or_else(|| {
        "The selected audio device is not available. Choose another device.".to_string()
    })
}

/// Picks the device that carries what the user hears: the default output's
/// monitor source on PulseAudio/PipeWire, or the default output on Windows.
fn default_game_audio_device(host: &Host) -> Option<Device> {
    if cfg!(target_os = "windows") {
        return host.default_output_device();
    }

    let monitor_id = host
        .default_output_device()
        .and_then(|device| device.id().ok())
        .map(|id| DeviceId::new(id.host(), format!("{}.monitor", id.id())));
    monitor_id
        .and_then(|id| host.device_by_id(&id))
        .or_else(|| {
            host.input_devices()
                .ok()?
                .find(|device| device.id().is_ok_and(|id| id.id().ends_with(".monitor")))
        })
        .or_else(|| host.default_input_device())
}

fn describe(device: &Device, loopback: bool) -> Option<AudioDevice> {
    let id = device.id().ok()?;
    let kind = if loopback {
        AudioDeviceKind::Loopback
    } else if id.id().ends_with(".monitor") {
        AudioDeviceKind::Monitor
    } else {
        AudioDeviceKind::Input
    };
    Some(AudioDevice {
        id: id.to_string(),
        name: display_name(device),
        kind,
    })
}

fn display_name(device: &Device) -> String {
    device
        .description()
        .map(|description| description.name().to_string())
        .unwrap_or_else(|_| device.to_string())
}
