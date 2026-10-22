//! One-shot generations outside any task, such as the commit messages and reviews of the Git
//! workspace.
//!
//! A generation runs on a thread of its own. The client starts it under an identity that it
//! chooses and reads its progress in short waits, because a review can outlast the read timeout
//! of one connection.
use crate::{Shared, accounts, provider};
use anyhow::{Result, ensure};
use kodem_core::*;
use std::time::{Duration, Instant};

/// Longest wait of one status request. The client's read timeout is longer.
const WAIT_LIMIT: Duration = Duration::from_secs(30);
/// How long the host keeps a finished generation that no client reads.
const KEEP_LIMIT: Duration = Duration::from_secs(10 * 60);

/// A running or finished generation.
#[derive(Default)]
pub(crate) struct Job {
    events: Vec<GenerationEvent>,
    outcome: Option<std::result::Result<String, String>>,
    finished: Option<Instant>,
}

/// Starts `generation` under the account it names, on a thread of its own.
pub(crate) fn start(host: &Shared, job: String, generation: Generation) -> Result<()> {
    let account = {
        let mut h = host.lock().unwrap();
        ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
        ensure!(
            !h.generations.contains_key(&job),
            "A generation with this identity already runs"
        );
        // Results that no client read are dropped after a while.
        h.generations.retain(|_, job| {
            job.finished
                .is_none_or(|finished| finished.elapsed() < KEEP_LIMIT)
        });
        let account = accounts::resolve(&h, generation.provider, generation.account.as_deref())?;
        h.generations.insert(job.clone(), Job::default());
        account
    };
    let host = host.clone();
    std::thread::spawn(move || {
        let mut progress = |event: GenerationEvent| {
            if let Some(job) = host.lock().unwrap().generations.get_mut(&job) {
                job.events.push(event);
            }
        };
        let outcome = provider::generate(&host, &account, &generation, &mut progress)
            .map_err(|error| format!("{error:#}"));
        if let Some(job) = host.lock().unwrap().generations.get_mut(&job) {
            job.outcome = Some(outcome);
            job.finished = Some(Instant::now());
        }
    });
    Ok(())
}

/// Returns the progress of a generation after its first `after` events, waiting up to
/// `wait_ms` for news. The host forgets a generation once a status reports it done.
pub(crate) fn status(
    host: &Shared,
    job: &str,
    after: usize,
    wait_ms: u64,
) -> Result<GenerationStatus> {
    let until = Instant::now() + Duration::from_millis(wait_ms).min(WAIT_LIMIT);
    loop {
        {
            let mut h = host.lock().unwrap();
            ensure!(!h.shutdown, "Host is restarting. Retry after reconnecting.");
            let Some(current) = h.generations.get(job) else {
                anyhow::bail!("The host no longer knows this generation");
            };
            let news = current.events.len() > after || current.outcome.is_some();
            if news || Instant::now() >= until {
                let events = current.events.get(after..).unwrap_or_default().to_vec();
                let next = current.events.len();
                let status = match &current.outcome {
                    Some(outcome) => {
                        let status = GenerationStatus {
                            events,
                            next,
                            done: true,
                            result: outcome.as_ref().ok().cloned(),
                            error: outcome.as_ref().err().cloned(),
                        };
                        h.generations.remove(job);
                        status
                    }
                    None => GenerationStatus {
                        events,
                        next,
                        ..GenerationStatus::default()
                    },
                };
                return Ok(status);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
