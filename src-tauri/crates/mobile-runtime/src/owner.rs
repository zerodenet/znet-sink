use crate::{
    control::{Operation, Request},
    ControlHandle, Error, Identity, Phase, RuntimeConfig, Status,
};
use std::{sync::Arc, thread};
use tokio::sync::{mpsc, oneshot, watch};
use zero_api::{CommandService, QueryService};
use zero_engine::EngineHandle;
use zero_proxy::{Proxy, ProxyHandle};

// Full-feature Zero config/protocol paths need an explicit native stack budget;
// production ownership must not depend on test-harness RUST_MIN_STACK.
const NATIVE_STACK_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct Shared {
    pub(crate) status: watch::Sender<Status>,
    stop: watch::Sender<bool>,
}
/// Host lifetime owner. No executor or RunningProxy lives in a UI handle.
/// Dropping the owner requests graceful shutdown; use shutdown().await for a
/// confirmed receipt. Dropping only UI handles never requests shutdown.
pub struct RuntimeOwner {
    shared: Arc<Shared>,
    requests: mpsc::Sender<Request>,
    thread: Option<thread::JoinHandle<()>>,
}
impl RuntimeOwner {
    pub async fn start(config: RuntimeConfig) -> Result<Self, Error> {
        if config.runtime.tun.is_some() {
            return Err(Error::NativeTunAdapterUnavailable);
        }
        let (status, _) = watch::channel(Status {
            phase: Phase::Starting,
            identity: None,
            issue: None,
        });
        let (stop, stop_rx) = watch::channel(false);
        let shared = Arc::new(Shared { status, stop });
        let (requests, receive) = mpsc::channel(64);
        let (startup, started) = oneshot::channel();
        let background = shared.clone();
        let thread = thread::Builder::new()
            .name("znet-proxy-owner".into())
            .stack_size(NATIVE_STACK_BYTES)
            .spawn(move || {
                let executor = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("znet-proxy-io")
                    .thread_stack_size(NATIVE_STACK_BYTES)
                    .enable_all()
                    .build()
                {
                    Ok(executor) => executor,
                    Err(_) => {
                        publish(
                            &background,
                            Phase::Failed,
                            None,
                            Some(Error::ExecutorUnavailable),
                        );
                        let _ = startup.send(Err(Error::ExecutorUnavailable));
                        return;
                    }
                };
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    executor.block_on(run(config, background.clone(), receive, stop_rx, startup))
                }));
                // Tokio shutdown_timeout cannot confirm completion of leaked
                // blocking tasks. Drop on this owner thread and publish Stopped
                // only after actual disposal. A caller may bound its wait and keep
                // Disposing/unknown while this original operation continues.
                drop(executor);
                match outcome {
                    Ok(Ok(identity)) => publish(&background, Phase::Stopped, Some(identity), None),
                    Ok(Err(error)) => {
                        let identity = background.status.borrow().identity.clone();
                        publish(&background, Phase::Failed, identity, Some(error));
                    }
                    Err(_) => {
                        let identity = background.status.borrow().identity.clone();
                        publish(
                            &background,
                            Phase::Failed,
                            identity,
                            Some(Error::CoreFailure),
                        );
                    }
                }
            })
            .map_err(|_| Error::ExecutorUnavailable)?;
        // The owner is constructed before waiting: cancellation of start
        // drops it and still requests cleanup of a proxy that may have spawned.
        let owner = Self {
            shared,
            requests,
            thread: Some(thread),
        };
        match started.await.map_err(|_| Error::ResultUnknown)? {
            Ok(()) => Ok(owner),
            Err(error) => Err(error),
        }
    }
    pub fn control(&self) -> ControlHandle {
        ControlHandle {
            shared: self.shared.clone(),
            requests: self.requests.clone(),
        }
    }
    pub fn status(&self) -> Status {
        self.shared.status.borrow().clone()
    }
    pub async fn shutdown(&mut self) -> Result<Status, Error> {
        let mut status = self.shared.status.subscribe();
        if matches!(status.borrow().phase, Phase::Starting | Phase::Spawned) {
            self.shared
                .status
                .send_modify(|status| status.phase = Phase::Stopping);
            self.shared.stop.send_replace(true);
        }
        loop {
            let snapshot = status.borrow().clone();
            match snapshot.phase {
                Phase::Stopped => return Ok(snapshot),
                Phase::Failed => return Err(snapshot.issue.unwrap_or(Error::CoreFailure)),
                _ => status.changed().await.map_err(|_| Error::ResultUnknown)?,
            }
        }
    }
}
impl Drop for RuntimeOwner {
    fn drop(&mut self) {
        if matches!(self.status().phase, Phase::Starting | Phase::Spawned) {
            self.shared
                .status
                .send_modify(|status| status.phase = Phase::Stopping);
            self.shared.stop.send_replace(true);
        }
        // The thread owns the real shutdown future even if a caller times out.
        // Detaching the JoinHandle does not abort it or claim completion.
        self.thread.take();
    }
}
fn publish(shared: &Shared, phase: Phase, identity: Option<Identity>, issue: Option<Error>) {
    shared.status.send_replace(Status {
        phase,
        identity,
        issue,
    });
}
async fn run(
    config: RuntimeConfig,
    shared: Arc<Shared>,
    mut receive: mpsc::Receiver<Request>,
    mut stop: watch::Receiver<bool>,
    startup: oneshot::Sender<Result<(), Error>>,
) -> Result<Identity, Error> {
    let proxy = match Proxy::new(config) {
        Ok(proxy) => proxy,
        Err(error) => {
            let issue = if matches!(
                error,
                zero_engine::EngineError::CompiledFeatureDisabled { .. }
            ) {
                Error::UncompiledProtocol
            } else {
                Error::InvalidConfiguration
            };
            publish(&shared, Phase::Failed, None, Some(issue.clone()));
            let _ = startup.send(Err(issue.clone()));
            return Err(issue);
        }
    };
    let identity = Identity {
        core_instance_id: proxy.engine().core_instance_id().into(),
        config_revision: proxy.engine().config_revision(),
    };
    let handle = ProxyHandle::new(EngineHandle::new(proxy.engine().clone()), proxy.clone());
    // This is the real network runtime, retained until shutdown is awaited.
    let running = proxy.spawn();
    if !*stop.borrow() {
        publish(&shared, Phase::Spawned, Some(identity.clone()), None);
    }
    let _ = startup.send(Ok(()));
    loop {
        if *stop.borrow() {
            break;
        }
        tokio::select! {
            biased;
            _=stop.changed()=>break,
            request=receive.recv()=>{
                let Some(request)=request else { break; };
                if *stop.borrow() { request.reject(Error::NotSubmitted);continue; }
                if request.abandoned() { continue; }
                let actual=Identity { core_instance_id:proxy.engine().core_instance_id().into(),config_revision:proxy.engine().config_revision() };
                if request.expected != actual { request.reject(Error::StaleIdentity);continue; }
                match request.operation {
                    Operation::Query(query,reply)=>{let _=reply.send(handle.query(query).map_err(|error|Error::Api(error.code)));},
                    Operation::Command(command,reply)=>{
                        // Drain an already-dispatched acknowledged operation;
                        // caller cancellation is not kernel cancellation.
                        let result=handle.execute_acknowledged(command).await.map_err(|error|Error::Api(error.code));
                        let _=reply.send(result);
                        shared.status.send_modify(|status| { status.identity=Some(Identity { core_instance_id:proxy.engine().core_instance_id().into(),config_revision:proxy.engine().config_revision() }); });
                    },
                }
            },
        }
    }
    receive.close();
    while let Some(request) = receive.recv().await {
        request.reject(Error::NotSubmitted);
    }
    publish(
        &shared,
        Phase::Stopping,
        Some(Identity {
            core_instance_id: proxy.engine().core_instance_id().into(),
            config_revision: proxy.engine().config_revision(),
        }),
        None,
    );
    let result = running.shutdown().await;
    let final_identity = Identity {
        core_instance_id: proxy.engine().core_instance_id().into(),
        config_revision: proxy.engine().config_revision(),
    };
    drop(handle);
    drop(proxy);
    publish(
        &shared,
        Phase::Disposing,
        Some(final_identity.clone()),
        None,
    );
    result
        .map(|_| final_identity)
        .map_err(|_| Error::CoreFailure)
}
