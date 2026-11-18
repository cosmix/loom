//! File attachments through chat dispatch: delivery by path, the browser's chip without it,
//! refusal of an id that is not stored, and the shared cap.

use std::fs;

use super::super::tests::{browser, ok_result};
use super::{attachment, opening, refused, send, Fixture};
use crate::plan_server::wire::chat::ChatCommand;
use crate::plan_session::attachments::stored::Sidecar;
use crate::plan_session::attachments::{
    AttachmentError, AttachmentInput, AttachmentKind, FileAttachmentInput, FileTag,
};
use crate::plan_session::input::InputRefusal;
use crate::plan_session::model::Item;

pub(super) const ID: &str = "0123456789abcdef";

pub(super) fn file_input(id: &str) -> AttachmentInput {
    AttachmentInput::File(FileAttachmentInput {
        kind: FileTag::File,
        id: id.into(),
        name: "client-name.png".into(),
    })
}

/// Store a 4-byte PNG named `sketch.png` under `ID`.
pub(super) fn store_file(f: &Fixture) -> String {
    let dir = &f.ctx.runtime().attachments;
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(format!("{ID}.png")), [0x89, b'P', b'N', b'G']).unwrap();
    let sidecar = Sidecar {
        name: "sketch.png".into(),
        media_type: "image/png".into(),
        bytes: 4,
        stored_at: 1,
    };
    fs::write(
        dir.join(format!("{ID}.json")),
        serde_json::to_vec(&sidecar).unwrap(),
    )
    .unwrap();
    dir.join(format!("{ID}.png")).to_str().unwrap().to_owned()
}

#[test]
fn a_file_is_delivered_by_path_and_the_chip_carries_no_path() {
    let f = super::fixture(false);
    let tab = f.connect(&browser());
    let path = store_file(&f);
    let cmd = ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: None,
        feature: None,
        plan: None,
        text: Some("Look at this".into()),
        attachments: vec![file_input(ID)],
    };

    assert_eq!(f.command(&tab, cmd), ok_result());
    let id = super::wait_for("the typed chat", || f.bridge.chat_session_id());
    let s = f.launched(&id);
    let item = opening(&f, &s);

    let expected = format!(
        "Look at this\n\n<loom-attachments>\n[1] file {path} (image/png, 4 bytes) \
         \"sketch.png\"\n</loom-attachments>"
    );
    assert_eq!(f.host.delivered_to(&s.socket), vec![expected]);
    let Item::User { attachments, .. } = item else {
        panic!("not a user item: {item:?}");
    };
    assert_eq!(attachments.len(), 1);
    let chip = &attachments[0];
    assert_eq!(
        (chip.kind.clone(), chip.path.as_str()),
        (AttachmentKind::File, "")
    );
    let file = chip.file.as_ref().expect("a file chip carries its file");
    assert_eq!(
        (file.id.as_str(), file.name.as_str(), file.bytes),
        (ID, "sketch.png", 4)
    );
}

#[test]
fn an_id_that_is_not_stored_is_refused_and_starts_nothing() {
    let f = super::fixture(false);
    let tab = f.connect(&browser());
    store_file(&f);
    for id in ["ffffffffffffffff", "../../etc/passwd", "0123456789ABCDEF"] {
        let result = f.command(&tab, send("look", vec![file_input(id)]));
        assert_eq!(
            result,
            refused(AttachmentError::UnknownFile(id.into())),
            "{id}"
        );
    }
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn files_count_toward_the_ten_attachment_cap() {
    let f = super::fixture(false);
    let tab = f.connect(&browser());
    store_file(&f);
    let mut inputs = vec![file_input(ID); 5];
    inputs.extend(vec![attachment("doc/product/demo/index.md"); 6]);

    let result = f.command(&tab, send("look", inputs));

    assert_eq!(result, refused(InputRefusal::TooManyAttachments));
    assert!(f.bridge.chat_session_id().is_none());
}
