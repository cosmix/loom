//! Dispatch authenticated requests while retaining their byte-budget permit.
use super::*;

pub(super) struct ClientDispatch<'a> {
    pub shutdown_flag: &'a AtomicBool,
    pub status_subscribers: &'a Mutex<Vec<Subscriber>>,
    pub log_subscribers: &'a Mutex<Vec<Subscriber>>,
    pub work_dir: &'a Path,
}

impl ClientDispatch<'_> {
    pub(super) fn serve(&self, writer: &Subscriber, incoming: AuthorizedRequest) -> Result<bool> {
        let AuthorizedRequest { request, _permit } = incoming;
        match request {
            Request::Ping { .. } => send_response(writer, &Response::Pong)?,
            Request::SubscribeStatus { .. } => {
                subscribe(writer, self.status_subscribers, "Status")?
            }
            Request::SubscribeLogs { .. } => subscribe(writer, self.log_subscribers, "Log")?,
            Request::Stop { .. } => {
                send_response(writer, &Response::Ok)?;
                self.shutdown_flag.store(true, Ordering::SeqCst);
                return Ok(false);
            }
            request => {
                let response = self.control(request);
                send_response(writer, &response)?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn control(&self, request: Request) -> Response {
        match request {
            Request::Unsubscribe { .. } => Response::Ok,
            request @ (Request::DisputeCriteria { .. }
            | Request::FileDispute { .. }
            | Request::BlockStage { .. }
            | Request::FreezeContracts { .. }
            | Request::ObserveChanges { .. }) => serve_stage_control(self.work_dir, request),
            Request::Ping { .. }
            | Request::Stop { .. }
            | Request::SubscribeStatus { .. }
            | Request::SubscribeLogs { .. } => {
                unreachable!("connection request dispatched as stage control")
            }
        }
    }
}
