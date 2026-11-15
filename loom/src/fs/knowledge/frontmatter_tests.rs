use super::*;
use std::fs;

#[test]
fn update_preserves_unknown_yaml_and_body() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("topic.md");
    let body = "# Topic\n\nBody.\n";
    fs::write(
        &path,
        format!("---\nowner:\n  team: docs\nstate: draft\n---\n{body}"),
    )
    .unwrap();

    update_file(&path, |frontmatter| {
        frontmatter.state = Some(LifecycleState::Active);
        Ok(())
    })
    .unwrap();

    let updated = fs::read_to_string(path).unwrap();
    let (raw, _) = split_frontmatter_raw(&updated).unwrap();
    let mapping: Value = crate::yaml::from_str(raw).unwrap();
    assert_eq!(mapping["owner"]["team"], "docs");
    assert_eq!(mapping["state"], "active");
    assert!(updated.ends_with(body));
}

#[test]
fn update_preserves_flow_and_complex_unknown_values() {
    for raw in [
        "{state: draft, owner: docs}\n",
        "state: draft\n'owner: team':\n  nested: [one, two]\nnotes: |\n  first\n  second\n",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("topic.md");
        let body = "# Topic\r\n\r\nBody.\r\n";
        fs::write(&path, format!("---\n{raw}---\n{body}")).unwrap();
        let mut expected: Map<String, Value> = crate::yaml::from_str(raw).unwrap();
        expected.insert("state".into(), Value::String("active".into()));

        update_file(&path, |frontmatter| {
            frontmatter.state = Some(LifecycleState::Active);
            Ok(())
        })
        .unwrap();

        let updated = fs::read_to_string(path).unwrap();
        let (rendered, actual_body) = split_frontmatter_raw(&updated).unwrap();
        let actual: Map<String, Value> = crate::yaml::from_str(rendered).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual_body, body);
    }
}

#[test]
fn unsupported_mapping_is_left_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("topic.md");
    for raw in [
        "state: draft\nowner:\n  ? [complex, key]\n  : value\n",
        "state: draft\nowner: !custom docs\n",
        "state: draft\nowner: [!custom docs]\n",
        "state: draft\nowner: .nan\n",
    ] {
        let original = format!("---\n{raw}---\n# Topic\n");
        fs::write(&path, &original).unwrap();

        assert!(
            update_file(&path, |frontmatter| {
                frontmatter.state = Some(LifecycleState::Active);
                Ok(())
            })
            .is_err(),
            "accepted {raw}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}

#[test]
fn knowledge_documents_have_closed_frontmatter() {
    let root = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../doc/loom/knowledge"
    ));
    let mut directories = vec![root.to_path_buf()];
    let mut checked = 0;
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                directories.push(path);
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                let text = fs::read_to_string(&path).unwrap();
                if text.lines().next() == Some("---") {
                    let result = split_frontmatter_raw(&text);
                    assert!(result.is_ok(), "{}: {result:?}", path.display());
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 0, "no knowledge frontmatter checked");
}

#[test]
fn update_uses_shared_delimiters_and_preserves_body() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("topic.md");
    let body = "# Topic\r\n\r\nBody.\r\n";
    fs::write(
        &path,
        format!("--- \r\nowner: docs\r\nstate: draft\r\n---\t\r\n{body}"),
    )
    .unwrap();

    update_file(&path, |frontmatter| {
        frontmatter.state = Some(LifecycleState::Active);
        Ok(())
    })
    .unwrap();

    let updated = fs::read_to_string(path).unwrap();
    let (raw, actual_body) = split_frontmatter_raw(&updated).unwrap();
    let mapping: Value = crate::yaml::from_str(raw).unwrap();
    assert_eq!(mapping["owner"], "docs");
    assert_eq!(mapping["state"], "active");
    assert_eq!(actual_body, body);
}
