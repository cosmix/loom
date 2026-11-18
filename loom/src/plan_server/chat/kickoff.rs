use crate::plan_server::wire::kickoff::{KickoffShape, KickoffView};
use crate::planning::kinds::is_feature_name;

const MAX_BODY_BYTES: usize = 16 * 1024;
const MAX_STRING_CHARS: usize = 500;
const MAX_LIST_ITEMS: usize = 40;

pub(super) fn parse_shape(markdown: &str) -> Option<KickoffShape> {
    let body = last_body(markdown)?;
    if body.len() > MAX_BODY_BYTES {
        return None;
    }
    let shape = serde_json::from_str(body).ok()?;
    Some(sanitize(shape))
}

struct Fence {
    marker: u8,
    width: usize,
    body_start: usize,
    kickoff: bool,
}

fn last_body(markdown: &str) -> Option<&str> {
    let mut open: Option<Fence> = None;
    let mut last = None;
    let mut offset = 0;
    for line in markdown.split_inclusive('\n') {
        if let Some((marker, width, info)) = fence_line(line) {
            if let Some(fence) = &open {
                if marker == fence.marker && width >= fence.width && info.is_empty() {
                    if fence.kickoff {
                        last = Some(&markdown[fence.body_start..offset]);
                    }
                    open = None;
                }
            } else {
                let kickoff = info == "loom-kickoff";
                if kickoff {
                    // An unfinished final block must not revive an earlier proposal.
                    last = None;
                }
                open = Some(Fence {
                    marker,
                    width,
                    body_start: offset + line.len(),
                    kickoff,
                });
            }
        }
        offset += line.len();
    }
    last
}

fn fence_line(line: &str) -> Option<(u8, usize, &str)> {
    let line = line.trim();
    let marker = *line.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let width = line.bytes().take_while(|byte| *byte == marker).count();
    (width >= 3).then(|| (marker, width, line[width..].trim()))
}

fn sanitize(mut shape: KickoffShape) -> KickoffShape {
    shape.name = shape
        .name
        .map(|name| clean_string(&name))
        .filter(|name| is_feature_name(name));
    shape.problem = clean_string(&shape.problem);
    shape.decisions = clean_list(shape.decisions);
    shape.questions = clean_list(shape.questions);
    shape.sections = shape
        .sections
        .into_iter()
        .filter_map(|mut section| {
            section.title = clean_string(&section.title);
            (!section.title.is_empty()).then_some(section)
        })
        .take(MAX_LIST_ITEMS)
        .collect();
    shape
}

fn clean_string(value: &str) -> String {
    value.trim().chars().take(MAX_STRING_CHARS).collect()
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| clean_string(&value))
        .filter(|value| !value.is_empty())
        .take(MAX_LIST_ITEMS)
        .collect()
}

/// Whether `shape` is what [`parse_shape`] could have produced: strings and lists within their
/// limits. A stored draft that is not is left alone rather than shown.
pub(super) fn within_limits(shape: &KickoffShape) -> bool {
    sanitize(shape.clone()) == *shape
}

/// Latest taking-shape state of the running chat.
///
/// `current` holds the agent's shape exactly as parsed, so `observe` can tell a repeated block
/// from a new one. The user names the work in the conversation and the agent writes the name into
/// its block, so the name is only ever the agent's.
#[derive(Default)]
pub(super) struct KickoffTracker {
    session: Option<String>,
    current: Option<KickoffView>,
}

impl KickoffTracker {
    /// Feed an assistant text item, resetting first when its session changes.
    pub(super) fn observe(&mut self, session: &str, markdown: &str) -> Option<KickoffView> {
        if self.session.as_deref() != Some(session) {
            self.session = Some(session.to_owned());
            self.current = None;
        }
        let shape = parse_shape(markdown)?;
        if self
            .current
            .as_ref()
            .is_some_and(|view| view.shape == shape)
        {
            return None;
        }
        let target = self.current.as_ref().and_then(|view| view.target.clone());
        let view = KickoffView {
            session: session.to_owned(),
            shape,
            target,
        };
        self.current = Some(view.clone());
        Some(view)
    }

    /// Record the repository-relative target for the current session's view.
    pub(super) fn confirm(&mut self, session: &str, target: &str) -> Option<KickoffView> {
        let view = self
            .current
            .as_mut()
            .filter(|view| view.session == session)?;
        view.target = Some(target.to_owned());
        Some(view.clone())
    }

    /// Start `session` from an earlier draft's shape.
    pub(super) fn seed(&mut self, session: &str, shape: KickoffShape) -> KickoffView {
        let view = KickoffView {
            session: session.to_owned(),
            shape,
            target: None,
        };
        self.session = Some(session.to_owned());
        self.current = Some(view.clone());
        view
    }

    pub(super) fn view(&self) -> Option<KickoffView> {
        self.current.clone()
    }

    pub(super) fn clear(&mut self, session: &str) {
        if self.session.as_deref() == Some(session) {
            self.session = None;
            self.current = None;
        }
    }
}

#[cfg(test)]
#[path = "kickoff_tests.rs"]
mod tests;
