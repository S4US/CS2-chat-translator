use chrono::{SecondsFormat, Utc};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, RecvTimeoutError, Sender},
        LazyLock, Mutex,
    },
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum TeamScope {
    All,
    Ct,
    T,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub team: TeamScope,
    pub player_name: String,
    pub original_text: String,
    pub timestamp: String,
    pub source_offset: u64,
}

#[derive(Default)]
pub struct TailController {
    stop: Mutex<Option<Sender<()>>>,
}

impl TailController {
    pub fn start(&self, app: AppHandle, log_path: String) -> io::Result<()> {
        self.stop();

        let path = PathBuf::from(log_path);
        if !path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Log file does not exist: {}", path.display()),
            ));
        }

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let mut guard = self.stop.lock().expect("tail lock poisoned");
        *guard = Some(stop_tx);
        drop(guard);

        thread::spawn(move || {
            let mut offset = file_len(&path).unwrap_or(0);
            let mut partial = Vec::new();
            let (event_tx, event_rx) = mpsc::channel::<notify::Result<Event>>();
            let mut watcher = notify::recommended_watcher(event_tx).ok();

            if let Some(watcher) = watcher.as_mut() {
                let watch_path = path.parent().unwrap_or_else(|| Path::new("."));
                let _ = watcher.watch(watch_path, RecursiveMode::NonRecursive);
            }

            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }

                match event_rx.recv_timeout(Duration::from_millis(350)) {
                    Ok(Ok(event)) if should_read_event(&event, &path) => {
                        read_new_lines(&app, &path, &mut offset, &mut partial);
                    }
                    Ok(_) => {}
                    Err(RecvTimeoutError::Timeout) => {
                        read_new_lines(&app, &path, &mut offset, &mut partial);
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        Ok(())
    }

    pub fn stop(&self) {
        if let Some(stop) = self.stop.lock().expect("tail lock poisoned").take() {
            let _ = stop.send(());
        }
    }
}

pub fn default_log_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        Some(PathBuf::from(
            r"C:\Program Files (x86)\Steam\steamapps\common\Counter-Strike Global Offensive\game\csgo\console.log",
        ))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::home_dir().map(|home| {
            home.join(".local/share/Steam/steamapps/common/Counter-Strike Global Offensive/game/csgo/console.log")
        })
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

static CHAT_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:(?:\d{2}/\d{2})\s+\d{2}:\d{2}:\d{2}\s+)?\[(ALL|CT|T)\]\s+([^:]+):\s?(.*)$")
        .expect("chat line pattern should compile")
});

pub fn parse_chat_line(line: &str, source_offset: u64) -> Option<ChatMessage> {
    let captures = CHAT_LINE.captures(line.trim_end())?;
    let team = match captures.get(1)?.as_str() {
        "ALL" => TeamScope::All,
        "CT" => TeamScope::Ct,
        "T" => TeamScope::T,
        _ => return None,
    };

    Some(ChatMessage {
        id: Uuid::new_v4().to_string(),
        team,
        player_name: clean_player_name(captures.get(2)?.as_str()),
        original_text: captures.get(3)?.as_str().to_string(),
        timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        source_offset,
    })
}

fn clean_player_name(raw_name: &str) -> String {
    let cleaned = raw_name
        .replace(
            ['\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}'],
            "",
        )
        .trim()
        .to_string();

    let location_marker = cleaned
        .find('@')
        .or_else(|| cleaned.find('﹫'))
        .or_else(|| cleaned.find('＠'));

    match location_marker {
        Some(index) => cleaned[..index].trim().to_string(),
        None => cleaned,
    }
}

fn read_new_lines(app: &AppHandle, path: &Path, offset: &mut u64, partial: &mut Vec<u8>) {
    let result = read_appended_lines(path, offset, partial, |line, line_offset| {
        if let Some(message) = parse_chat_line(&line, line_offset) {
            let _ = app.emit("chat-message", message);
        }
    });

    if let Err(message) = result {
        let _ = app.emit("tail-error", message);
    }
}

/// Reads lines appended since `offset`. A trailing line without a newline is
/// still being written, so its bytes wait in `partial` for the next read.
/// Lines are decoded leniently so one malformed line cannot stall the reader,
/// and `offset` advances per line so an error never replays earlier lines.
fn read_appended_lines(
    path: &Path,
    offset: &mut u64,
    partial: &mut Vec<u8>,
    mut on_line: impl FnMut(String, u64),
) -> Result<(), &'static str> {
    let current_len = file_len(path).map_err(|_| "Log file is unavailable")?;
    if current_len < *offset {
        *offset = 0;
        partial.clear();
    }

    if current_len == *offset {
        return Ok(());
    }

    let mut file = File::open(path).map_err(|_| "Could not open log file")?;
    file.seek(SeekFrom::Start(*offset))
        .map_err(|_| "Could not seek log file")?;

    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let bytes = reader
            .read_until(b'\n', &mut buffer)
            .map_err(|_| "Could not read log file")?;
        if bytes == 0 {
            return Ok(());
        }

        *offset += bytes as u64;
        partial.extend_from_slice(&buffer);
        if !buffer.ends_with(b"\n") {
            return Ok(());
        }

        let line = String::from_utf8_lossy(partial).into_owned();
        let line_offset = *offset - partial.len() as u64;
        partial.clear();
        on_line(line, line_offset);
    }
}

fn file_len(path: &Path) -> io::Result<u64> {
    Ok(path.metadata()?.len())
}

fn should_read_event(event: &Event, path: &Path) -> bool {
    let interesting = matches!(
        event.kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any
    );

    interesting
        && event.paths.iter().any(|event_path| {
            event_path == path
                || event_path
                    .file_name()
                    .zip(path.file_name())
                    .map(|(left, right)| left == right)
                    .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_chat_scopes() {
        let all = parse_chat_line("[ALL] PlayerName: gg wp", 12).unwrap();
        let ct = parse_chat_line("[CT] Some One: drop me an awp", 13).unwrap();
        let t = parse_chat_line("[T] rus-her: rushing b", 14).unwrap();

        assert_eq!(all.team, TeamScope::All);
        assert_eq!(ct.team, TeamScope::Ct);
        assert_eq!(t.team, TeamScope::T);
        assert_eq!(all.player_name, "PlayerName");
        assert_eq!(ct.original_text, "drop me an awp");
    }

    #[test]
    fn parses_timestamped_console_chat() {
        let ct = parse_chat_line("06/02 10:37:14  [CT] saus‎﹫CT Start: Hello", 15).unwrap();
        let all = parse_chat_line("06/02 10:38:05  [ALL] saus‎: Hello", 16).unwrap();

        assert_eq!(ct.team, TeamScope::Ct);
        assert_eq!(ct.player_name, "saus");
        assert_eq!(ct.original_text, "Hello");
        assert_eq!(all.team, TeamScope::All);
        assert_eq!(all.player_name, "saus");
        assert_eq!(all.original_text, "Hello");
    }

    #[test]
    fn strips_common_team_location_markers() {
        let normal_at = parse_chat_line("[CT] saus@CT Start: Hello", 1).unwrap();
        let full_width_at = parse_chat_line("[T] saus＠T Spawn: Hello", 2).unwrap();

        assert_eq!(normal_at.player_name, "saus");
        assert_eq!(full_width_at.player_name, "saus");
    }

    #[test]
    fn preserves_colons_and_empty_messages() {
        let with_colon = parse_chat_line("[ALL] Player: site: b", 1).unwrap();
        let empty = parse_chat_line("[T] Player: ", 2).unwrap();

        assert_eq!(with_colon.original_text, "site: b");
        assert_eq!(empty.original_text, "");
    }

    #[test]
    fn ignores_console_noise() {
        assert!(parse_chat_line("Unknown command: foo", 0).is_none());
        assert!(parse_chat_line("[SPEC] Player: no", 0).is_none());
    }

    struct Tail {
        file: tempfile::NamedTempFile,
        offset: u64,
        partial: Vec<u8>,
    }

    impl Tail {
        fn new() -> Self {
            Self {
                file: tempfile::NamedTempFile::new().unwrap(),
                offset: 0,
                partial: Vec::new(),
            }
        }

        fn append(&mut self, bytes: &[u8]) {
            use std::io::Write;
            self.file.write_all(bytes).unwrap();
            self.file.flush().unwrap();
        }

        fn read(&mut self) -> Vec<(String, u64)> {
            let mut lines = Vec::new();
            let (path, offset, partial) = (self.file.path(), &mut self.offset, &mut self.partial);
            read_appended_lines(path, offset, partial, |line, at| lines.push((line, at))).unwrap();
            lines
        }
    }

    #[test]
    fn waits_for_unfinished_lines() {
        let mut tail = Tail::new();
        tail.append(b"[ALL] A: one\n[ALL] B: tw");
        assert_eq!(tail.read(), vec![("[ALL] A: one\n".to_string(), 0)]);

        tail.append(b"o\n");
        assert_eq!(tail.read(), vec![("[ALL] B: two\n".to_string(), 13)]);
        assert!(tail.read().is_empty());
    }

    #[test]
    fn invalid_utf8_does_not_stall_or_repeat_lines() {
        let mut tail = Tail::new();
        tail.append(b"[ALL] A: one\n[ALL] B: \xff\xfe\n[ALL] C: three\n");
        let lines = tail.read();
        assert_eq!(lines.len(), 3);
        assert!(lines[1].0.starts_with("[ALL] B: "));
        assert_eq!(lines[2].0, "[ALL] C: three\n");
        assert!(tail.read().is_empty());
    }

    #[test]
    fn keeps_characters_split_across_writes() {
        let mut tail = Tail::new();
        let line = "[ALL] Миша: раш\n".as_bytes();
        tail.append(&line[..12]);
        assert!(tail.read().is_empty());

        tail.append(&line[12..]);
        let lines = tail.read();
        assert_eq!(lines[0].0, "[ALL] Миша: раш\n");
        assert_eq!(parse_chat_line(&lines[0].0, 0).unwrap().player_name, "Миша");
    }

    #[test]
    fn restarts_when_log_is_truncated() {
        let mut tail = Tail::new();
        tail.append(b"[ALL] A: one\n[ALL] B: two\n");
        assert_eq!(tail.read().len(), 2);

        tail.file.as_file().set_len(0).unwrap();
        use std::io::Seek;
        tail.file.as_file_mut().rewind().unwrap();
        tail.append(b"[ALL] C: new\n");
        assert_eq!(tail.read(), vec![("[ALL] C: new\n".to_string(), 0)]);
    }
}
