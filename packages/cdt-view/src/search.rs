//! Search off the key loop. Typing used to run the whole walk (or spawn `rg`)
//! inline, so every keystroke stalled the UI on a large tree. Here one worker
//! thread does the work and the loop just posts queries and picks up results.
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Kind {
    /// File and folder names, via the `ignore` walker.
    Names,
    /// File contents, via the `rg` binary.
    Content,
}

struct Job {
    seq: u64,
    kind: Kind,
    root: PathBuf,
    query: String,
    hidden: bool,
}

/// One finished search. `seq` says which query it answers.
pub(crate) struct Done {
    seq: u64,
    pub(crate) hits: Vec<PathBuf>,
    pub(crate) err: Option<String>,
}

pub(crate) struct Search {
    tx: Sender<Job>,
    rx: Receiver<Done>,
    /// Bumped per query, so a result that arrives after the query moved on is
    /// recognised as stale and dropped.
    seq: u64,
    inflight: bool,
}

impl Search {
    pub(crate) fn spawn() -> Self {
        let (tx, job_rx) = channel::<Job>();
        let (done_tx, rx) = channel::<Done>();
        // Detached: the worker ends when `tx` drops with the App.
        thread::spawn(move || worker(job_rx, done_tx));
        Search {
            tx,
            rx,
            seq: 0,
            inflight: false,
        }
    }

    pub(crate) fn request(&mut self, kind: Kind, root: PathBuf, query: String, hidden: bool) {
        self.seq += 1;
        self.inflight = self
            .tx
            .send(Job {
                seq: self.seq,
                kind,
                root,
                query,
                hidden,
            })
            .is_ok();
    }

    /// Stop caring about whatever is running; its result will be discarded.
    pub(crate) fn cancel(&mut self) {
        self.seq += 1;
        self.inflight = false;
    }

    /// The newest result for the current query, if one has landed. Results for
    /// superseded queries are thrown away here.
    pub(crate) fn take_fresh(&mut self) -> Option<Done> {
        let mut fresh = None;
        while let Ok(d) = self.rx.try_recv() {
            if d.seq == self.seq {
                fresh = Some(d);
            }
        }
        if fresh.is_some() {
            self.inflight = false;
        }
        fresh
    }

    /// True while a search for the current query is still running.
    pub(crate) fn pending(&self) -> bool {
        self.inflight
    }

    /// Block for the current query's result, giving up after `cap`. Needed when
    /// Enter lands before the hits do: picking from an empty list would cd to
    /// the root, somewhere the user never selected.
    pub(crate) fn wait(&mut self, cap: Duration) -> Option<Done> {
        let deadline = Instant::now() + cap;
        while self.inflight {
            let left = deadline.checked_duration_since(Instant::now())?;
            match self.rx.recv_timeout(left) {
                Ok(d) if d.seq == self.seq => {
                    self.inflight = false;
                    return Some(d);
                }
                Ok(_) => {}            // a stale answer; keep waiting for ours
                Err(_) => return None, // timed out, or the worker is gone
            }
        }
        None
    }
}

fn worker(jobs: Receiver<Job>, out: Sender<Done>) {
    while let Ok(mut job) = jobs.recv() {
        // Keystrokes that arrived while the last search ran are already
        // obsolete: only the newest query still matters, so drop the rest
        // instead of walking the tree once per character.
        while let Ok(newer) = jobs.try_recv() {
            job = newer;
        }
        // ponytail: a long search runs to completion before the next starts,
        // because neither the `ignore` walker nor `rg` is cancellable here.
        // Coalescing keeps that to one wasted search; add real cancellation
        // only if a single query is slow enough to feel it.
        if out.send(run(job)).is_err() {
            return; // the picker is gone
        }
    }
}

fn run(job: Job) -> Done {
    match job.kind {
        Kind::Names => Done {
            seq: job.seq,
            hits: cdt_search::find_names(&job.root, &job.query, job.hidden),
            err: None,
        },
        Kind::Content => match cdt_search::grep(&job.root, &job.query, job.hidden) {
            Ok(hits) => Done {
                seq: job.seq,
                hits,
                err: None,
            },
            Err(e) => Done {
                seq: job.seq,
                hits: Vec::new(),
                err: Some(format!("rg unavailable: {e}")),
            },
        },
    }
}

/// Prints the hit count rather than 500 paths, so a failing assertion stays
/// readable.
impl std::fmt::Debug for Done {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Done")
            .field("seq", &self.seq)
            .field("hits", &self.hits.len())
            .field("err", &self.err)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn a_requested_search_comes_back_with_hits() {
        let mut s = Search::spawn();
        assert!(!s.pending());
        s.request(Kind::Names, root(), "lib.rs".into(), false);
        assert!(s.pending());

        let done = s.wait(Duration::from_secs(10)).expect("a result");
        assert!(done.hits.iter().any(|p| p.ends_with("lib.rs")), "{done:?}",);
        assert!(!s.pending());
    }

    /// The point of `seq`: a result for an abandoned query must never be shown.
    #[test]
    fn a_result_for_a_superseded_query_is_discarded() {
        let mut s = Search::spawn();
        s.request(Kind::Names, root(), "lib.rs".into(), false);
        s.cancel();
        // Give the worker time to finish and post the now-stale answer.
        thread::sleep(std::time::Duration::from_millis(300));
        assert!(s.take_fresh().is_none(), "stale result leaked through");
        assert!(!s.pending());
    }

    #[test]
    fn take_fresh_keeps_only_the_newest_answer() {
        let mut s = Search::spawn();
        for q in ["l", "li", "lib"] {
            s.request(Kind::Names, root(), q.into(), false);
        }
        let last = s.seq;
        let done = s.wait(Duration::from_secs(10)).expect("a result");
        assert_eq!(done.seq, last, "answered a stale query");
    }

    #[test]
    fn a_missing_rg_is_reported_rather_than_silently_empty() {
        // Only meaningful where rg is absent; where it exists this just checks
        // that a content search answers at all.
        let mut s = Search::spawn();
        s.request(Kind::Content, root(), "Search".into(), false);
        let done = s.wait(Duration::from_secs(10)).expect("a result");
        match done.err {
            Some(e) => assert!(e.contains("rg"), "{e}"),
            None => assert!(!done.hits.is_empty(), "rg found nothing in its own source"),
        }
    }
}
