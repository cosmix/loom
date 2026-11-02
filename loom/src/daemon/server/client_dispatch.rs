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
                subscribe(writer, self.status_subscribers, "Status")?;
                self.replay_completion(writer)?;
            }
            Request::SubscribeLogs { .. } => subscribe(writer, self.log_subscribers, "Log")?,
            Request::Unsubscribe { .. } => {
                let mut stream = writer
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                for subscribers in [self.status_subscribers, self.log_subscribers] {
                    subscribers
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .retain(|existing| !Arc::ptr_eq(existing, writer));
                }
                write_bounded_response(&mut stream, &Response::Ok)?;
                // A broadcaster may already hold a snapshot of either list.
                // Closing the write half makes that stale snapshot fail after
                // the acknowledgement, so no event can follow Unsubscribe.
                let _ = stream.shutdown(std::net::Shutdown::Write);
                return Ok(false);
            }
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

    fn replay_completion(&self, writer: &Subscriber) -> Result<()> {
        let subscribed = self
            .status_subscribers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .any(|entry| Arc::ptr_eq(entry, writer));
        if subscribed
            && self
                .work_dir
                .join(super::super::status::COMPLETION_MARKER)
                .exists()
        {
            for frame in super::super::status::completion_frames(self.work_dir)? {
                send_response(writer, &frame)?;
            }
        }
        Ok(())
    }

    fn control(&self, request: Request) -> Response {
        match request {
            request @ (Request::DisputeCriteria { .. }
            | Request::FileDispute { .. }
            | Request::BlockStage { .. }
            | Request::FreezeContracts { .. }
            | Request::ObserveChanges { .. }) => serve_stage_control(self.work_dir, request),
            Request::Ping { .. }
            | Request::Unsubscribe { .. }
            | Request::Stop { .. }
            | Request::SubscribeStatus { .. }
            | Request::SubscribeLogs { .. } => {
                unreachable!("connection request dispatched as stage control")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::protocol::{read_message, write_message};
    use crate::daemon::server::tokens::USER_TOKEN_FILE;

    #[test]
    fn late_subscriber_receives_persisted_failure_and_summary() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(USER_TOKEN_FILE), "user-secret").unwrap();
        std::fs::create_dir(temp.path().join("stages")).unwrap();
        std::fs::write(
            temp.path().join("orchestrator.complete"),
            r#"{"kind":"scheduler_failure","message":"panic"}"#,
        )
        .unwrap();
        for _ in 0..2 {
            let (server, mut client) = UnixStream::pair().unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let path = temp.path().to_path_buf();
            let handler = std::thread::spawn(move || {
                super::super::handle_client_connection(
                    server,
                    Arc::new(AtomicBool::new(false)),
                    Arc::new(Mutex::new(vec![])),
                    Arc::new(Mutex::new(vec![])),
                    &path,
                    ByteBudget::new(2 * crate::daemon::wire::MAX_REQUEST_BYTES),
                )
            });
            write_message(
                &mut client,
                &Request::SubscribeStatus {
                    auth_token: "user-secret".into(),
                },
            )
            .unwrap();
            assert!(matches!(
                read_message::<Response, _>(&mut client),
                Ok(Response::Ok)
            ));
            assert!(
                matches!(read_message::<Response, _>(&mut client), Ok(Response::Error { message }) if message.contains("Scheduler failure: panic"))
            );
            assert!(
                matches!(read_message::<Response, _>(&mut client), Ok(Response::OrchestrationComplete { summary }) if summary.failure_count == 1)
            );
            drop(client);
            handler.join().unwrap().unwrap();
        }
    }
}
