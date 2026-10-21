//! PulseAudio and PipeWire can record a single playback stream ("monitor
//! stream"), which is how `parec --monitor-stream` works.

use super::{is_game_name, CaptureEvent};
use pulseaudio::protocol::{
    self, stream::BufferAttr, ChannelMap, Command, CommandReply, Prop, Props, SampleFormat,
    SampleSpec,
};
use pulseaudio::{Client, RecordStream};
use std::{
    io::BufReader,
    os::unix::net::UnixStream,
    sync::mpsc::SyncSender,
    time::Duration,
};

/// The server converts the game's audio to the rate the VAD uses.
pub const SAMPLE_RATE: u32 = 16_000;
const BYTES_PER_SAMPLE: usize = 4;
/// Deliver audio every 50 ms. The server default is about two seconds.
const FRAGMENT_BYTES: u32 = SAMPLE_RATE / 20 * BYTES_PER_SAMPLE as u32;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(3);
const CLIENT_NAME: &std::ffi::CStr = c"CS2 Chat Translator";

pub type Stream = RecordStream;

pub struct Backend {
    control: Control,
    client: Client,
}

impl Backend {
    pub fn connect() -> Result<Self, String> {
        let control = Control::connect().map_err(server_error)?;
        let client = Client::from_env(CLIENT_NAME).map_err(server_error)?;
        Ok(Self { control, client })
    }

    /// Returns the index of the game's playback stream. A playing stream wins
    /// over a paused one, and a newer one over an older one.
    pub fn find_game(&mut self) -> Result<Option<u32>, String> {
        let inputs: protocol::SinkInputInfoList = self
            .control
            .roundtrip(Command::GetSinkInputInfoList)
            .map_err(server_error)?;

        Ok(inputs
            .iter()
            .filter(|input| is_game_stream(&input.props))
            .max_by_key(|input| (!input.corked, input.index))
            .map(|input| input.index))
    }

    pub fn attach(&self, index: u32, sender: SyncSender<CaptureEvent>) -> Result<Stream, String> {
        let mut props = Props::new();
        props.set(Prop::MediaName, c"CS2 voice translation");
        let params = protocol::RecordStreamParams {
            sample_spec: SampleSpec {
                format: SampleFormat::Float32Le,
                channels: 1,
                sample_rate: SAMPLE_RATE,
            },
            channel_map: ChannelMap::mono(),
            direct_on_input_index: Some(index),
            buffer_attr: BufferAttr {
                fragment_size: FRAGMENT_BYTES,
                ..Default::default()
            },
            props,
            ..Default::default()
        };

        let mut partial = Vec::new();
        let on_audio = move |data: &[u8]| {
            partial.extend_from_slice(data);
            let complete = partial.len() - partial.len() % BYTES_PER_SAMPLE;
            let samples = partial[..complete]
                .chunks_exact(BYTES_PER_SAMPLE)
                .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                .collect();
            partial.drain(..complete);
            // Dropping audio is better than blocking the client when the pipeline lags.
            let _ = sender.try_send(CaptureEvent::Samples(samples));
        };

        tauri::async_runtime::block_on(self.client.create_record_stream(params, on_audio))
            .map_err(|err| err.to_string())
    }
}

fn is_game_stream(props: &Props) -> bool {
    [Prop::ApplicationName, Prop::ApplicationProcessBinary]
        .into_iter()
        .filter_map(|prop| props.get(prop))
        .any(|value| is_game_name(&prop_text(value)))
}

fn prop_text(value: &[u8]) -> String {
    let value = value.strip_suffix(&[0]).unwrap_or(value);
    String::from_utf8_lossy(value).into_owned()
}

fn server_error(err: impl std::fmt::Display) -> String {
    format!("Could not talk to the audio server (PulseAudio or PipeWire): {err}")
}

/// A blocking connection for queries. The async client has no call for
/// listing playback streams.
struct Control {
    socket: BufReader<UnixStream>,
    version: u16,
    seq: u32,
}

impl Control {
    fn connect() -> Result<Self, String> {
        let path = pulseaudio::socket_path_from_env()
            .ok_or_else(|| "no server socket found".to_string())?;
        let cookie = pulseaudio::cookie_path_from_env()
            .and_then(|path| std::fs::read(path).ok())
            .unwrap_or_default();
        let socket = UnixStream::connect(path).map_err(|err| err.to_string())?;
        socket
            .set_read_timeout(Some(CONTROL_TIMEOUT))
            .map_err(|err| err.to_string())?;

        let mut control = Self {
            socket: BufReader::new(socket),
            version: protocol::MAX_VERSION,
            seq: 0,
        };
        let auth: protocol::AuthReply = control
            .roundtrip(Command::Auth(protocol::AuthParams {
                version: protocol::MAX_VERSION,
                supports_shm: false,
                supports_memfd: false,
                cookie,
            }))
            .map_err(|err| err.to_string())?;
        control.version = auth.version.min(protocol::MAX_VERSION);

        let mut props = Props::new();
        props.set(Prop::ApplicationName, CLIENT_NAME);
        let _: protocol::SetClientNameReply = control
            .roundtrip(Command::SetClientName(props))
            .map_err(|err| err.to_string())?;
        Ok(control)
    }

    fn roundtrip<R: CommandReply>(&mut self, command: Command) -> Result<R, protocol::ProtocolError> {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        protocol::write_command_message(self.socket.get_mut(), seq, &command, self.version)?;
        let (reply_seq, reply) = protocol::read_reply_message(&mut self.socket, self.version)?;
        if reply_seq != seq {
            return Err(protocol::ProtocolError::Invalid(format!(
                "reply {reply_seq} does not match request {seq}"
            )));
        }
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_game_by_application_name() {
        let mut props = Props::new();
        props.set(Prop::ApplicationName, c"cs2");
        assert!(is_game_stream(&props));

        let mut props = Props::new();
        props.set(Prop::ApplicationName, c"Firefox");
        props.set(Prop::ApplicationProcessBinary, c"firefox");
        assert!(!is_game_stream(&props));
    }

    #[test]
    fn finds_game_by_process_binary() {
        let mut props = Props::new();
        props.set(Prop::ApplicationName, c"Counter-Strike 2");
        props.set(Prop::ApplicationProcessBinary, c"cs2");
        assert!(is_game_stream(&props));
    }
}
