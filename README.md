# CS2 Chat Translator

A cross-platform desktop companion app for translating Counter-Strike 2 chat in
real time. The app listens to CS2's `console.log`, detects newly appended chat
messages, translates them, and shows the original and translated text in a
compact native desktop window.

## Features

- Tails CS2 `console.log` from the current end of the file.
- Parses `[ALL]`, `[CT]`, and `[T]` chat messages.
- Ignores old log history and non-chat console output.
- Supports Windows and Linux.
- Prompts for a custom `console.log` path when the default path is missing.
- Supports DeepL and OpenAI-compatible LLM providers.
- Stores API keys through the operating system keyring.
- Provides a modern companion UI with provider settings, target language, status,
  retry, and clear controls.

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
  libxdo-devel

sudo dnf group install "c-development"
```

If Cargo reports that `dbus-1.pc` is missing, install:

```bash
sudo dnf install dbus-devel pkgconf-pkg-config
```

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

## Translation Providers

V1 supports:

- DeepL
- OpenAI-compatible LLM APIs, including DeepSeek-style endpoints

API keys are stored through the operating system keyring. On Linux, a desktop
secret service such as GNOME Keyring or KWallet may be required. If a keyring is
not available, use provider environment variables while developing:

- `DEEPL_API_KEY`
- `OPENAI_COMPAT_API_KEY`

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
