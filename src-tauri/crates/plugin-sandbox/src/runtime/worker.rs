//! Process-wide bounded VM execution broker. This queue schedules CPU/VM
//! execution only; business schedules, SDK work and retries retain their owners.
use crate::{contract::Error, policy::Lease};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Condvar, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

pub const GLOBAL_WORKERS: usize = 2;
pub const GLOBAL_QUEUE_LIMIT: usize = 32;
pub const QUEUE_WAIT_LIMIT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
type Work = Box<dyn FnOnce() -> Result<Value, Error> + Send + 'static>;

struct Owner {
    id: u64,
    accepting: Arc<AtomicBool>,
}
struct Job {
    id: u64,
    owner: Arc<Owner>,
    lease: Arc<Lease>,
    cancelled: Arc<AtomicBool>,
    work: Work,
    reply: mpsc::SyncSender<Result<Value, Error>>,
}
struct Active {
    owner: Arc<Owner>,
    lease: Arc<Lease>,
    cancelled: Arc<AtomicBool>,
}
struct State {
    started: bool,
    failed: bool,
    next_owner: u64,
    next_job: u64,
    queue: VecDeque<Job>,
    active: BTreeMap<u64, Active>,
    submitted: u64,
    completed: u64,
}
struct Broker {
    state: Mutex<State>,
    changed: Condvar,
    workers: usize,
    queue_limit: usize,
}

/// Metadata only. A drained owner has no executing VM/SDK callback, but the
/// shared broker threads may remain idle for other independent host owners.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerSnapshot {
    pub accepting: bool,
    pub queued: usize,
    pub running: usize,
    pub draining: usize,
    pub drained: bool,
    pub global_running: usize,
    pub global_queued: usize,
    pub global_worker_limit: usize,
    pub global_queue_limit: usize,
    pub queue_wait_limit_ms: u64,
    pub ordinary_execution_wall_max_ms: u64,
    pub scheduled_execution_wall_max_ms: u64,
}
#[derive(Clone)]
pub struct Executor {
    broker: Arc<Broker>,
    owner: Arc<Owner>,
}
fn global() -> Arc<Broker> {
    static BROKER: OnceLock<Arc<Broker>> = OnceLock::new();
    BROKER
        .get_or_init(|| Broker::new(GLOBAL_WORKERS, GLOBAL_QUEUE_LIMIT))
        .clone()
}
impl Default for Executor {
    fn default() -> Self {
        global().owner()
    }
}
impl Broker {
    fn new(workers: usize, queue_limit: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                started: false,
                failed: false,
                next_owner: 0,
                next_job: 0,
                queue: VecDeque::new(),
                active: BTreeMap::new(),
                submitted: 0,
                completed: 0,
            }),
            changed: Condvar::new(),
            workers,
            queue_limit,
        })
    }
    fn owner(self: &Arc<Self>) -> Executor {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let id = state.next_owner.checked_add(1);
        state.next_owner = id.unwrap_or(u64::MAX);
        Executor {
            broker: self.clone(),
            owner: Arc::new(Owner {
                id: id.unwrap_or(0),
                accepting: Arc::new(AtomicBool::new(id.is_some() && !state.failed)),
            }),
        }
    }
    fn start(self: &Arc<Self>, state: &mut State) -> Result<(), Error> {
        if state.failed {
            return Err(Error::HostFailure);
        }
        if state.started {
            return Ok(());
        }
        state.started = true;
        for index in 0..self.workers {
            let broker = self.clone();
            if thread::Builder::new()
                .name(format!("znet-plugin-vm-{index}"))
                .spawn(move || broker.run())
                .is_err()
            {
                state.failed = true;
                return Err(Error::HostFailure);
            }
        }
        Ok(())
    }
    fn run(self: Arc<Self>) {
        loop {
            let job = {
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    let mut index = 0;
                    while index < state.queue.len() {
                        let invalid = {
                            let job = &state.queue[index];
                            check(&job.owner, &job.lease, &job.cancelled).err()
                        };
                        if let Some(error) = invalid {
                            let job = state.queue.remove(index).expect("existing queue slot");
                            let _ = job.reply.send(Err(error));
                            state.completed = state.completed.saturating_add(1);
                            self.changed.notify_all();
                        } else {
                            index += 1;
                        }
                    }
                    if let Some(job) = state.queue.pop_front() {
                        state.active.insert(
                            job.id,
                            Active {
                                owner: job.owner.clone(),
                                lease: job.lease.clone(),
                                cancelled: job.cancelled.clone(),
                            },
                        );
                        break job;
                    }
                    if state.failed {
                        return;
                    }
                    let waited = self
                        .changed
                        .wait_timeout(state, POLL_INTERVAL)
                        .unwrap_or_else(|e| e.into_inner());
                    state = waited.0;
                }
            };
            let result = if let Err(error) = check(&job.owner, &job.lease, &job.cancelled) {
                Err(error)
            } else {
                catch_unwind(AssertUnwindSafe(job.work)).unwrap_or(Err(Error::HostFailure))
            };
            let result = check(&job.owner, &job.lease, &job.cancelled).and(result);
            // Work has returned and dropped its VM before the active slot is freed.
            {
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                state.active.remove(&job.id);
                state.completed = state.completed.saturating_add(1);
                self.changed.notify_all();
            }
            drop(job.lease);
            drop(job.owner);
            drop(job.cancelled);
            let _ = job.reply.send(result);
        }
    }
}
fn check(owner: &Owner, lease: &Lease, cancelled: &AtomicBool) -> Result<(), Error> {
    if !owner.accepting.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    lease.check(None)
}

/// Retains the original grant epoch and single component lease through host
/// state publication. It cannot obtain new permissions after stop/re-enable.
pub struct ExecutionReceipt {
    value: Value,
    lease: Arc<Lease>,
    owner: Arc<Owner>,
    cancelled: Arc<AtomicBool>,
}
impl ExecutionReceipt {
    pub fn check(&self) -> Result<(), Error> {
        check(&self.owner, &self.lease, &self.cancelled)
    }
    pub fn value(&self) -> Result<&Value, Error> {
        self.check()?;
        Ok(&self.value)
    }
    pub fn into_value(self) -> Result<Value, Error> {
        self.check()?;
        Ok(self.value)
    }
}
impl Executor {
    pub(crate) fn open(&self) -> Arc<AtomicBool> {
        // A separate cancel signal is not needed: VM callbacks receive this
        // actual owner's accepting flag, which cannot be reset by guest input.
        self.owner.accepting.clone()
    }
    pub(crate) fn is_accepting(&self) -> bool {
        self.owner.accepting.load(Ordering::Acquire)
    }
    pub(crate) fn execute(
        &self,
        lease: Arc<Lease>,
        cancelled: Arc<AtomicBool>,
        work: Work,
    ) -> Result<ExecutionReceipt, Error> {
        check(&self.owner, &lease, &cancelled)?;
        let (reply, receive) = mpsc::sync_channel(1);
        let id = {
            let mut state = self.broker.state.lock().unwrap_or_else(|e| e.into_inner());
            check(&self.owner, &lease, &cancelled)?;
            self.broker.start(&mut state)?;
            if state.queue.len() >= self.broker.queue_limit {
                return Err(Error::Busy);
            }
            let id = state.next_job.checked_add(1).ok_or(Error::BudgetExceeded)?;
            state.next_job = id;
            state.submitted = state.submitted.saturating_add(1);
            state.queue.push_back(Job {
                id,
                owner: self.owner.clone(),
                lease: lease.clone(),
                cancelled: cancelled.clone(),
                work,
                reply,
            });
            self.broker.changed.notify_all();
            id
        };
        // Never return "cancelled/finished" while a blocking native SDK call is
        // still executing. close/snapshot expose draining without pretending it stopped.
        let value = loop {
            match receive.recv_timeout(POLL_INTERVAL) {
                Ok(result) => break result?,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(Error::HostFailure),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Err(error) = check(&self.owner, &lease, &cancelled) {
                        let mut state = self.broker.state.lock().unwrap_or_else(|e| e.into_inner());
                        // Remove only an undispatched request. Active native IO
                        // keeps its slot until it actually returns.
                        if let Some(index) = state.queue.iter().position(|job| job.id == id) {
                            state.queue.remove(index);
                            state.completed = state.completed.saturating_add(1);
                            self.broker.changed.notify_all();
                            return Err(error);
                        }
                    }
                }
            }
        };
        check(&self.owner, &lease, &cancelled)?;
        Ok(ExecutionReceipt {
            value,
            lease,
            owner: self.owner.clone(),
            cancelled,
        })
    }
    pub fn snapshot(&self) -> WorkerSnapshot {
        let state = self.broker.state.lock().unwrap_or_else(|e| e.into_inner());
        let running = state
            .active
            .values()
            .filter(|job| job.owner.id == self.owner.id)
            .count();
        let draining = state
            .active
            .values()
            .filter(|job| {
                job.owner.id == self.owner.id
                    && check(&job.owner, &job.lease, &job.cancelled).is_err()
            })
            .count();
        let queued = state
            .queue
            .iter()
            .filter(|job| job.owner.id == self.owner.id)
            .count();
        WorkerSnapshot {
            accepting: self.is_accepting() && !state.failed,
            queued,
            running,
            draining,
            drained: queued == 0 && running == 0,
            global_running: state.active.len(),
            global_queued: state.queue.len(),
            global_worker_limit: self.broker.workers,
            global_queue_limit: self.broker.queue_limit,
            queue_wait_limit_ms: 5000,
            ordinary_execution_wall_max_ms: 2000,
            scheduled_execution_wall_max_ms: 120000,
        }
    }
    /// Non-waiting close: stop admissions, cancel queued work, deny active late
    /// deliveries. In-flight native work may still drain under its SDK deadline.
    fn stop_admissions(&self) -> WorkerSnapshot {
        self.owner.accepting.store(false, Ordering::Release);
        {
            let mut state = self.broker.state.lock().unwrap_or_else(|e| e.into_inner());
            let mut index = 0;
            while index < state.queue.len() {
                if state.queue[index].owner.id == self.owner.id {
                    let job = state.queue.remove(index).expect("existing queue slot");
                    job.cancelled.store(true, Ordering::Release);
                    let _ = job.reply.send(Err(Error::Cancelled));
                    state.completed = state.completed.saturating_add(1);
                } else {
                    index += 1;
                }
            }
            for active in state
                .active
                .values()
                .filter(|active| active.owner.id == self.owner.id)
            {
                active.cancelled.store(true, Ordering::Release);
            }
            self.broker.changed.notify_all();
        }
        self.snapshot()
    }
    /// Finite wait for this owner only; reports remaining active work honestly.
    pub fn close(&self, wait: Duration) -> WorkerSnapshot {
        self.stop_admissions();
        let deadline = Instant::now() + wait.min(Duration::from_secs(120));
        let mut state = self.broker.state.lock().unwrap_or_else(|e| e.into_inner());
        while state
            .active
            .values()
            .any(|active| active.owner.id == self.owner.id)
        {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            state = self
                .broker
                .changed
                .wait_timeout(state, deadline.saturating_duration_since(now))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        drop(state);
        self.snapshot()
    }
}

#[cfg(test)]
mod tests;
