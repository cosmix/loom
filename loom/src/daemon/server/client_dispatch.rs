//! Dispatch authenticated requests while retaining their byte-budget permit.
use super::*;

pub(super) struct ClientDispatch<'a> {
    pub shutdown_flag: &'a AtomicBool,
    pub status_subscribers: &'a Mutex<Vec<UnixStream>>,
    pub log_subscribers: &'a Mutex<Vec<UnixStream>>,
    pub work_dir: &'a Path,
}

impl ClientDispatch<'_> {
    pub(super) fn serve(
        &self,
        stream: &mut UnixStream,
        incoming: AuthorizedRequest,
    ) -> Result<bool> {
        let AuthorizedRequest {
            preface,
            authorization,
            peer_pid,
            request,
            _permit,
        } = incoming;
        match request {
            Request::Ping { .. } => write_message(stream, &Response::Pong)?,
            Request::SubscribeStatus { .. } => {
                subscribe(stream, self.status_subscribers, "Status")?
            }
            Request::SubscribeLogs { .. } => subscribe(stream, self.log_subscribers, "Log")?,
            Request::Stop { .. } => {
                write_message(stream, &Response::Ok)?;
                self.shutdown_flag.store(true, Ordering::SeqCst);
                return Ok(false);
            }
            request => {
                let response = self.control(&preface, authorization, peer_pid, request);
                write_message(stream, &response)?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn control(
        &self,
        preface: &RequestPreface,
        authorization: Authorization,
        peer_pid: Option<u32>,
        request: Request,
    ) -> Response {
        match request {
            Request::Unsubscribe { .. } => Response::Ok,
            Request::ValidateInheritedConfinement {
                stage_id,
                session_id,
                working_dir,
                ..
            } => crate::daemon::inherited::authorize(
                self.work_dir,
                &stage_id,
                &session_id,
                &working_dir,
                peer_pid,
            ),
            request
            @ (Request::CompleteStage { .. } | Request::RecordCompletionEvidence { .. }) => {
                complete(preface, authorization, peer_pid, request, self.work_dir)
            }
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
