use anyhow::Context;
use serde::Deserialize;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

/// Events that agents can send to the TUI via the hook socket.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "event")]
pub enum HookEvent {
    /// Start timer on a task. If `task_id` is present, use it; otherwise use
    /// the TUI's currently selected task.
    #[serde(rename = "session_start")]
    SessionStart { task_id: Option<i64> },

    /// Stop the currently running timer (pause for user input).
    #[serde(rename = "waiting_user")]
    WaitingUser,

    /// Stop all running timers for today (session ended).
    #[serde(rename = "session_end")]
    SessionEnd,
}

/// Non-blocking Unix domain socket listener for agent hook events.
pub struct HookListener {
    listener: Option<UnixListener>,
    socket_path: PathBuf,
}

impl HookListener {
    /// Create a listener. Removes stale socket files and binds a new one.
    /// Returns a listener that does nothing if the socket path can't be created
    /// (non-fatal — TUI works fine without hooks).
    pub fn new() -> Self {
        let socket_path = Self::socket_path();

        // Ensure parent directory exists.
        if let Some(parent) = socket_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Remove stale socket from previous run.
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }

        let listener = UnixListener::bind(&socket_path)
            .context("failed to bind hook socket")
            .ok()
            .and_then(|l| {
                l.set_nonblocking(true)
                    .context("failed to set hook socket non-blocking")
                    .ok()
                    .map(|_| l)
            });

        Self { listener, socket_path }
    }

    fn socket_path() -> PathBuf {
        directories::ProjectDirs::from("", "", "task-timer-tui")
            .map(|d| d.cache_dir().join("hook.sock"))
            .unwrap_or_else(|| PathBuf::from("/tmp/task-timer-tui-hook.sock"))
    }

    /// Returns true if the socket is listening (agents can connect).
    pub fn is_active(&self) -> bool {
        self.listener.is_some()
    }

    /// Non-blocking poll: accept any pending connection, read a JSON line,
    /// parse it, and return the event. Returns `None` if nothing available.
    pub fn poll(&self) -> Option<HookEvent> {
        let listener = self.listener.as_ref()?;
        let (stream, _addr) = match listener.accept() {
            Ok(s) => s,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => return None,
            Err(_) => return None,
        };
        Self::read_event(stream)
    }

    fn read_event(stream: UnixStream) -> Option<HookEvent> {
        use std::io::Read;
        let mut buf = [0u8; 1024];
        let mut reader = std::io::BufReader::new(stream);
        let n = reader.read(&mut buf).ok()?;
        let line = std::str::from_utf8(&buf[..n]).ok()?;
        serde_json::from_str(line).ok()
    }
}

impl Drop for HookListener {
    fn drop(&mut self) {
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_session_start_with_task_id() {
        let json = r#"{"event":"session_start","task_id":42}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        assert_eq!(ev, HookEvent::SessionStart { task_id: Some(42) });
    }

    #[test]
    fn parse_session_start_without_task_id() {
        let json = r#"{"event":"session_start"}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        assert_eq!(ev, HookEvent::SessionStart { task_id: None });
    }

    #[test]
    fn parse_waiting_user() {
        let json = r#"{"event":"waiting_user"}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        assert_eq!(ev, HookEvent::WaitingUser);
    }

    #[test]
    fn parse_session_end() {
        let json = r#"{"event":"session_end"}"#;
        let ev: HookEvent = serde_json::from_str(json).unwrap();
        assert_eq!(ev, HookEvent::SessionEnd);
    }

    #[test]
    fn socket_roundtrip() {
        let listener = HookListener::new();
        assert!(listener.is_active());

        let path = listener.socket_path.clone();

        // Connect in a thread and send an event.
        let handle = std::thread::spawn(move || {
            use std::os::unix::net::UnixStream;
            use std::io::Write;
            let mut stream = UnixStream::connect(&path).unwrap();
            stream
                .write_all(b"{\"event\":\"session_start\",\"task_id\":7}\n")
                .unwrap();
        });

        // Poll should return the event.
        let mut got = None;
        for _ in 0..50 {
            if let Some(ev) = listener.poll() {
                got = Some(ev);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        handle.join().unwrap();
        assert_eq!(
            got,
            Some(HookEvent::SessionStart { task_id: Some(7) })
        );
    }
}
