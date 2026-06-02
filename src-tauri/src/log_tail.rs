use chrono::Utc;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, RecvTimeoutError, Sender},
        Mutex,
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
            let mut partial = String::new();
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

pub fn parse_chat_line(line: &str, source_offset: u64) -> Option<ChatMessage> {
    let regex = Regex::new(r"^\[(ALL|CT|T)\]\s+([^:]+):\s?(.*)$").ok()?;
    let captures = regex.captures(line.trim_end())?;
    let team = match captures.get(1)?.as_str() {
        "ALL" => TeamScope::All,
        "CT" => TeamScope::Ct,
        "T" => TeamScope::T,
        _ => return None,
    };

    Some(ChatMessage {
        id: Uuid::new_v4().to_string(),
        team,
        player_name: captures.get(2)?.as_str().trim().to_string(),
        original_text: captures.get(3)?.as_str().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        source_offset,
    })
}

fn read_new_lines(app: &AppHandle, path: &Path, offset: &mut u64, partial: &mut String) {
    let Ok(current_len) = file_len(path) else {
        let _ = app.emit("tail-error", "Log file is unavailable");
        return;
    };

    if current_len < *offset {
        *offset = 0;
        partial.clear();
    }

    if current_len == *offset {
        return;
    }

    let Ok(mut file) = File::open(path) else {
        let _ = app.emit("tail-error", "Could not open log file");
        return;
    };

    if file.seek(SeekFrom::Start(*offset)).is_err() {
        let _ = app.emit("tail-error", "Could not seek log file");
        return;
    }

    let mut reader = BufReader::new(file);
    let mut consumed = *offset;
    loop {
        let mut buffer = String::new();
        let Ok(bytes) = reader.read_line(&mut buffer) else {
            let _ = app.emit("tail-error", "Could not read log file");
            return;
        };

        if bytes == 0 {
            break;
        }

        consumed += bytes as u64;
        if !buffer.ends_with('\n') && consumed == current_len {
            partial.push_str(&buffer);
            break;
        }

        if !partial.is_empty() {
            partial.push_str(&buffer);
            buffer = partial.clone();
            partial.clear();
        }

        if let Some(message) = parse_chat_line(&buffer, consumed - bytes as u64) {
            let _ = app.emit("chat-message", message);
        }
    }

    *offset = consumed;
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
}
