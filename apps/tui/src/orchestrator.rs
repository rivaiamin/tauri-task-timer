use anyhow::{anyhow, Result};
use ratatui::text::Line;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const ADAPTER_BUNDLE: &str = include_str!(concat!(env!("OUT_DIR"), "/orchestrator-adapter.mjs"));
const MAX_OUTPUT: usize = 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub fn display_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch == '\t' { ' ' } else { ch })
        .filter(|ch| !ch.is_control() || *ch == '\n')
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Driver {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct HistoryCursor {
    pub epoch: String,
    pub seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub role: String,
    pub text: String,
    pub timestamp: String,
    pub seq_start: u64,
    pub seq_end: u64,
    pub message_id: Option<String>,
    pub is_wake: bool,
}

pub(crate) fn history_role_label(entry: &HistoryEntry) -> Cow<'_, str> {
    match entry.role.as_str() {
        "user_message" if entry.is_wake => Cow::Borrowed("user / automated wake"),
        "user_message" => Cow::Borrowed("user"),
        "assistant_message" => Cow::Borrowed("assistant"),
        "reasoning" => Cow::Borrowed("reasoning"),
        "tool_call" => Cow::Borrowed("tool call"),
        "todo" => Cow::Borrowed("todo"),
        "error" => Cow::Borrowed("error"),
        "notification" => Cow::Borrowed("notification"),
        "compaction" => Cow::Borrowed("compaction"),
        "plugin" => Cow::Borrowed("plugin"),
        other => Cow::Owned(display_text(other)),
    }
}

pub(crate) fn history_entry_header(entry: &HistoryEntry) -> String {
    format!(
        "[{}] {}",
        history_role_label(entry),
        display_text(&entry.timestamp)
    )
}

pub(crate) fn history_entry_rendered_lines(entry: &HistoryEntry, width: usize) -> usize {
    let width = width.max(1);
    let header_lines = Line::from(history_entry_header(entry)).width().max(1).div_ceil(width);
    let text_lines = display_text(&entry.text)
        .lines()
        .map(|line| Line::from(line).width().max(1).div_ceil(width))
        .sum::<usize>();
    header_lines + text_lines + 1
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryPage {
    pub agent_id: String,
    pub epoch: String,
    pub direction: String,
    pub start_cursor: Option<HistoryCursor>,
    pub end_cursor: Option<HistoryCursor>,
    pub has_older: bool,
    pub has_newer: bool,
    pub reset: bool,
    pub stale_cursor: bool,
    pub gap: bool,
    pub min_seq: u64,
    pub max_seq: u64,
    pub entries: Vec<HistoryEntry>,
}

pub fn parse_history_page(json: &[u8], expected_agent_id: &str) -> Result<HistoryPage> {
    let raw: Value = serde_json::from_slice(json)
        .map_err(|error| anyhow!("invalid Paseo history JSON: {error}"))?;
    let object = raw
        .as_object()
        .ok_or_else(|| anyhow!("Paseo history page must be an object"))?;
    let string = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Paseo history page has invalid {key}"))
    };
    let boolean = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_bool)
            .ok_or_else(|| anyhow!("Paseo history page has invalid {key}"))
    };
    let agent_id = string("agentId")?.to_owned();
    let epoch = string("epoch")?.to_owned();
    if agent_id != expected_agent_id || epoch.is_empty() {
        return Err(anyhow!("Paseo history identity changed"));
    }
    let direction = string("direction")?.to_owned();
    if !matches!(direction.as_str(), "tail" | "before" | "after") {
        return Err(anyhow!("Paseo history page has invalid direction"));
    }
    let cursor = |key: &str| -> Result<Option<HistoryCursor>> {
        match object.get(key) {
            Some(Value::Null) => Ok(None),
            Some(value) => {
                let cursor: HistoryCursor = serde_json::from_value(value.clone())
                    .map_err(|_| anyhow!("Paseo history page has invalid {key}"))?;
                if cursor.epoch != epoch {
                    return Err(anyhow!("Paseo history cursor epoch changed"));
                }
                Ok(Some(cursor))
            }
            None => Err(anyhow!("Paseo history page is missing {key}")),
        }
    };
    let window = object
        .get("window")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("Paseo history page has invalid window"))?;
    let min_seq = window
        .get("minSeq")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("Paseo history page has invalid minSeq"))?;
    let max_seq = window
        .get("maxSeq")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("Paseo history page has invalid maxSeq"))?;
    if min_seq > max_seq {
        return Err(anyhow!("Paseo history page has an invalid sequence window"));
    }
    let rows = object
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Paseo history page has invalid entries"))?;
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let row = row
            .as_object()
            .ok_or_else(|| anyhow!("Paseo history entry must be an object"))?;
        let item = row
            .get("item")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow!("Paseo history entry has invalid item"))?;
        let role = item
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Paseo history item has no type"))?
            .to_owned();
        let text = history_item_text(item, &role);
        let timestamp = row
            .get("timestamp")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Paseo history entry has invalid timestamp"))?
            .to_owned();
        let seq_start = row
            .get("seqStart")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("Paseo history entry has invalid seqStart"))?;
        let seq_end = row
            .get("seqEnd")
            .and_then(Value::as_u64)
            .filter(|seq| *seq >= seq_start)
            .ok_or_else(|| anyhow!("Paseo history entry has invalid seqEnd"))?;
        let message_id = item
            .get("messageId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let is_wake = role == "user_message" && text.starts_with("orchestrator wake:");
        entries.push(HistoryEntry {
            role,
            text,
            timestamp,
            seq_start,
            seq_end,
            message_id,
            is_wake,
        });
    }
    if entries.windows(2).any(|pair| pair[0].seq_start > pair[1].seq_start) {
        return Err(anyhow!("Paseo history entries are not in sequence order"));
    }
    let start_cursor = cursor("startCursor")?;
    let end_cursor = cursor("endCursor")?;
    Ok(HistoryPage {
        agent_id,
        epoch,
        direction,
        start_cursor,
        end_cursor,
        has_older: boolean("hasOlder")?,
        has_newer: boolean("hasNewer")?,
        reset: boolean("reset")?,
        stale_cursor: boolean("staleCursor")?,
        gap: boolean("gap")?,
        min_seq,
        max_seq,
        entries,
    })
}

fn history_item_text(item: &serde_json::Map<String, Value>, role: &str) -> String {
    let string = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or("");
    match role {
        "user_message" | "assistant_message" | "reasoning" => string("text").to_owned(),
        "error" | "notification" => string("message").to_owned(),
        "tool_call" => format!("{} [{}]", string("name"), string("status")),
        "todo" => item
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|entry| {
                        let object = entry.as_object()?;
                        let text = object.get("text")?.as_str()?;
                        Some(format!(
                            "{}{}",
                            if object.get("completed").and_then(Value::as_bool) == Some(true) {
                                "[x] "
                            } else {
                                "[ ] "
                            },
                            text
                        ))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        "compaction" => format!("context compaction ({})", string("status")),
        "plugin" => format!("{} ({})", string("kind"), string("pluginId")),
        other => format!("Unsupported timeline item: {other}"),
    }
}

#[derive(Deserialize)]
struct DriverRow {
    id: String,
    #[serde(default)]
    name: String,
    status: String,
}

pub fn parse_drivers(json: &[u8]) -> Result<Vec<Driver>> {
    let rows: Vec<DriverRow> = serde_json::from_slice(json)
        .map_err(|e| anyhow!("invalid Paseo driver list: {e}"))?;
    rows.into_iter()
        .map(|row| {
            if row.id.trim().is_empty() || row.status.trim().is_empty() {
                return Err(anyhow!("Paseo returned a driver without an ID or status"));
            }
            Ok(Driver {
                id: row.id,
                name: row.name,
                status: row.status,
            })
        })
        .collect()
}

#[derive(Debug)]
pub struct CommandOutput {
    pub stdout: Vec<u8>,
}

pub struct CommandResponse {
    pub generation: u64,
    pub result: Result<CommandOutput, String>,
}

struct CommandRequest {
    generation: u64,
    program: String,
    args: Vec<String>,
    stdin: Vec<u8>,
}

enum WorkerMessage {
    Run(CommandRequest),
    Stop,
}

pub struct CommandWorker {
    requests: SyncSender<WorkerMessage>,
    responses: Receiver<CommandResponse>,
    cancelled: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    adapter_file: PathBuf,
}

impl CommandWorker {
    pub fn new() -> Result<Self> {
        let adapter_file = write_adapter_file()?;
        let (request_tx, request_rx) = mpsc::sync_channel::<WorkerMessage>(1);
        let (response_tx, response_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_thread = thread::Builder::new()
            .name("orchestrator-command".into())
            .spawn(move || {
                while let Ok(message) = request_rx.recv() {
                    let WorkerMessage::Run(request) = message else {
                        break;
                    };
                    let result = run_command(&request, &worker_cancelled);
                    if response_tx
                        .send(CommandResponse {
                            generation: request.generation,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            responses: response_rx,
            cancelled,
            thread: Some(worker_thread),
            adapter_file,
        })
    }

    pub fn submit(&self, generation: u64, program: &str, args: &[&str], stdin: &[u8]) -> bool {
        if stdin.len() > MAX_OUTPUT {
            return false;
        }
        let request = CommandRequest {
            generation,
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            stdin: stdin.to_vec(),
        };
        match self.requests.try_send(WorkerMessage::Run(request)) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
        }
    }

    pub fn adapter_path(&self) -> &std::path::Path {
        &self.adapter_file
    }

    pub fn try_recv(&self) -> Option<CommandResponse> {
        match self.responses.try_recv() {
            Ok(response) => Some(response),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }
}

impl Drop for CommandWorker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let _ = self.requests.send(WorkerMessage::Stop);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
        let _ = fs::remove_file(&self.adapter_file);
    }
}

fn write_adapter_file() -> Result<PathBuf> {
    for _ in 0..16 {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "task-timer-orchestrator-{}-{id}.mjs",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(ADAPTER_BUNDLE.as_bytes()) {
                    let _ = fs::remove_file(&path);
                    return Err(error.into());
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(anyhow!("could not create a private Paseo adapter file"))
}

fn run_command(request: &CommandRequest, cancelled: &AtomicBool) -> Result<CommandOutput, String> {
    if cancelled.load(Ordering::SeqCst) {
        return Err("command cancelled".into());
    }
    if request.stdin.len() > MAX_OUTPUT {
        return Err("command input exceeded 1 MiB".into());
    }
    let mut command = Command::new(&request.program);
    command
        .args(&request.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let mut stdin = child.stdin.take().expect("stdin pipe");
    let request_bytes = request.stdin.clone();
    let stdin_writer = thread::spawn(move || stdin.write_all(&request_bytes));
    let output_overflow = Arc::new(AtomicBool::new(false));
    let error_overflow = Arc::new(AtomicBool::new(false));
    let output_rx = spawn_reader(
        child.stdout.take().expect("stdout pipe"),
        Arc::clone(&output_overflow),
    );
    let error_rx = spawn_reader(
        child.stderr.take().expect("stderr pipe"),
        Arc::clone(&error_overflow),
    );
    let started = Instant::now();
    let status = loop {
        if cancelled.load(Ordering::SeqCst) {
            terminate(&mut child);
            return Err("command cancelled".into());
        }
        if started.elapsed() >= COMMAND_TIMEOUT {
            terminate(&mut child);
            return Err("command timed out".into());
        }
        if output_overflow.load(Ordering::Relaxed) || error_overflow.load(Ordering::Relaxed) {
            terminate(&mut child);
            return Err("command output exceeded 1 MiB".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                terminate(&mut child);
                return Err(error.to_string());
            }
        }
    };
    stdin_writer
        .join()
        .map_err(|_| "command stdin writer panicked".to_owned())?
        .map_err(|error| error.to_string())?;
    let stdout = receive_pipe(output_rx)?;
    let stderr = receive_pipe(error_rx)?;
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            format!("{} exited with {status}", request.program)
        } else {
            format!("{}: {detail}", request.program)
        });
    }
    Ok(CommandOutput { stdout })
}

fn spawn_reader<R: Read + Send + 'static>(
    reader: R,
    overflow: Arc<AtomicBool>,
) -> Receiver<Result<Vec<u8>, String>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = reader
            .take((MAX_OUTPUT + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())
            .and_then(|_| {
                if bytes.len() > MAX_OUTPUT {
                    overflow.store(true, Ordering::Relaxed);
                    Err("command output exceeded 1 MiB".into())
                } else {
                    Ok(bytes)
                }
            });
        let _ = tx.send(result);
    });
    rx
}

fn receive_pipe(receiver: Receiver<Result<Vec<u8>, String>>) -> Result<Vec<u8>, String> {
    receiver
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| "command pipe did not close".to_owned())?
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_list_requires_a_valid_array_and_complete_identity() {
        let drivers = parse_drivers(
            br#"[{"id":"agent-1","name":"driver","status":"idle"}]"#,
        )
        .unwrap();
        assert_eq!(drivers, vec![Driver {
            id: "agent-1".into(),
            name: "driver".into(),
            status: "idle".into(),
        }]);
        assert!(parse_drivers(br#"{"status":"idle"}"#).is_err());
        assert!(parse_drivers(br#"[{"id":"","status":"idle"}]"#).is_err());
    }

    #[test]
    fn display_text_removes_terminal_controls_and_preserves_wrapping() {
        assert_eq!(display_text("user\u{1b}[2J\tmessage\nreply"), "user[2J message\nreply");
    }
    #[test]
    fn history_headers_strip_terminal_controls_from_roles_and_timestamps() {
        let entry = HistoryEntry {
            role: "custom\u{1b}[2J".into(),
            text: String::new(),
            timestamp: "now\u{1b}[31m".into(),
            seq_start: 1,
            seq_end: 1,
            message_id: None,
            is_wake: false,
        };
        assert_eq!(history_entry_header(&entry), "[custom[2J] now[31m");
    }

    fn history_page(entries: Value, direction: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "agentId": "agent-1",
            "epoch": "epoch-a",
            "direction": direction,
            "startCursor": {"epoch": "epoch-a", "seq": 10},
            "endCursor": {"epoch": "epoch-a", "seq": 20},
            "hasOlder": true,
            "hasNewer": false,
            "reset": false,
            "staleCursor": false,
            "gap": false,
            "window": {"minSeq": 1, "maxSeq": 20, "nextSeq": 21},
            "entries": entries,
        }))
        .unwrap()
    }

    #[test]
    fn history_parser_preserves_duplicate_text_and_marks_wake_prompts() {
        let bytes = history_page(
            serde_json::json!([
                {
                    "item": {"type":"user_message","text":"same prompt","messageId":"m1"},
                    "timestamp":"2026-10-19T10:00:00Z","seqStart":11,"seqEnd":11,"sourceSeqRanges":[]
                },
                {
                    "item": {"type":"user_message","text":"same prompt","messageId":"m2"},
                    "timestamp":"2026-10-19T10:00:01Z","seqStart":12,"seqEnd":12,"sourceSeqRanges":[]
                },
                {
                    "item": {"type":"user_message","text":"orchestrator wake: continue","messageId":"m3"},
                    "timestamp":"2026-10-19T10:00:02Z","seqStart":13,"seqEnd":13,"sourceSeqRanges":[]
                },
                {
                    "item": {"type":"tool_call","name":"shell","status":"completed"},
                    "timestamp":"2026-10-19T10:00:03Z","seqStart":14,"seqEnd":14,"sourceSeqRanges":[]
                }
            ]),
            "tail",
        );
        let page = parse_history_page(&bytes, "agent-1").unwrap();
        assert_eq!(page.entries.len(), 4);
        assert_eq!(page.entries[0].text, page.entries[1].text);
        assert_ne!(page.entries[0].seq_start, page.entries[1].seq_start);
        assert!(page.entries[2].is_wake);
        assert_eq!(page.entries[3].role, "tool_call");
        assert_eq!(page.entries[3].text, "shell [completed]");
    }

    #[test]
    fn history_parser_rejects_wrong_identity_epoch_and_sequence_order() {
        let base = history_page(serde_json::json!([]), "before");
        assert!(parse_history_page(&base, "another-agent").is_err());

        let mut wrong_epoch: Value = serde_json::from_slice(&base).unwrap();
        wrong_epoch["startCursor"]["epoch"] = Value::String("old-epoch".into());
        assert!(parse_history_page(&wrong_epoch.to_string().into_bytes(), "agent-1").is_err());

        let mut reversed: Value = serde_json::from_slice(&base).unwrap();
        reversed["entries"] = serde_json::json!([
            {
                "item":{"type":"assistant_message","text":"later"},
                "timestamp":"2026-10-19T10:00:01Z","seqStart":12,"seqEnd":12,"sourceSeqRanges":[]
            },
            {
                "item":{"type":"user_message","text":"earlier"},
                "timestamp":"2026-10-19T10:00:00Z","seqStart":11,"seqEnd":11,"sourceSeqRanges":[]
            }
        ]);
        assert!(parse_history_page(&reversed.to_string().into_bytes(), "agent-1").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn dropping_worker_cancels_a_running_child_promptly() {
        let marker = std::env::temp_dir().join(format!(
            "task-timer-orchestrator-worker-{}-{}.started",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let escaped = marker.to_string_lossy().replace('\'', "'\\''");
        let script = format!("printf started > '{escaped}'; exec sleep 30");
        let worker = CommandWorker::new().unwrap();
        assert!(worker.submit(7, "sh", &["-c", &script], &[]));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !marker.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists(), "test child did not start");
        let started = Instant::now();
        drop(worker);
        let _ = fs::remove_file(marker);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "dropping the worker waited for the child timeout"
        );
    }

    /// Every other test fakes `node`, so nothing else proves the real bundle
    /// even loads. A bundled CommonJS dependency that calls `require()` crashed
    /// at import time ("Dynamic require of \"crypto\" is not supported").
    #[test]
    fn bundled_adapter_loads_under_real_node() {
        use std::io::Write;
        use std::process::Stdio;

        let adapter = write_adapter_file().unwrap();
        let mut child = Command::new("node")
            .arg(&adapter)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("node is required to run the adapter");
        child.stdin.take().unwrap().write_all(b"not json").unwrap();
        let output = child.wait_with_output().unwrap();
        let _ = fs::remove_file(&adapter);

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("invalid adapter request JSON"),
            "adapter did not reach its own input validation: {stderr}"
        );
    }
}

