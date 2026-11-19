use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::error::CoreError;
use crate::persist::atomic_write;

/// Which document a queued save belongs to, so the window can clear the right dirty flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveKind {
    Layout,
    Scene,
    Settings,
    Panel,
    Marker,
}

enum Msg {
    Write { kind: SaveKind, path: PathBuf, bytes: Vec<u8> },
    Flush(mpsc::Sender<()>),
    Stop,
}

/// Disk writes on one background thread. The queue is bounded so a stuck disk cannot grow without limit.
pub struct BackgroundWriter {
    tx: SyncSender<Msg>,
    done: Receiver<(SaveKind, Result<(), CoreError>)>,
    join: Option<JoinHandle<()>>,
}

impl BackgroundWriter {
    pub fn new(bound: usize) -> Self {
        Self::spawn(bound.max(1), None)
    }

    fn spawn(bound: usize, gate: Option<Receiver<()>>) -> Self {
        let (tx, rx) = mpsc::sync_channel(bound);
        let (done_tx, done) = mpsc::channel();
        let join = thread::spawn(move || worker(rx, done_tx, gate));
        Self { tx, done, join: Some(join) }
    }

    /// Queue one file. Fails when the queue is already full.
    pub fn submit(&self, kind: SaveKind, path: PathBuf, bytes: Vec<u8>) -> Result<(), CoreError> {
        match self.tx.try_send(Msg::Write { kind, path, bytes }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(CoreError::new("save queue is full")),
            Err(TrySendError::Disconnected(_)) => Err(CoreError::new("save writer has stopped")),
        }
    }

    /// Finished writes since the last poll. The window uses these to show errors and clear dirty flags.
    pub fn poll(&self) -> Vec<(SaveKind, Result<(), CoreError>)> {
        let mut out = Vec::new();
        while let Ok(item) = self.done.try_recv() {
            out.push(item);
        }
        out
    }

    /// Wait until every queued write has finished. Returns the first error.
    pub fn flush(&self) -> Result<(), CoreError> {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.tx.send(Msg::Flush(ack_tx)).map_err(|_| CoreError::new("save writer has stopped"))?;
        match ack_rx.recv_timeout(Duration::from_secs(30)) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => {
                return Err(CoreError::new("save writer did not finish"));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(CoreError::new("save writer has stopped"));
            }
        }
        let mut first = Ok(());
        for (_, result) in self.poll() {
            if first.is_ok() {
                first = result;
            }
        }
        first
    }
}

impl Drop for BackgroundWriter {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn worker(
    rx: Receiver<Msg>,
    done: mpsc::Sender<(SaveKind, Result<(), CoreError>)>,
    gate: Option<Receiver<()>>,
) {
    loop {
        if let Some(gate) = &gate {
            if gate.recv().is_err() {
                break;
            }
        }
        match rx.recv() {
            Ok(Msg::Write { kind, path, bytes }) => {
                let result = atomic_write(&path, &bytes);
                if done.send((kind, result)).is_err() {
                    break;
                }
            }
            Ok(Msg::Flush(ack)) => {
                if ack.send(()).is_err() {
                    break;
                }
            }
            Ok(Msg::Stop) | Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_queued_save_lands_after_flush_and_a_full_queue_is_refused() {
        let dir = std::env::temp_dir().join(format!("seal-writer-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scene.seal");

        let (gate_tx, gate_rx) = mpsc::channel();
        let writer = BackgroundWriter::spawn(1, Some(gate_rx));
        writer.submit(SaveKind::Scene, path.clone(), b"one".to_vec()).unwrap();
        let refused = writer.submit(SaveKind::Scene, path.clone(), b"two".to_vec());
        assert_eq!(refused.unwrap_err().to_string(), "save queue is full");
        gate_tx.send(()).unwrap();
        gate_tx.send(()).unwrap();
        writer.flush().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"one");
        drop(gate_tx);
        drop(writer);

        let writer = BackgroundWriter::new(4);
        let bad = dir.join("not-a-directory").join("scene.seal");
        fs::write(dir.join("not-a-directory"), b"file").unwrap();
        writer.submit(SaveKind::Scene, bad, b"nope".to_vec()).unwrap();
        let err = writer.flush().unwrap_err();
        assert!(!err.to_string().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
