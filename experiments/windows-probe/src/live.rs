//! Shared bounded session control, independent of GTK and Windows handles.
use std::{
    num::NonZeroU16,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
};

#[cfg_attr(not(windows), allow(dead_code))]
pub enum Command {
    Input(Vec<u8>),
    Resize(u16, u16),
}

pub struct State {
    pub parser: vt100::Parser,
    pub revision: u64,
    pub status: String,
    pub running: bool,
}

impl State {
    pub fn new() -> Self {
        Self {
            parser: vt100::Parser::new(
                NonZeroU16::new(24).unwrap(),
                NonZeroU16::new(80).unwrap(),
                0,
            ),
            revision: 1,
            status: "Starting local terminal…".into(),
            running: true,
        }
    }

    pub fn finish(&mut self, status: String) {
        self.running = false;
        self.status = status;
        self.revision = self.revision.wrapping_add(1);
    }
}

pub struct Snapshot {
    pub screen: vt100::Screen,
    pub revision: u64,
    pub status: String,
    pub running: bool,
}

pub struct Session {
    tx: SyncSender<Command>,
    pub state: Arc<Mutex<State>>,
    pub stop: Arc<AtomicBool>,
}

impl Session {
    pub fn start() -> Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel(32);
        let state = Arc::new(Mutex::new(State::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_state = state.clone();
        let worker_stop = stop.clone();
        std::thread::Builder::new()
            .name("windows-terminal".into())
            .spawn(move || {
                #[cfg(windows)]
                let result = crate::conpty::run_live(rx, worker_state.clone(), worker_stop);
                #[cfg(not(windows))]
                let result: Result<String, String> = {
                    drop((rx, worker_stop));
                    Err("Interactive ConPTY requires Windows".into())
                };
                if let Ok(mut state) = worker_state.lock() {
                    state.finish(result.unwrap_or_else(|error| error));
                }
            })
            .map_err(|_| "Unable to start terminal worker")?;
        Ok(Self { tx, state, stop })
    }

    pub fn input(&self, bytes: Vec<u8>) -> Result<(), String> {
        if bytes.is_empty() || bytes.len() > 4096 || self.stop.load(Ordering::Acquire) {
            return Err("Input rejected (empty, over 4096 bytes or stopping)".into());
        }
        self.tx
            .try_send(Command::Input(bytes))
            .map_err(|_| "Terminal input busy or closed".into())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        if !(20..=160).contains(&cols) || !(5..=60).contains(&rows) {
            return Err("Terminal dimensions outside 20–160 × 5–60".into());
        }
        self.tx
            .try_send(Command::Resize(cols, rows))
            .map_err(|_| "Terminal resize busy or closed".into())
    }

    pub fn snapshot(&self, revision: u64) -> Option<Snapshot> {
        let state = self.state.try_lock().ok()?;
        (state.revision != revision).then(|| Snapshot {
            screen: state.parser.screen().clone(),
            revision: state.revision,
            status: state.status.clone(),
            running: state.running,
        })
    }

    pub fn close(&self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn self_test() -> Result<String, String> {
    #[cfg(not(windows))]
    {
        Err("Live ConPTY validation requires Windows".into())
    }
    #[cfg(windows)]
    {
        use std::time::{Duration, Instant};
        let session = Session::start()?;
        session.resize(100, 30)?;
        session.input(
            "set RR_A=RUSTREMOTE_CONPTY_\r\nset RR_B=OK_été\r\necho %RR_A%%RR_B%\r\n"
                .as_bytes()
                .to_vec(),
        )?;
        let deadline = Instant::now() + Duration::from_secs(12);
        let mut marker = false;
        while Instant::now() < deadline {
            if let Some(snapshot) = session.snapshot(0) {
                if snapshot
                    .screen
                    .contents()
                    .contains(crate::probe_output::EXPECTED)
                {
                    marker = true;
                    break;
                }
                if !snapshot.running {
                    return Err(snapshot.status);
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !marker {
            return Err("Live ANSI marker timeout".into());
        }
        session.input(b"exit /b 0\r\n".to_vec())?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut exited = false;
        while Instant::now() < deadline {
            if let Some(snapshot) = session.snapshot(0) {
                if !snapshot.running {
                    if snapshot.status != "Local terminal exited (0)" {
                        return Err(snapshot.status);
                    }
                    exited = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !exited {
            return Err("Live terminal exit timeout".into());
        }
        // A second live child exercises cancellation independently of shell exit.
        let cancelled = Session::start()?;
        cancelled.close();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(snapshot) = cancelled.snapshot(0) {
                if !snapshot.running {
                    if snapshot.status != "Local terminal stopped" {
                        return Err(snapshot.status);
                    }
                    return Ok("LIVE_CONPTY_OK: streaming ANSI/UTF-8, input, resize, exit0, cancellation, private job lifecycle".into());
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err("Live terminal cancellation timeout".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queues_are_bounded_and_stop_does_not_depend_on_a_free_queue_slot() {
        let (tx, _rx) = mpsc::sync_channel(1);
        let session = Session {
            tx,
            state: Arc::new(Mutex::new(State::new())),
            stop: Arc::new(AtomicBool::new(false)),
        };
        assert!(session.input(vec![0; 4097]).is_err());
        assert!(session.resize(161, 24).is_err());
        session.input(b"a".to_vec()).unwrap();
        assert!(session.input(b"b".to_vec()).is_err());
        session.close();
        assert!(session.stop.load(Ordering::Acquire));
        assert!(session.input(b"c".to_vec()).is_err());
    }

    #[test]
    fn snapshots_skip_unchanged_frames_and_do_not_block_on_reader() {
        let (tx, _rx) = mpsc::sync_channel(1);
        let session = Session {
            tx,
            state: Arc::new(Mutex::new(State::new())),
            stop: Arc::new(AtomicBool::new(false)),
        };
        let initial = session.snapshot(0).unwrap();
        assert!(initial.running);
        assert!(session.snapshot(initial.revision).is_none());
        let mut state = session.state.lock().unwrap();
        assert!(session.snapshot(0).is_none());
        state.finish("Closed".into());
        drop(state);
        assert!(!session.snapshot(initial.revision).unwrap().running);
    }
}
