use crate::{Error, Identity, Status};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use zero_api::{AuthContext, CommandRequest, CommandResponse, QueryRequest, QueryResponse};

pub(crate) enum Operation {
    Query(QueryRequest, oneshot::Sender<Result<QueryResponse, Error>>),
    Command(
        CommandRequest,
        oneshot::Sender<Result<CommandResponse, Error>>,
    ),
}
pub(crate) struct Request {
    pub expected: Identity,
    pub operation: Operation,
}
impl Request {
    pub(crate) fn abandoned(&self) -> bool {
        match &self.operation {
            Operation::Query(_, reply) => reply.is_closed(),
            Operation::Command(_, reply) => reply.is_closed(),
        }
    }
    pub(crate) fn reject(self, error: Error) {
        match self.operation {
            Operation::Query(_, reply) => {
                let _ = reply.send(Err(error));
            }
            Operation::Command(_, reply) => {
                let _ = reply.send(Err(error));
            }
        }
    }
}
#[derive(Clone)]
pub struct ControlHandle {
    pub(crate) shared: Arc<crate::owner::Shared>,
    pub(crate) requests: mpsc::Sender<Request>,
}
impl ControlHandle {
    pub fn status(&self) -> Status {
        self.shared.status.borrow().clone()
    }
    /// This handle cannot stop or restart the runtime. The native owner remains
    /// alive independently of how many UI handles exist.
    pub async fn query(
        &self,
        expected: &Identity,
        auth: &AuthContext,
        request: QueryRequest,
    ) -> Result<QueryResponse, Error> {
        if !auth.allows(request.required_permission()) {
            return Err(Error::Forbidden);
        }
        let (reply, receive) = oneshot::channel();
        self.submit(Request {
            expected: expected.clone(),
            operation: Operation::Query(request, reply),
        })?;
        receive.await.map_err(|_| Error::ResultUnknown)?
    }
    pub async fn command(
        &self,
        expected: &Identity,
        auth: &AuthContext,
        request: CommandRequest,
    ) -> Result<CommandResponse, Error> {
        if !auth.allows(request.required_permission()) {
            return Err(Error::Forbidden);
        }
        // Never turn a desktop TUN command into claimed native VPN success.
        if matches!(
            request,
            CommandRequest::TunStart(_)
                | CommandRequest::TunStop(_)
                | CommandRequest::TunRecover(_)
        ) {
            return Err(Error::NativeTunAdapterUnavailable);
        }
        if let CommandRequest::ConfigApply(config) | CommandRequest::ConfigApplyRuntime(config) =
            &request
        {
            let config = crate::RuntimeConfig::parse(&config.config.to_string())
                .map_err(|_| Error::InvalidConfiguration)?;
            if config.runtime.tun.is_some() {
                return Err(Error::NativeTunAdapterUnavailable);
            }
        }
        let (reply, receive) = oneshot::channel();
        self.submit(Request {
            expected: expected.clone(),
            operation: Operation::Command(request, reply),
        })?;
        receive.await.map_err(|_| Error::ResultUnknown)?
    }
    fn submit(&self, request: Request) -> Result<(), Error> {
        if self.status().phase != crate::Phase::Spawned {
            return Err(Error::NotSubmitted);
        }
        match self.requests.try_send(request) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(Error::Busy),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(Error::NotSubmitted),
        }
    }
}
