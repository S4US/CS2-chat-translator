# CS2 Chat Translator

A cross-platform desktop companion app for translating Counter-Strike 2 chat in
real time. The app listens to CS2's `console.log`, detects newly appended chat
messages, translates them, and shows the original and translated text in a
compact native desktop window. It can also listen to voice chat, transcribe what
players say, and translate it into the same timeline.

## Requirements

- CS2 launch option `-condebug` must be enabled.
- Windows or Linux.
- Node.js and npm for frontend development.
- Rust/Cargo and the Tauri platform prerequisites for native development.

### Fedora Tauri Prerequisites

```bash
sudo dnf check-update

sudo dnf install webkit2gtk4.1-devel \
  openssl-devel \
  curl \
  wget \
  file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  libxdo-devel \
  dbus-devel \
  alsa-lib-devel \
  gcc-c++

sudo dnf group install "c-development"
```

If Cargo reports that `dbus-1.pc` is missing, install:

```bash
sudo dnf install dbus-devel pkgconf-pkg-config
```

Voice capture uses `cpal`, which needs `alsa-lib-devel` (`alsa.pc`). Voice
detection statically links ONNX Runtime, which needs `gcc-c++`
(`cannot find -lstdc++`). The first build downloads ONNX Runtime, so it needs
network access.

## Default Log Paths

- Windows: `C:\Program Files (x86)\Steam\steamapps\common\Counter-Strike Global Offensive\game\csgo\console.log`
- Linux: `~/.local/share/Steam/steamapps/common/Counter-Strike Global Offensive/game/csgo/console.log`

If neither default path exists, the app asks you to choose `console.log`.

## How It Works

1. Start CS2 with `-condebug`.
2. Launch the translator.
3. The app finds the default `console.log` or asks you to select it.
4. Listening starts from the current end of the file, so old messages are not
   translated.
5. New chat lines are parsed, queued, translated, and displayed in the timeline.

## Voice Chat

Voice translation runs in four steps:

1. Capture game audio at the device's native rate, mix it to mono, and
   resample it to 16 kHz.
2. Detect speech with [Silero VAD](https://github.com/snakers4/silero-vad)
   and automatic gain control, so silence, music and game noise are not sent
   to the API.
3. Transcribe each speech segment with a Whisper model through an
   OpenAI-compatible `/audio/transcriptions` endpoint. DeepInfra
   (`openai/whisper-large-v3-turbo`) is the default.
4. Translate the transcript with the selected translation provider into the
   target language. LLM providers use a voice-chat prompt tuned for CS2 callouts.

Setup:

1. Open settings and find the **Voice chat** section.
2. Paste a speech-to-text API key (for DeepInfra, create one at
   <https://deepinfra.com>). You can also set `SPEECH_TO_TEXT_API_KEY`.
3. Choose the audio source:
   - **CS2 only** (default) records only the game's audio, so Discord, music
     and other apps are not transcribed. It includes teammates' and enemies'
     voice chat, because CS2 plays it. Start voice before or after the game:
     the status bar shows **Waiting for CS2** until the game is running, and
     the app reconnects when the game restarts.
     - **Linux:** uses a PulseAudio or PipeWire monitor stream on the stream
       named `cs2`. Check the name with `pactl list sink-inputs` while the game
       runs.
     - **Windows:** uses WASAPI process loopback on `cs2.exe`. This needs
       Windows 11 or a recent Windows 10 version.
   - **All system audio** records your default output, so the app hears
     everything you hear.
   - You can also pick a specific device.

   If the level meter stays empty while CS2 is in the background, enable
   playing audio in the background in CS2's audio settings.
4. Set the spoken language, or leave it on automatic detection. Setting it
   improves accuracy and adds a CS2 callout prompt for Russian and English.
5. Click **Voice**. The status bar shows a level meter. It turns purple while
   speech is detected.

If the meter stays empty, the selected source is not receiving audio. If the
meter moves but no speech is detected, lower the speech threshold.

Click the speaker icon on a voice message to replay the audio that was sent for
transcription. The status bar counts segments that the hallucination filters
dropped; hover over the count to see the last reason.

## Overlay

Click **Overlay** to show the latest translations in a small always-on-top
window above the game.

1. Drag the overlay by its header and resize it from the bottom-right corner.
2. Click **Lock** on the overlay (or use the checkbox in settings). A locked
   overlay hides its header and lets mouse clicks pass through to the game.
3. To move it again, clear the lock checkbox in settings.

The overlay shows up to six messages from the last 45 seconds. Its position and
size are restored on the next launch. CS2 must run in borderless windowed mode;
exclusive fullscreen draws over every other window. On Wayland, the compositor
decides whether always-on-top is honored (KDE does).

## Translation Providers

V1 supports:

- DeepL
- OpenAI-compatible LLM APIs, including DeepSeek-style endpoints

API keys are stored in the operating system keyring. On Linux, this is the
Secret Service (GNOME Keyring or KWallet), so keys survive reboots. If no Secret
Service is running, saving a key fails. In that case, set the key in an
environment variable instead:

- `DEEPL_API_KEY`
- `OPENAI_COMPAT_API_KEY`
- `SPEECH_TO_TEXT_API_KEY`

### CS2 slang

Both providers are told that the text is CS2 chat, so slang gets its CS2
meaning: for example, "флешка" becomes a flashbang, not a USB drive. LLM
providers get a built-in list of common CS2 terms and Russian and Ukrainian
slang. DeepL gets a short CS2 description as context.

When a word is still translated wrongly, add it under **CS2 terms** in
settings, one per line:

```text
муха = Scout (SSG 08)
лоу = low HP
```

Your terms override the built-in ones. LLM providers get them in the prompt and
also match misspelled or misheard forms. DeepL gets them as context, which
helps less reliably.

### Hiding messages in your language

Turn on **Hide messages that need no translation** to show only messages that
needed a translation. The app hides a message when:

- Whisper detects that voice chat is already in the target language.
- DeepL detects the target language as the source.
- An LLM reports that the text is already in the target language, or that it
  has nothing to translate (`gg`, `?`, names).

With hiding on, chat messages appear once their translation is done, so they
do not flash in and out.

## Development Status

This is an early v1 implementation. The frontend build passes, and Rust parser
tests are included. Full native Tauri checks require the Linux desktop
development packages listed above.

## Development

```bash
npm install
npm run tauri:dev
```

## Build

```bash
npm run tauri:build
```

## Verification

```bash
npm run build
cd src-tauri
cargo fmt --check
cargo test
```

## Third-Party Models

`src-tauri/models/silero_vad.onnx` is Silero VAD v6.2.3 by the Silero Team,
distributed under the MIT License.
