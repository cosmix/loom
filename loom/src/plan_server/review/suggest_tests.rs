//! Tests for the suggestion box.
use super::suggest::*;
use serde_json::{json, Value};

fn browser(id: &str, name: &str) -> BrowserRef {
    BrowserRef {
        id: id.to_owned(),
        public_id: format!("public-{name}"),
        name: name.to_owned(),
    }
}

fn setup() -> (SuggestionBox, BrowserRef, BrowserRef) {
    (
        SuggestionBox::new(),
        browser("sender-secret", "Sam"),
        browser("holder-secret", "Dana"),
    )
}

fn passage() -> SuggestedPassage {
    SuggestedPassage {
        path: "doc/product/section_01.md".to_owned(),
        anchor: "goals".to_owned(),
        text: "Keep one author at a time.".to_owned(),
        lines: Some((3, 4)),
        stage: None,
        summary_sentence: None,
    }
}

fn send(
    inbox: &mut SuggestionBox,
    sender: &BrowserRef,
    holder: &BrowserRef,
    note: &str,
) -> Delivery {
    inbox.submit(sender, Some(holder), passage(), note).unwrap()
}

fn answer(
    inbox: &mut SuggestionBox,
    holder: &BrowserRef,
    id: &str,
    action: SuggestionAction,
) -> Delivery {
    inbox.answer(holder, Some(holder), id, action).unwrap()
}

fn assert_no_private_ids(message: &Value) {
    let text = message.to_string();
    assert!(!text.contains("sender-secret"), "sender id leaked: {text}");
    assert!(!text.contains("holder-secret"), "holder id leaked: {text}");
}

#[test]
fn drop_for_holder_change_clears_pending() {
    let (mut inbox, sender, holder) = setup();
    let other = browser("other-secret", "Alex");
    send(&mut inbox, &sender, &holder, "first");
    send(&mut inbox, &other, &holder, "second");

    let deliveries = inbox.drop_for_holder_change();

    assert_eq!(inbox.pending(), 0);
    assert!(inbox.pending_for().is_empty());
    assert!(inbox.drop_for_holder_change().is_empty());
    assert_eq!(deliveries.len(), 2);
    assert_eq!(deliveries[0].to_browser, sender.id);
    assert_eq!(deliveries[1].to_browser, other.id);
    for (delivery, id) in deliveries.iter().zip(["sg-1", "sg-2"]) {
        assert_eq!(delivery.message["type"], "review");
        assert_eq!(delivery.message["kind"], "suggestion_status");
        assert_eq!(delivery.message["id"], id);
        assert_eq!(delivery.message["state"], "dropped");
        assert!(delivery.message["reason"].is_string());
        assert_no_private_ids(&delivery.message);
        assert!(!delivery.message.to_string().contains("other-secret"));
    }
    let next = send(&mut inbox, &sender, &holder, "new holder");
    assert_eq!(next.message["id"], "sg-3");
}

#[test]
fn pending_for_returns_cards_in_order() {
    let (mut inbox, sender, holder) = setup();
    let mut expected = Vec::new();
    for n in 1..=12 {
        let delivery = send(&mut inbox, &sender, &holder, &format!("note {n}"));
        assert_eq!(delivery.to_browser, holder.id);
        assert_eq!(delivery.message["id"], format!("sg-{n}"));
        assert_no_private_ids(&delivery.message);
        expected.push(delivery.message);
    }
    answer(&mut inbox, &holder, "sg-2", SuggestionAction::Attach);
    expected.remove(1);

    let cards = inbox.pending_for();

    assert_eq!(cards, expected);
    assert_eq!(
        inbox.pending_for(),
        expected,
        "replay does not consume cards"
    );
    assert_eq!(inbox.pending(), 11);
    assert_eq!(cards[0]["from"], "Sam");
    assert_eq!(cards[0]["passage"]["path"], "doc/product/section_01.md");
}

#[test]
fn answer_by_non_holder_refused() {
    let (mut inbox, sender, holder) = setup();
    let impostor = BrowserRef {
        id: "impostor".to_owned(),
        ..holder.clone()
    };
    send(&mut inbox, &sender, &holder, "note");
    let before = inbox.pending_for();

    let result = inbox.answer(&impostor, Some(&holder), "sg-1", SuggestionAction::Attach);

    assert_eq!(result, Err(SuggestRefusal::NotHolder));
    assert_eq!(inbox.pending_for(), before);
    let unknown = inbox.answer(&sender, Some(&holder), "missing", SuggestionAction::Dismiss);
    assert_eq!(unknown, Err(SuggestRefusal::NotHolder));
}

#[test]
fn unknown_id_refused() {
    let (mut inbox, sender, holder) = setup();
    send(&mut inbox, &sender, &holder, "note");
    let before = inbox.pending_for();

    let result = inbox.answer(
        &holder,
        Some(&holder),
        "sg-unknown",
        SuggestionAction::Dismiss,
    );

    assert_eq!(result, Err(SuggestRefusal::UnknownId));
    assert_eq!(inbox.pending_for(), before);
}

#[test]
fn bad_path_refused() {
    let (mut inbox, sender, holder) = setup();
    for path in [
        "",
        "plans/PLAN-x.md",
        "/doc/x",
        "doc/",
        "doc/../x",
        "doc/./x",
        "doc//x",
        "doc/a\\b",
        "doc/a\nb",
        "doc//absolute",
    ] {
        let bad = SuggestedPassage {
            path: path.to_owned(),
            ..passage()
        };

        let result = inbox.submit(&sender, Some(&holder), bad, "note");

        assert!(
            matches!(result, Err(SuggestRefusal::BadPath(_))),
            "{path:?}: {result:?}"
        );
        assert_eq!(inbox.pending(), 0);
    }
    let valid = send(&mut inbox, &sender, &holder, "valid");
    assert_eq!(valid.message["id"], "sg-1", "refusals do not consume ids");
}

#[test]
fn holder_answers_route_status_only_to_sender() {
    let (mut inbox, sender, holder) = setup();
    send(&mut inbox, &sender, &holder, "attach");
    send(&mut inbox, &sender, &holder, "dismiss");

    let attached = answer(&mut inbox, &holder, "sg-1", SuggestionAction::Attach);
    let dismissed = answer(&mut inbox, &holder, "sg-2", SuggestionAction::Dismiss);

    assert_eq!(attached.to_browser, sender.id);
    assert_eq!(dismissed.to_browser, sender.id);
    assert_eq!(
        attached.message,
        json!({
            "type": "review", "kind": "suggestion_status", "id": "sg-1", "state": "attached"
        })
    );
    assert_eq!(
        dismissed.message,
        json!({
            "type": "review", "kind": "suggestion_status", "id": "sg-2", "state": "dismissed"
        })
    );
    assert_no_private_ids(&attached.message);
    assert_no_private_ids(&dismissed.message);
    assert_eq!(inbox.pending(), 0);
    let duplicate = inbox.answer(&holder, Some(&holder), "sg-1", SuggestionAction::Attach);
    assert_eq!(duplicate, Err(SuggestRefusal::UnknownId));
}

#[test]
fn no_holder_and_self_submission_refused() {
    let (mut inbox, sender, holder) = setup();

    let no_holder = inbox.submit(&sender, None, passage(), "note");
    let self_submit = inbox.submit(&holder, Some(&holder), passage(), "note");
    let no_holder_answer = inbox.answer(&holder, None, "sg-1", SuggestionAction::Attach);

    assert_eq!(no_holder, Err(SuggestRefusal::NoHolder));
    assert_eq!(self_submit, Err(SuggestRefusal::SenderIsHolder));
    assert_eq!(no_holder_answer, Err(SuggestRefusal::NoHolder));
    assert_eq!(inbox.pending(), 0);
}

#[test]
fn note_limit_counts_unicode_characters() {
    let (mut inbox, sender, holder) = setup();
    let accepted = inbox.submit(&sender, Some(&holder), passage(), &"é".repeat(1000));

    let refused = inbox.submit(&sender, Some(&holder), passage(), &"é".repeat(1001));

    assert!(accepted.is_ok());
    assert_eq!(refused, Err(SuggestRefusal::NoteTooLong));
    assert_eq!(inbox.pending(), 1);
}

#[test]
fn twenty_first_pending_card_refused_until_answered() {
    let (mut inbox, sender, holder) = setup();
    for _ in 0..20 {
        send(&mut inbox, &sender, &holder, "note");
    }

    let refused = inbox.submit(&sender, Some(&holder), passage(), "overflow");

    assert_eq!(refused, Err(SuggestRefusal::TooMany));
    assert_eq!(inbox.pending(), 20);
    answer(&mut inbox, &holder, "sg-1", SuggestionAction::Dismiss);
    let replacement = send(&mut inbox, &sender, &holder, "replacement");
    assert_eq!(replacement.message["id"], "sg-21");
    assert_eq!(inbox.pending(), 20);
}
