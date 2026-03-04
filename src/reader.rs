use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::{fs, thread};

use arc_swap::ArcSwap;
use tracing::{debug, error};

use crate::config::parse_command;
use crate::errors::Result;
use crate::events::{Event, EventSender};
use crate::snapshot::StateSnapshot;

const QUERY_PREFIX: &[u8] = b"query\0";

/// `CommandReader` is responsible for sending and receiving commands via a Unix socket.
/// It acts as an IPC mechanism for the `karakuri` application, allowing external processes
/// or the CLI client to communicate with the running daemon.
pub struct CommandReader {
    events: EventSender,
    shared_state: Arc<ArcSwap<StateSnapshot>>,
}

impl CommandReader {
    /// The path to the Unix socket used for inter-process communication.
    const SOCKET_PATH: &str = "/tmp/karakuri.socket";

    /// Sends a command and its arguments to the running `karakuri` application via a Unix socket.
    /// The arguments are serialized and sent as a byte stream.
    pub fn send_command(params: impl IntoIterator<Item = String>) -> Result<()> {
        let output = params
            .into_iter()
            .flat_map(|param| [param.as_bytes(), &[0]].concat())
            .collect::<Vec<_>>();
        let size: u32 = output.len().try_into()?;
        debug!("{:?} {output:?}", size.to_le_bytes());

        let mut stream = UnixStream::connect(CommandReader::SOCKET_PATH)?;
        stream.write_all(&size.to_le_bytes())?;
        stream.write_all(&output)?;
        Ok(())
    }

    /// Sends a query to the running daemon and returns the JSON response.
    pub fn send_query(query: &str) -> Result<String> {
        let msg = format!("query\0{query}");
        let msg_bytes = msg.as_bytes();
        let size: u32 = msg_bytes.len().try_into()?;

        let mut stream = UnixStream::connect(CommandReader::SOCKET_PATH)?;
        stream.write_all(&size.to_le_bytes())?;
        stream.write_all(msg_bytes)?;

        // Read response: 4-byte LE u32 length + JSON bytes
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf)?;
        let resp_len = u32::from_le_bytes(len_buf) as usize;
        let mut resp_buf = vec![0u8; resp_len];
        stream.read_exact(&mut resp_buf)?;
        Ok(String::from_utf8_lossy(&resp_buf).to_string())
    }

    /// Creates a new `CommandReader` instance.
    pub fn new(events: EventSender, shared_state: Arc<ArcSwap<StateSnapshot>>) -> Self {
        CommandReader {
            events,
            shared_state,
        }
    }

    /// Starts the `CommandReader` in a new thread, listening for incoming commands on a Unix socket.
    /// Any errors encountered in the runner thread are logged.
    pub fn start(mut self) {
        thread::spawn(move || {
            if let Err(err) = self.runner() {
                error!("{err}");
            }
        });
    }

    /// The main runner function for the `CommandReader` thread.
    fn runner(&mut self) -> Result<()> {
        _ = fs::remove_file(CommandReader::SOCKET_PATH);
        let listener = UnixListener::bind(CommandReader::SOCKET_PATH)?;

        for stream in listener.incoming() {
            let Ok(mut stream) = stream.inspect_err(|err| error!("reading stream {err}")) else {
                continue;
            };
            let mut buffer = [0u8; 4];

            if !full_read(&mut stream, buffer.len(), &mut buffer) {
                continue;
            }
            let size = u32::from_le_bytes(buffer) as usize;
            let mut buffer = vec![0u8; size];

            if !full_read(&mut stream, buffer.len(), &mut buffer) {
                continue;
            }

            // Query messages start with "query\0" — respond with JSON.
            if buffer.starts_with(QUERY_PREFIX) {
                self.handle_query(&mut stream, &buffer[QUERY_PREFIX.len()..]);
                continue;
            }

            // Existing fire-and-forget command dispatch.
            let argv = buffer
                .split(|c| *c == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).to_string())
                .collect::<Vec<_>>();
            let argv_ref = argv.iter().map(String::as_str).collect::<Vec<_>>();

            if let Ok(command) =
                parse_command(&argv_ref).inspect_err(|err| error!("parsing command: {err}"))
            {
                _ = self
                    .events
                    .send(Event::Command { command })
                    .inspect_err(|err| {
                        error!("sending command: {err}");
                    });
            }
        }
        Ok(())
    }

    fn handle_query(&self, stream: &mut UnixStream, query_bytes: &[u8]) {
        let query = String::from_utf8_lossy(query_bytes);
        // Trim trailing null bytes from the query
        let query = query.trim_end_matches('\0');
        let snapshot = self.shared_state.load();

        let response = match query {
            "state" => serde_json::to_string(&**snapshot),
            "focused" => serde_json::to_string(&snapshot.focused_window),
            "displays" => serde_json::to_string(&snapshot.displays),
            "config" => serde_json::to_string(&snapshot.config_flags),
            _ => Ok(r#"{"error":"unknown query"}"#.to_string()),
        };

        let json = response.unwrap_or_else(|e| format!(r#"{{"error":"{e}"}}"#));
        let len = (json.len() as u32).to_le_bytes();
        _ = stream.write_all(&len);
        _ = stream.write_all(json.as_bytes());
    }
}

fn full_read(stream: &mut UnixStream, expected: usize, buffer: &mut [u8]) -> bool {
    if let Ok(count) = stream.read(buffer).inspect_err(|err| {
        error!("{err}");
    }) && count == expected
    {
        true
    } else {
        error!("short read, expected {expected}.");
        false
    }
}
