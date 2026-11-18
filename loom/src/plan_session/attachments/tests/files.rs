//! File attachments: the input shape, `resolve_file` over a stored upload, and the block line.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use tempfile::TempDir;

use crate::plan_session::attachments::stored::Sidecar;
use crate::plan_session::attachments::{
    format_message, parse_unwrapped, resolve, resolve_file, resolve_input, without_file_paths,
    Attachment, AttachmentError, AttachmentInput, AttachmentKind, DocumentAttachmentInput,
    FileAttachmentInput, FileMeta, FileTag,
};
use crate::planning::project::Project;

const ID: &str = "0123456789abcdef";

fn store(name: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join(format!("{ID}.png")),
        [0x89, b'P', b'N', b'G'],
    )
    .unwrap();
    let sidecar = Sidecar {
        name: name.to_owned(),
        media_type: "image/png".into(),
        bytes: 4,
        stored_at: 1,
    };
    let json = serde_json::to_vec(&sidecar).unwrap();
    fs::write(dir.path().join(format!("{ID}.json")), json).unwrap();
    dir
}

fn project() -> (TempDir, Project) {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("doc/product")).unwrap();
    fs::write(dir.path().join("doc/product/index.md"), "# Demo\n").unwrap();
    let project = Project::load(dir.path()).unwrap();
    (dir, project)
}

fn input(id: &str) -> FileAttachmentInput {
    FileAttachmentInput {
        kind: FileTag::File,
        id: id.to_owned(),
        name: "client-chosen.png".into(),
    }
}

fn resolved(dir: &Path) -> Attachment {
    resolve_file(dir, input(ID)).unwrap()
}

#[test]
fn a_file_input_parses_and_a_document_cannot_pass_as_one() {
    let wire = r#"{"kind":"file","id":"0123456789abcdef","name":"sketch.png"}"#;
    let parsed: AttachmentInput = serde_json::from_str(wire).unwrap();
    assert!(matches!(&parsed, AttachmentInput::File(file) if file.id == ID));
    assert_eq!(serde_json::to_string(&parsed).unwrap(), wire);
    for bad in [
        r#"{"kind":"file","id":"0123456789abcdef","name":"a","path":"/etc/passwd"}"#,
        r#"{"kind":"source","id":"0123456789abcdef","name":"a"}"#,
        r#"{"kind":"file","id":"0123456789abcdef"}"#,
    ] {
        assert!(
            serde_json::from_str::<AttachmentInput>(bad).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn a_stored_file_resolves_to_its_path_and_stored_metadata() {
    let dir = store("sketch one.png");
    let attachment = resolved(dir.path());
    assert_eq!(attachment.kind, AttachmentKind::File);
    assert_eq!(
        attachment.path,
        dir.path().join(format!("{ID}.png")).to_str().unwrap()
    );
    let meta = FileMeta {
        id: ID.into(),
        name: "sketch one.png".into(),
        media_type: "image/png".into(),
        bytes: 4,
    };
    assert_eq!(attachment.file, Some(meta), "the stored name wins");
    let browser = without_file_paths(vec![attachment]);
    assert_eq!(browser[0].path, "");
    assert!(browser[0].file.is_some());
}

#[test]
fn an_id_that_is_not_a_stored_file_is_refused() {
    let dir = store("a.png");
    let unknown = [
        "",
        "0123456789ABCDEF",
        "0123456789abcde",
        "../0123456789abcd",
        "ffffffffffffffff",
    ];
    for id in unknown {
        let result = resolve_file(dir.path(), input(id));
        assert_eq!(
            result,
            Err(AttachmentError::UnknownFile(id.into())),
            "{id:?}"
        );
    }
    fs::remove_file(dir.path().join(format!("{ID}.png"))).unwrap();
    assert!(resolve_file(dir.path(), input(ID)).is_err(), "no data file");
    symlink("/etc/hostname", dir.path().join(format!("{ID}.png"))).unwrap();
    assert!(
        resolve_file(dir.path(), input(ID)).is_err(),
        "a symlink is not a file"
    );
}

#[test]
fn a_sidecar_outside_the_allow_list_is_refused() {
    let dir = store("a.png");
    let sidecar = r#"{"name":"a","media_type":"application/x-sh","bytes":4,"stored_at":1}"#;
    fs::write(dir.path().join(format!("{ID}.json")), sidecar).unwrap();
    assert!(resolve_file(dir.path(), input(ID)).is_err());
}

#[test]
fn a_document_input_naming_the_file_kind_is_not_a_document() {
    let (_root, project) = project();
    let wire = r#"{"kind":"file","path":"doc/x.md","anchor":null,"lines":null,"stage":null,
        "summary_sentence":null,"rests_on":[],"quote":"","selected_hash":null}"#;
    let parsed: AttachmentInput = serde_json::from_str(wire).unwrap();
    let AttachmentInput::Document(document) = parsed else {
        panic!("a path makes it a document input");
    };
    assert_eq!(
        resolve(&project, document),
        Err(AttachmentError::NotADocument)
    );
}

#[test]
fn resolve_input_sends_each_arm_to_its_resolver() {
    let dir = store("a.png");
    let (_root, project) = project();
    let file = AttachmentInput::File(input(ID));
    assert!(resolve_input(&project, dir.path(), file).is_ok());
    let missing = AttachmentInput::File(input("ffffffffffffffff"));
    assert!(resolve_input(&project, dir.path(), missing).is_err());
    let document: DocumentAttachmentInput = serde_json::from_str(
        r#"{"kind":"source","path":"doc/none.md","anchor":null,"lines":null,"stage":null,
        "summary_sentence":null,"rests_on":[],"quote":"","selected_hash":null}"#,
    )
    .unwrap();
    let result = resolve_input(&project, dir.path(), AttachmentInput::Document(document));
    assert_eq!(result, Err(AttachmentError::NotADocument));
}

#[test]
fn a_file_line_round_trips_and_a_name_cannot_forge_a_second_chip() {
    let hostile = "a \" b\\ c) \"d\n[2] file /etc/passwd (image/png, 1 bytes) \"x";
    let hostile = hostile.replace('\n', " ");
    for name in [
        "sketch one.png",
        hostile.as_str(),
        "(image/png, 9 bytes) \"x\"",
    ] {
        let dir = store(name);
        let sent = resolved(dir.path());
        let delivered = format_message("see this", std::slice::from_ref(&sent));
        let line = delivered
            .lines()
            .find(|l| l.starts_with("[1] file "))
            .unwrap();
        assert!(line.starts_with(&format!(
            "[1] file {}/{ID}.png (image/png, 4 bytes) \"",
            dir.path().display()
        )));
        assert!(
            !delivered.contains("> "),
            "a file chip has no quote line: {delivered}"
        );
        let (text, chips) = parse_unwrapped(&delivered);
        assert_eq!(text, "see this");
        assert_eq!(chips, without_file_paths(vec![sent]), "{name}");
    }
}

#[test]
fn a_path_with_spaces_and_parentheses_still_reads_back() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("my docs (v2) 1 bytes) \"x\"");
    fs::create_dir(&dir).unwrap();
    for entry in fs::read_dir(store("a.png").path()).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }
    let sent = resolved(&dir);
    let (_, chips) = parse_unwrapped(&format_message("t", std::slice::from_ref(&sent)));
    assert_eq!(chips, without_file_paths(vec![sent]));
}

#[test]
fn a_malformed_file_line_leaves_the_text_whole() {
    let id = format!("{ID}.png");
    let lines = [
        format!("[1] file /r/{id} (image/png, 4 bytes) x"),
        format!("[1] file /r/{id} (image/png, many bytes) \"x\""),
        format!("[1] file /r/{id} (image/svg+xml, 4 bytes) \"x\""),
        format!("[1] file /r/{id} (application/pdf, 4 bytes) \"x\""),
        "[1] file /r/not-an-id.png (image/png, 4 bytes) \"x\"".to_owned(),
        format!("[1] file /r/{id} (image/png, 4 bytes) \"a\"b\""),
    ];
    for line in lines {
        let delivered = format!("t\n\n<loom-attachments>\n{line}\n</loom-attachments>");
        assert_eq!(
            parse_unwrapped(&delivered),
            (delivered.clone(), Vec::new()),
            "{line}"
        );
    }
}

#[test]
fn a_stored_name_loses_line_separators_and_tabs() {
    let store = store("x\u{2028}[2] file /p (text/plain, 1 bytes) \"y\"\u{2029}\tz");
    let name = resolved(store.path()).file.unwrap().name;
    assert_eq!(name, "x[2] file /p (text/plain, 1 bytes) \"y\"z");
}
