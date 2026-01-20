use super::super::*;

pub fn create_test_session(status: SessionStatus) -> Session {
    let mut session = Session::new();
    session.status = status;
    session
}
