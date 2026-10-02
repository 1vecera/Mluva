//! Capture-ordered terminal cleanup values consumed by recording completion.
//!
//! The asynchronous segment scheduler owns candidate validation. Completion binds
//! its terminal snapshot to the exact session, model and recognition string.

use mluva_core::{personalization::integrity_violations, text};
use mluva_providers::{codex::CodexAppServerClient, compatible::RewriteOptions};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub const MAX_SEGMENT_CHARACTERS: usize = 8_000;
pub const MAX_RESPONSE_CHARACTERS: usize = 8_000;

#[derive(Clone, Debug)]
pub struct SegmentCleanupConfiguration {
    pub concurrency_limit: usize,
    pub pending_capacity: usize,
    pub attempt_timeout: Duration,
    pub stop_drain_timeout: Duration,
    pub segment_character_limit: usize,
    pub response_character_limit: usize,
}
impl Default for SegmentCleanupConfiguration {
    fn default() -> Self {
        Self {
            concurrency_limit: 2,
            pending_capacity: 8,
            attempt_timeout: Duration::from_secs(8),
            stop_drain_timeout: Duration::from_secs(2),
            segment_character_limit: MAX_SEGMENT_CHARACTERS,
            response_character_limit: MAX_RESPONSE_CHARACTERS,
        }
    }
}
impl SegmentCleanupConfiguration {
    fn validate(&self) -> Result<(), String> {
        if self.concurrency_limit == 0 {
            return Err("Cleanup concurrency must be positive.".into());
        }
        if self.attempt_timeout.is_zero() || self.stop_drain_timeout.is_zero() {
            return Err("Cleanup timeouts must be positive.".into());
        }
        if self.segment_character_limit == 0 || self.response_character_limit == 0 {
            return Err("Cleanup request and response bounds must be positive.".into());
        }
        Ok(())
    }
}

pub type CleanupFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub trait SegmentCleanupAttempt: Send + Sync {
    fn transform<'a>(
        &'a self,
        prepared: &'a str,
    ) -> CleanupFuture<'a, mluva_providers::Result<String>>;
    fn cancel(&self);
    fn close(&self) -> CleanupFuture<'_, ()>;
}
pub type SegmentAttemptFactory =
    Arc<dyn Fn() -> Result<Arc<dyn SegmentCleanupAttempt>, String> + Send + Sync>;
pub type SegmentPreparation = Arc<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

pub struct CodexSegmentCleanupAttempt {
    pub client: CodexAppServerClient,
    pub cwd: PathBuf,
    pub model_identifier: String,
    pub instructions: String,
}
impl SegmentCleanupAttempt for CodexSegmentCleanupAttempt {
    fn transform<'a>(
        &'a self,
        prepared: &'a str,
    ) -> CleanupFuture<'a, mluva_providers::Result<String>> {
        Box::pin(async move {
            self.client
                .transform(
                    &cleanup_prompt(prepared, &self.instructions),
                    &self.cwd,
                    RewriteOptions {
                        model: Some(&self.model_identifier),
                        ..Default::default()
                    },
                    None,
                )
                .await
        })
    }
    fn cancel(&self) {
        self.client.cancel();
    }
    fn close(&self) -> CleanupFuture<'_, ()> {
        Box::pin(self.client.close())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SegmentCleanupState {
    Raw,
    Rewriting,
    Waiting,
    Cleaned,
    Fallback,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lifecycle {
    Active,
    Stopping,
    Finished,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentCleanupProjection {
    pub identifier: String,
    pub sequence: i64,
    pub raw_text: String,
    pub revised_text: Option<String>,
    pub state: SegmentCleanupState,
    pub failure: Option<SegmentCleanupFailure>,
}

pub fn cleanup_prompt(text: &str, instructions: &str) -> String {
    format!(
        "{instructions} Preserve every fact, number, name, URL, path, identifier, command, and negation. Return only the cleaned text.\n\nDICTATION:\n{text}"
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SegmentCleanupFailure {
    Cancelled,
    InputTooLarge,
    MalformedOutput,
    OutputTooLarge,
    Processing,
    Provider,
    Safety,
    SkippedCapacity,
    Timeout,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentCleanupTerminalSegment {
    pub identifier: String,
    pub sequence: i64,
    pub raw_text: String,
    pub selected_text: String,
    pub failure: Option<SegmentCleanupFailure>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SegmentCleanupTerminalSnapshot {
    pub session_identifier: String,
    pub provider_identifier: String,
    pub model_identifier: String,
    pub segments: Vec<SegmentCleanupTerminalSegment>,
    pub stop_drain_seconds: f64,
}

impl SegmentCleanupTerminalSnapshot {
    pub fn raw_text(&self) -> String {
        join_segments(
            self.segments
                .iter()
                .map(|segment| segment.raw_text.as_str()),
        )
    }
    pub fn selected_text(&self) -> String {
        join_segments(
            self.segments
                .iter()
                .map(|segment| segment.selected_text.as_str()),
        )
    }
    pub fn successful_segments(&self) -> usize {
        self.segments
            .iter()
            .filter(|segment| segment.failure.is_none())
            .count()
    }
    pub fn failed_segments(&self) -> usize {
        self.segments.len() - self.successful_segments()
    }
    pub fn enhancement_outcome(&self) -> &'static str {
        if self.failed_segments() == 0 {
            "completed"
        } else if self.successful_segments() == 0 {
            "raw-fallback"
        } else {
            "safe-fallback"
        }
    }
}

struct Record {
    projection: SegmentCleanupProjection,
    settled: bool,
}
struct Active {
    token: Uuid,
    stop: CancellationToken,
    attempt: Option<Arc<dyn SegmentCleanupAttempt>>,
}
struct Policy {
    lifecycle: Lifecycle,
    accepted: HashSet<String>,
    records: Vec<Record>,
    pending: VecDeque<usize>,
    active: BTreeMap<usize, Active>,
    published: usize,
}
struct Shared {
    session: String,
    provider: String,
    model: String,
    prepare: SegmentPreparation,
    vocabulary: Vec<String>,
    factory: SegmentAttemptFactory,
    configuration: SegmentCleanupConfiguration,
    policy: Mutex<Policy>,
    changed: Notify,
    tasks_done: Notify,
    tasks: AtomicUsize,
    runtime: tokio::runtime::Handle,
}

/// One capture owns bounded independent attempts; only an ordered settled prefix
/// is published. Runtime tasks retain provider cleanup after Stop or caller exit.
pub struct SegmentCleanupSession {
    shared: Arc<Shared>,
}
impl SegmentCleanupSession {
    pub fn new(
        session: String,
        provider: String,
        model: String,
        prepare: SegmentPreparation,
        vocabulary: Vec<String>,
        factory: SegmentAttemptFactory,
        configuration: SegmentCleanupConfiguration,
    ) -> Result<Self, String> {
        if session.is_empty() || provider.is_empty() || model.is_empty() {
            return Err("Cleanup session, provider, and model identifiers are required.".into());
        }
        configuration.validate()?;
        Ok(Self {
            shared: Arc::new(Shared {
                session,
                provider,
                model,
                prepare,
                vocabulary,
                factory,
                configuration,
                policy: Mutex::new(Policy {
                    lifecycle: Lifecycle::Active,
                    accepted: HashSet::new(),
                    records: vec![],
                    pending: VecDeque::new(),
                    active: BTreeMap::new(),
                    published: 0,
                }),
                changed: Notify::new(),
                tasks_done: Notify::new(),
                tasks: AtomicUsize::new(0),
                runtime: tokio::runtime::Handle::current(),
            }),
        })
    }
    pub fn has_stable_segments(&self) -> bool {
        !self.shared.policy.lock().unwrap().records.is_empty()
    }
    pub fn accept_stable_segment(&self, identifier: &str, raw: &str) -> bool {
        let raw = text::trim(raw);
        if raw.is_empty() {
            return false;
        }
        let mut policy = self.shared.policy.lock().unwrap();
        if policy.lifecycle != Lifecycle::Active || !policy.accepted.insert(identifier.into()) {
            return false;
        }
        let sequence = policy.records.len();
        policy.records.push(Record {
            projection: SegmentCleanupProjection {
                identifier: identifier.into(),
                sequence: sequence as i64,
                raw_text: raw.into(),
                revised_text: None,
                state: SegmentCleanupState::Raw,
                failure: None,
            },
            settled: false,
        });
        if raw.chars().count() > self.shared.configuration.segment_character_limit {
            settle(
                &mut policy.records[sequence],
                None,
                Some(SegmentCleanupFailure::InputTooLarge),
            );
            publish(&mut policy);
        } else if policy.active.len() < self.shared.configuration.concurrency_limit {
            self.shared.start(&mut policy, sequence);
        } else if policy.pending.len() < self.shared.configuration.pending_capacity {
            policy.records[sequence].projection.state = SegmentCleanupState::Waiting;
            policy.pending.push_back(sequence);
        } else {
            settle(
                &mut policy.records[sequence],
                None,
                Some(SegmentCleanupFailure::SkippedCapacity),
            );
            publish(&mut policy);
        }
        self.shared.changed.notify_waiters();
        true
    }
    pub fn projection(&self) -> Vec<SegmentCleanupProjection> {
        self.shared
            .policy
            .lock()
            .unwrap()
            .records
            .iter()
            .map(|record| record.projection.clone())
            .collect()
    }
    pub async fn stop_and_drain(&self) -> SegmentCleanupTerminalSnapshot {
        let started = Instant::now();
        {
            let mut policy = self.shared.policy.lock().unwrap();
            if matches!(policy.lifecycle, Lifecycle::Finished | Lifecycle::Cancelled) {
                return self.shared.snapshot(&policy, started.elapsed());
            }
            if policy.lifecycle == Lifecycle::Active {
                policy.lifecycle = Lifecycle::Stopping;
                for sequence in std::mem::take(&mut policy.pending) {
                    settle(
                        &mut policy.records[sequence],
                        None,
                        Some(SegmentCleanupFailure::Cancelled),
                    );
                }
                publish(&mut policy);
            }
        }
        let deadline = tokio::time::Instant::now() + self.shared.configuration.stop_drain_timeout;
        loop {
            let changed = self.shared.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.shared.policy.lock().unwrap().active.is_empty() {
                break;
            }
            if tokio::time::timeout_at(deadline, changed).await.is_err() {
                break;
            }
        }
        let (snapshot, closing) = {
            let mut policy = self.shared.policy.lock().unwrap();
            let closing = std::mem::take(&mut policy.active);
            for sequence in closing.keys() {
                settle(
                    &mut policy.records[*sequence],
                    None,
                    Some(SegmentCleanupFailure::Cancelled),
                );
            }
            publish(&mut policy);
            policy.lifecycle = Lifecycle::Finished;
            (self.shared.snapshot(&policy, started.elapsed()), closing)
        };
        interrupt(closing.into_values());
        self.shared.changed.notify_waiters();
        snapshot
    }
    pub fn cancel(&self) {
        let closing = {
            let mut policy = self.shared.policy.lock().unwrap();
            if matches!(policy.lifecycle, Lifecycle::Finished | Lifecycle::Cancelled) {
                return;
            }
            policy.lifecycle = Lifecycle::Cancelled;
            policy.pending.clear();
            let closing = std::mem::take(&mut policy.active);
            for record in &mut policy.records {
                if !matches!(
                    record.projection.state,
                    SegmentCleanupState::Cleaned | SegmentCleanupState::Fallback
                ) {
                    settle(record, None, Some(SegmentCleanupFailure::Cancelled));
                    record.projection.state = SegmentCleanupState::Cancelled;
                }
            }
            policy.published = policy.records.len();
            closing
        };
        interrupt(closing.into_values());
        self.shared.changed.notify_waiters();
    }
    pub async fn cancel_and_wait(&self) {
        self.cancel();
        self.wait_closed().await;
    }
    pub async fn wait_closed(&self) {
        loop {
            let changed = self.shared.tasks_done.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.shared.tasks.load(Ordering::Acquire) == 0 {
                return;
            }
            changed.await;
        }
    }
}
impl Drop for SegmentCleanupSession {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Shared {
    fn start(self: &Arc<Self>, policy: &mut Policy, sequence: usize) {
        let token = Uuid::new_v4();
        let stop = CancellationToken::new();
        policy.records[sequence].projection.state = SegmentCleanupState::Rewriting;
        policy.active.insert(
            sequence,
            Active {
                token,
                stop: stop.clone(),
                attempt: None,
            },
        );
        self.tasks.fetch_add(1, Ordering::AcqRel);
        let done = TaskDone(self.clone());
        let shared = self.clone();
        let working_stop = stop.clone();
        self.runtime.spawn(async move {
            let _done = done;
            shared.run(sequence, token, working_stop).await;
        });
        self.tasks.fetch_add(1, Ordering::AcqRel);
        let done = TaskDone(self.clone());
        let shared = self.clone();
        let deadline = tokio::time::Instant::now() + self.configuration.attempt_timeout;
        self.runtime.spawn(async move {
            let _done = done;
            tokio::select! { biased;
                _ = stop.cancelled() => {},
                _ = tokio::time::sleep_until(deadline) => shared.expire(sequence, token),
            }
        });
    }
    async fn run(self: Arc<Self>, sequence: usize, token: Uuid, stop: CancellationToken) {
        let raw = self.policy.lock().unwrap().records[sequence]
            .projection
            .raw_text
            .clone();
        let mut attempt = None;
        let work = async {
            let prepare = self.prepare.clone();
            let prepared = tokio::task::spawn_blocking(move || prepare(&raw))
                .await
                .map_err(|_| SegmentCleanupFailure::Processing)?
                .map_err(|_| SegmentCleanupFailure::Processing)?;
            let prepared = text::trim(&prepared);
            if prepared.is_empty() {
                return Err(SegmentCleanupFailure::Processing);
            }
            if prepared.chars().count() > self.configuration.segment_character_limit {
                return Err(SegmentCleanupFailure::InputTooLarge);
            }
            let factory = self.factory.clone();
            let client = tokio::task::spawn_blocking(move || factory())
                .await
                .map_err(|_| SegmentCleanupFailure::Provider)?
                .map_err(|_| SegmentCleanupFailure::Provider)?;
            attempt = Some(client.clone());
            {
                let mut policy = self.policy.lock().unwrap();
                let Some(active) = policy
                    .active
                    .get_mut(&sequence)
                    .filter(|active| active.token == token)
                else {
                    return Err(SegmentCleanupFailure::Cancelled);
                };
                active.attempt = Some(client.clone());
            }
            let candidate = tokio::select! { biased;
                _ = stop.cancelled() => return Err(SegmentCleanupFailure::Cancelled),
                result = client.transform(prepared) => result.map_err(|_|SegmentCleanupFailure::Provider)?,
            };
            let candidate = text::trim(&candidate);
            if candidate.is_empty() {
                return Err(SegmentCleanupFailure::MalformedOutput);
            }
            if candidate.chars().count() > self.configuration.response_character_limit {
                return Err(SegmentCleanupFailure::OutputTooLarge);
            }
            if !integrity_violations(prepared, candidate, &self.vocabulary).is_empty() {
                return Err(SegmentCleanupFailure::Safety);
            }
            Ok(candidate.to_owned())
        };
        let result = work.await;
        {
            let mut policy = self.policy.lock().unwrap();
            if policy
                .active
                .get(&sequence)
                .is_some_and(|active| active.token == token)
            {
                policy.active.remove(&sequence).unwrap().stop.cancel();
                let (candidate, failure) = match result {
                    Ok(candidate) => (Some(candidate), None),
                    Err(failure) => (None, Some(failure)),
                };
                settle(&mut policy.records[sequence], candidate, failure);
                publish(&mut policy);
                while policy.lifecycle == Lifecycle::Active
                    && policy.active.len() < self.configuration.concurrency_limit
                {
                    let Some(next) = policy.pending.pop_front() else {
                        break;
                    };
                    self.start(&mut policy, next);
                }
                self.changed.notify_waiters();
            }
        }
        if let Some(attempt) = attempt {
            attempt.cancel();
            attempt.close().await;
        }
    }
    fn expire(self: &Arc<Self>, sequence: usize, token: Uuid) {
        let active = {
            let mut policy = self.policy.lock().unwrap();
            if policy
                .active
                .get(&sequence)
                .is_none_or(|active| active.token != token)
            {
                return;
            }
            let active = policy.active.remove(&sequence).unwrap();
            settle(
                &mut policy.records[sequence],
                None,
                Some(SegmentCleanupFailure::Timeout),
            );
            publish(&mut policy);
            while policy.lifecycle == Lifecycle::Active
                && policy.active.len() < self.configuration.concurrency_limit
            {
                let Some(next) = policy.pending.pop_front() else {
                    break;
                };
                self.start(&mut policy, next);
            }
            self.changed.notify_waiters();
            active
        };
        interrupt(std::iter::once(active));
    }
    fn snapshot(&self, policy: &Policy, elapsed: Duration) -> SegmentCleanupTerminalSnapshot {
        SegmentCleanupTerminalSnapshot {
            session_identifier: self.session.clone(),
            provider_identifier: self.provider.clone(),
            model_identifier: self.model.clone(),
            stop_drain_seconds: elapsed.as_secs_f64(),
            segments: policy
                .records
                .iter()
                .map(|record| {
                    let projection = &record.projection;
                    let cleaned = projection.state == SegmentCleanupState::Cleaned
                        && projection.revised_text.is_some();
                    SegmentCleanupTerminalSegment {
                        identifier: projection.identifier.clone(),
                        sequence: projection.sequence,
                        raw_text: projection.raw_text.clone(),
                        selected_text: if cleaned {
                            projection.revised_text.clone().unwrap()
                        } else {
                            projection.raw_text.clone()
                        },
                        failure: if cleaned {
                            projection.failure
                        } else {
                            Some(
                                projection
                                    .failure
                                    .unwrap_or(SegmentCleanupFailure::Cancelled),
                            )
                        },
                    }
                })
                .collect(),
        }
    }
}
fn settle(record: &mut Record, candidate: Option<String>, failure: Option<SegmentCleanupFailure>) {
    record.projection.revised_text = if failure.is_none() { candidate } else { None };
    record.projection.failure = failure;
    record.projection.state = SegmentCleanupState::Waiting;
    record.settled = true;
}
fn publish(policy: &mut Policy) {
    while let Some(record) = policy.records.get_mut(policy.published) {
        if record.projection.state != SegmentCleanupState::Waiting || !record.settled {
            break;
        }
        record.projection.state = if record.projection.failure.is_none() {
            SegmentCleanupState::Cleaned
        } else {
            SegmentCleanupState::Fallback
        };
        policy.published += 1;
    }
}
fn interrupt(attempts: impl Iterator<Item = Active>) {
    for active in attempts {
        active.stop.cancel();
        if let Some(attempt) = active.attempt {
            attempt.cancel();
        }
    }
}
struct TaskDone(Arc<Shared>);
impl Drop for TaskDone {
    fn drop(&mut self) {
        self.0.tasks.fetch_sub(1, Ordering::AcqRel);
        self.0.tasks_done.notify_waiters();
    }
}

fn join_segments<'a>(segments: impl Iterator<Item = &'a str>) -> String {
    segments
        .map(text::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
