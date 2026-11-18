//! Tests for the authoring holder state machine.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::plan_server::authoring::{Authoring, AuthoringEvent, FreeReason, Refusal, Take, GRACE};
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::presence::Presence;

struct Browser {
    id: BrowserId,
    public: PublicId,
}

fn browser() -> Browser {
    Browser {
        id: BrowserId::mint().unwrap(),
        public: PublicId::mint(),
    }
}

fn t0() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

fn take(auth: &Authoring, who: &Browser) -> Result<(), Refusal> {
    auth.require(&who.id, &who.public, Take::TakeIfFree, false)
}

fn recorder(auth: &Authoring) -> Arc<Mutex<Vec<AuthoringEvent>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    auth.subscribe(Box::new(move |event| {
        sink.lock().unwrap().push(event.clone())
    }));
    events
}

fn not_holder(holder: &Browser) -> Result<(), Refusal> {
    Err(Refusal::NotHolder {
        holder: holder.public.clone(),
    })
}

/// `a` holds authoring with one open tab, closed at `t0`.
fn away_holder(auth: &Authoring, a: &Browser) {
    take(auth, a).unwrap();
    auth.connection_opened(&a.id);
    auth.connection_closed(&a.id, t0());
}

#[test]
fn first_browser_takes_and_every_call_of_it_passes() {
    let auth = Authoring::new(false);
    let events = recorder(&auth);
    let a = browser();
    assert_eq!(take(&auth, &a), Ok(()));
    assert!(auth.is_holder(&a.id));
    assert_eq!(auth.holder(), Some((a.public.clone(), None)));
    assert_eq!(
        auth.require(&a.id, &a.public, Take::HolderOnly, false),
        Ok(())
    );
    assert_eq!(take(&auth, &a), Ok(()));
    let taken = AuthoringEvent::Taken {
        public_id: a.public.clone(),
    };
    assert_eq!(*events.lock().unwrap(), vec![taken]);
}

#[test]
fn second_browser_is_refused_while_held_and_while_away() {
    let auth = Authoring::new(false);
    let (a, b) = (browser(), browser());
    take(&auth, &a).unwrap();
    auth.connection_opened(&a.id);
    assert_eq!(take(&auth, &b), not_holder(&a));
    assert_eq!(
        auth.require(&b.id, &b.public, Take::HolderOnly, false),
        not_holder(&a)
    );
    auth.connection_closed(&a.id, t0());
    assert_eq!(auth.holder(), Some((a.public.clone(), Some(t0()))));
    assert_eq!(take(&auth, &b), not_holder(&a));
    assert!(!auth.is_holder(&b.id));
}

#[test]
fn grace_frees_at_exactly_600_seconds_and_not_at_599() {
    let auth = Authoring::new(false);
    let a = browser();
    away_holder(&auth, &a);
    let events = recorder(&auth);
    auth.tick(t0() + Duration::from_secs(599));
    assert!(auth.is_holder(&a.id));
    assert!(events.lock().unwrap().is_empty());
    auth.tick(t0() + GRACE);
    assert_eq!(auth.holder(), None);
    let freed = AuthoringEvent::Freed {
        reason: FreeReason::GraceExpired,
    };
    assert_eq!(*events.lock().unwrap(), vec![freed]);
}

#[test]
fn tick_never_frees_a_holder_that_is_not_away() {
    let auth = Authoring::new(false);
    let a = browser();
    take(&auth, &a).unwrap();
    auth.connection_opened(&a.id);
    auth.tick(t0() + GRACE * 100);
    assert!(auth.is_holder(&a.id));
}

#[test]
fn closing_one_of_two_holder_tabs_does_not_mark_away() {
    let auth = Authoring::new(false);
    let events = recorder(&auth);
    let a = browser();
    take(&auth, &a).unwrap();
    auth.connection_opened(&a.id);
    auth.connection_opened(&a.id);
    auth.connection_closed(&a.id, t0());
    assert_eq!(auth.holder(), Some((a.public.clone(), None)));
    auth.tick(t0() + GRACE);
    assert!(auth.is_holder(&a.id));
    let later = t0() + Duration::from_secs(5);
    auth.connection_closed(&a.id, later);
    assert_eq!(auth.holder(), Some((a.public.clone(), Some(later))));
    assert_eq!(
        events.lock().unwrap().last(),
        Some(&AuthoringEvent::Away { since: later })
    );
}

#[test]
fn holder_tab_reopening_inside_grace_comes_back() {
    let auth = Authoring::new(false);
    let a = browser();
    away_holder(&auth, &a);
    let events = recorder(&auth);
    auth.connection_opened(&a.id);
    assert_eq!(*events.lock().unwrap(), vec![AuthoringEvent::Back]);
    assert_eq!(auth.holder(), Some((a.public.clone(), None)));
    auth.tick(t0() + GRACE);
    assert!(auth.is_holder(&a.id));
}

#[test]
fn another_browser_connecting_never_clears_away() {
    let auth = Authoring::new(false);
    let (a, b) = (browser(), browser());
    away_holder(&auth, &a);
    auth.connection_opened(&b.id);
    assert_eq!(auth.holder(), Some((a.public.clone(), Some(t0()))));
    auth.tick(t0() + GRACE);
    assert_eq!(auth.holder(), None);
}

#[test]
fn read_only_mode_refuses_take_if_free() {
    let auth = Authoring::new(true);
    let a = browser();
    assert_eq!(take(&auth, &a), Err(Refusal::ReadOnlyMode));
    assert_eq!(
        auth.require(&a.id, &a.public, Take::HolderOnly, false),
        Err(Refusal::ReadOnlyMode)
    );
    assert_eq!(auth.holder(), None);
}

#[test]
fn stopping_refuses_take_if_free_but_not_holder_only() {
    let auth = Authoring::new(false);
    let (a, b) = (browser(), browser());
    assert_eq!(
        auth.require(&a.id, &a.public, Take::TakeIfFree, true),
        Err(Refusal::Stopping)
    );
    assert_eq!(auth.holder(), None);
    take(&auth, &a).unwrap();
    assert_eq!(
        auth.require(&a.id, &a.public, Take::TakeIfFree, true),
        Err(Refusal::Stopping)
    );
    assert_eq!(
        auth.require(&a.id, &a.public, Take::HolderOnly, true),
        Ok(())
    );
    assert_eq!(
        auth.require(&b.id, &b.public, Take::HolderOnly, true),
        not_holder(&a)
    );
}

#[test]
fn holder_only_never_takes() {
    let auth = Authoring::new(false);
    let events = recorder(&auth);
    let a = browser();
    assert_eq!(
        auth.require(&a.id, &a.public, Take::HolderOnly, false),
        Err(Refusal::NoHolder)
    );
    assert_eq!(auth.holder(), None);
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn release_if_frees_only_the_named_holder() {
    let auth = Authoring::new(false);
    let (a, b) = (browser(), browser());
    away_holder(&auth, &a);
    auth.tick(t0() + GRACE);
    take(&auth, &b).unwrap();
    let ended = FreeReason::Released("session ended");
    assert!(!auth.release_if(&a.public, ended));
    assert!(auth.is_holder(&b.id));
    let events = recorder(&auth);
    assert!(auth.release_if(&b.public, ended));
    assert_eq!(auth.holder(), None);
    assert_eq!(
        *events.lock().unwrap(),
        vec![AuthoringEvent::Freed { reason: ended }]
    );
    assert!(!auth.release_if(&b.public, ended));
}

#[test]
fn release_frees_any_holder_and_is_silent_when_free() {
    let auth = Authoring::new(false);
    let events = recorder(&auth);
    auth.release(FreeReason::Released("stop"));
    assert!(events.lock().unwrap().is_empty());
    let a = browser();
    take(&auth, &a).unwrap();
    auth.release(FreeReason::Released("stop"));
    assert_eq!(auth.holder(), None);
    assert_eq!(
        events.lock().unwrap().last(),
        Some(&AuthoringEvent::Freed {
            reason: FreeReason::Released("stop")
        })
    );
}

#[test]
fn subscriber_may_read_the_view_from_its_callback() {
    let auth = Arc::new(Authoring::new(false));
    let presence = Arc::new(Presence::new());
    let a = browser();
    presence.set_name(&a.id, "Dana");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (inner, names, sink) = (Arc::clone(&auth), Arc::clone(&presence), Arc::clone(&seen));
    auth.subscribe(Box::new(move |_| {
        sink.lock().unwrap().push(inner.view(&names))
    }));
    away_holder(&auth, &a);
    let views = seen.lock().unwrap();
    assert_eq!(views.len(), 2, "Taken then Away");
    let holder = views[1].holder.as_ref().expect("held");
    assert_eq!(holder.name, "Dana");
    assert_eq!(holder.away_since.as_deref(), Some("2027-01-15T08:00:00Z"));
    assert_eq!(holder.frees_at.as_deref(), Some("2027-01-15T08:10:00Z"));
}

#[test]
fn view_is_empty_while_nobody_holds() {
    let auth = Authoring::new(false);
    assert_eq!(auth.view(&Presence::new()).holder, None);
}
