//! An append-only event log shared by runs and authoring sessions: every event goes to the
//! in-memory log (for late subscribers and reconnects), an optional JSON-lines record on disk,
//! and a broadcast channel (for live subscribers), in that order. `stream` turns a journal into
//! a server-sent event response that first replays the log after `Last-Event-ID`, then follows
//! live events, and ends after the last event.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::watch;

/// Live subscribers that fall this far behind re-read the log instead.
const CHANNEL: usize = 256;

/// An event as stored and streamed.
pub trait Entry: Clone + Serialize + Send + Sync + 'static {
    /// Position in the log, from 0; also the SSE event id.
    fn seq(&self) -> u64;
    /// The SSE event name.
    fn kind(&self) -> &'static str;
    /// True for the final event of the log; the stream ends after it.
    fn is_last(&self) -> bool;
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned lock only means another thread panicked mid-update; the data is still usable.
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct Journal<T> {
    /// For log messages only.
    name: String,
    log: Mutex<Vec<T>>,
    tx: broadcast::Sender<T>,
    record: Mutex<Option<std::io::BufWriter<std::fs::File>>>,
}

impl<T: Entry> Journal<T> {
    /// A journal that also writes `<dir>/<name>.jsonl` when `dir` is given (best effort: a
    /// record that cannot be opened is logged and skipped).
    pub fn new(name: &str, dir: Option<&Path>) -> Self {
        let record = dir.and_then(|dir| {
            std::fs::create_dir_all(dir)
                .and_then(|()| std::fs::File::create(dir.join(format!("{name}.jsonl"))))
                .map(std::io::BufWriter::new)
                .map_err(|e| tracing::warn!(%name, error = %e, "no record on disk"))
                .ok()
        });
        Self {
            name: name.to_owned(),
            log: Mutex::new(Vec::new()),
            tx: broadcast::channel(CHANNEL).0,
            record: Mutex::new(record),
        }
    }

    /// Appends the event `make` builds for the next sequence number.
    pub fn append(&self, make: impl FnOnce(u64) -> T) {
        let mut log = lock(&self.log);
        let entry = make(log.len() as u64);
        // Events come about once per simulated step or model call: a buffered write and flush
        // (no fsync) under the lock is cheap at that rate and keeps the record in log order.
        if let Some(writer) = lock(&self.record).as_mut() {
            match serde_json::to_string(&entry) {
                Ok(line) => {
                    if let Err(e) = writeln!(writer, "{line}").and_then(|()| writer.flush()) {
                        tracing::warn!(name = %self.name, error = %e, "record write failed");
                    }
                }
                Err(e) => tracing::warn!(name = %self.name, error = %e, "event not recorded"),
            }
        }
        log.push(entry.clone());
        // No receivers is normal (nobody watching yet).
        let _ = self.tx.send(entry);
    }

    pub fn len(&self) -> usize {
        lock(&self.log).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Events after `after` (all of them when `None`), a live receiver subscribed before the
    /// snapshot was taken (so nothing falls between the two), and whether the log already holds
    /// its last event.
    pub fn subscribe(&self, after: Option<u64>) -> (Vec<T>, broadcast::Receiver<T>, bool) {
        let log = lock(&self.log);
        let rx = self.tx.subscribe();
        let ended = log.last().is_some_and(Entry::is_last);
        (after_seq(&log, clamp(after, log.len())), rx, ended)
    }

    /// Events after `after` (all of them when `None`), for a subscriber that fell behind.
    pub fn since(&self, after: Option<u64>) -> Vec<T> {
        after_seq(&lock(&self.log), after)
    }
}

/// An id the log has not reached yet (a client from another server life, or a forged header)
/// would make the stream skip every future event: treat it as "after the newest one".
fn clamp(after: Option<u64>, len: usize) -> Option<u64> {
    let newest = (len as u64).checked_sub(1);
    match (after, newest) {
        (Some(a), Some(n)) => Some(a.min(n)),
        (Some(_), None) => None,
        (None, _) => None,
    }
}

fn after_seq<T: Entry>(log: &[T], after: Option<u64>) -> Vec<T> {
    log.iter()
        .filter(|e| after.is_none_or(|a| e.seq() > a))
        .cloned()
        .collect()
}

/// The `Last-Event-ID` an `EventSource` sends when it reconnects.
pub fn last_event_id(headers: &HeaderMap) -> Option<u64> {
    headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
}

struct Feed<T> {
    journal: Arc<Journal<T>>,
    pending: VecDeque<T>,
    rx: broadcast::Receiver<T>,
    shutdown: watch::Receiver<bool>,
    last: Option<u64>,
    done: bool,
}

impl<T: Entry> Feed<T> {
    async fn next(&mut self) -> Option<T> {
        loop {
            if self.done {
                return None;
            }
            if let Some(entry) = self.pending.pop_front() {
                if self.last.is_some_and(|l| entry.seq() <= l) {
                    continue;
                }
                self.last = Some(entry.seq());
                self.done = entry.is_last();
                return Some(entry);
            }
            let received = tokio::select! {
                r = self.rx.recv() => r,
                _ = self.shutdown.wait_for(|stop| *stop) => return None,
            };
            match received {
                Ok(entry) => self.pending.push_back(entry),
                // Fell behind the live channel: refill from the log, which has everything.
                Err(RecvError::Lagged(_)) => self.pending.extend(self.journal.since(self.last)),
                Err(RecvError::Closed) => return None,
            }
        }
    }
}

fn sse_event<T: Entry>(entry: &T) -> Event {
    let data = serde_json::to_string(entry).unwrap_or_else(|_| "{}".into());
    Event::default()
        .id(entry.seq().to_string())
        .event(entry.kind())
        .data(data)
}

/// The event stream for `journal`. A journal that already holds its last event and has
/// nothing left to send answers 204, which tells an `EventSource` not to reconnect (it would
/// otherwise reconnect with the last id after every close, forever). This is decided from the
/// log itself, not from the producer's state, so a reconnect that lands between "finished" and
/// the last event still receives that event.
pub fn stream<T: Entry>(
    journal: Arc<Journal<T>>,
    after: Option<u64>,
    shutdown: watch::Receiver<bool>,
) -> Response {
    // The feed skips everything up to `after`, so it must use the clamped id as well.
    let after = clamp(after, journal.len());
    let (backlog, rx, ended) = journal.subscribe(after);
    if ended && backlog.is_empty() {
        return StatusCode::NO_CONTENT.into_response();
    }
    let feed = Feed {
        journal,
        pending: backlog.into(),
        rx,
        shutdown,
        last: after,
        done: false,
    };
    let stream = futures_util::stream::unfold(feed, |mut feed| async move {
        let entry = feed.next().await?;
        Some((Ok::<_, Infallible>(sse_event(&entry)), feed))
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[derive(Debug, Clone, Serialize)]
    struct Note {
        seq: u64,
        last: bool,
    }

    impl Entry for Note {
        fn seq(&self) -> u64 {
            self.seq
        }
        fn kind(&self) -> &'static str {
            "note"
        }
        fn is_last(&self) -> bool {
            self.last
        }
    }

    fn note(last: bool) -> impl FnOnce(u64) -> Note {
        move |seq| Note { seq, last }
    }

    /// The producer may be marked finished before its last event is logged. A reconnect in
    /// that gap must still be served (not a 204), and must receive the last event.
    #[tokio::test]
    async fn a_reconnect_before_the_last_event_is_logged_still_gets_it() {
        let journal = Arc::new(Journal::<Note>::new("t", None));
        journal.append(note(false));
        let (_tx, shutdown) = watch::channel(false);
        let response = stream(Arc::clone(&journal), Some(0), shutdown);
        assert_eq!(response.status(), StatusCode::OK);
        journal.append(note(true));
        let mut body = response.into_body().into_data_stream();
        let mut text = String::new();
        while let Some(chunk) = tokio::time::timeout(std::time::Duration::from_secs(5), body.next())
            .await
            .expect("the stream ends after the last event")
        {
            text.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
        }
        assert!(text.contains("id: 1"), "{text}");
        assert!(text.contains("\"last\":true"), "{text}");
    }

    #[tokio::test]
    async fn a_reconnect_after_the_last_event_gets_204() {
        let journal = Arc::new(Journal::<Note>::new("t", None));
        journal.append(note(false));
        journal.append(note(true));
        let (_tx, shutdown) = watch::channel(false);
        assert_eq!(
            stream(Arc::clone(&journal), Some(1), shutdown.clone()).status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(stream(journal, Some(0), shutdown).status(), StatusCode::OK);
    }

    /// An id past the end of a live log must not make the stream skip what comes next.
    #[tokio::test]
    async fn an_id_from_the_future_still_gets_the_last_event() {
        let journal = Arc::new(Journal::<Note>::new("t", None));
        journal.append(note(false));
        let (_tx, shutdown) = watch::channel(false);
        let response = stream(Arc::clone(&journal), Some(99), shutdown);
        assert_eq!(response.status(), StatusCode::OK);
        journal.append(note(true));
        let mut body = response.into_body().into_data_stream();
        let mut text = String::new();
        while let Some(chunk) = tokio::time::timeout(std::time::Duration::from_secs(5), body.next())
            .await
            .expect("the stream ends after the last event")
        {
            text.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
        }
        assert!(text.contains("id: 1"), "{text}");
        assert!(!text.contains("id: 0"), "{text}");
    }
}
