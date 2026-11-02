//! The watcher's baseline predates the project and the stores, so a document edited after they
//! load and before the watcher polls again still reaches them.

use std::net::SocketAddr;
use std::path::Path;

use super::support::project;
use crate::plan_server::context::BindInfo;
use crate::plan_server::files::texts::TextStore;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::{load, watch};

#[test]
fn an_edit_made_after_the_stores_load_reaches_the_text_store() {
    let dir = project();
    let bind = BindInfo::new(SocketAddr::from(([127, 0, 0, 1], 0)), "t");
    let read_only = ReadOnly::from_problems(Vec::new());
    let (ctx, mut watcher) = load(dir.path().to_path_buf(), read_only, bind).expect("load");
    crate::plan_server::files::install(&ctx);
    let rel = Path::new("doc/product/index.md");
    let edited = "# Demo, edited while the server started\n";
    std::fs::write(dir.path().join(rel), edited).expect("edit the document");

    // One poll sees the change; the next, finding nothing new, delivers it.
    let batch = (0..3)
        .find_map(|_| watcher.poll().expect("walk doc/"))
        .expect("the edit arrives as a batch");
    assert!(
        !batch.initial,
        "the baseline was taken after the stores loaded"
    );
    watch::apply_batch(&ctx, &batch);
    let texts = ctx
        .extensions()
        .get::<TextStore>()
        .expect("file mode installed");
    let current = texts.current(rel).expect("the document has a text");
    assert_eq!(current.text.as_ref(), edited);
    ctx.stop_workers()
        .expect("join the startup validation pass");
}
