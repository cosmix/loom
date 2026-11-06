---
kind: document
of: doc/product/feat-planner/section_10.md
file: sha256:4ffd03f8481e7308e08341adbe323404f246067eb8be44c79d36982c551fb184
cites:
  "doc/product/feat-planner/section_10.md#approval": "sha256:d16db62adf7bf6056fa9a9734cf36affa85aacf0cbb7a90f1aedbd5ce8d3fbe9"
  "doc/product/feat-planner/section_10.md#open-questions-in-documents": "sha256:c8f39019873a5dcebe75c8f09258206df82df61747fbe1d050ab8bf490a26ef6"
  "doc/product/feat-planner/section_10.md#read-only-viewers": "sha256:9dc6ddf107b75783bc63f7848263336995a9f4bc4f3edd644baa6c52b9d95598"
  "doc/product/feat-planner/section_10.md#refinement-after-plans-ran": "sha256:c2813f45ef4fbdaa34bd74d68d26edf7ede08c23800ae90ecec29faf44ffd6c5"
  "doc/product/feat-planner/section_10.md#refining-across-product-and-plans": "sha256:87b7c83f3bcfc4960d2407824e5b4d94c8a77356b32ba6529631189b75cfd795"
  "doc/product/feat-planner/section_10.md#seeing-what-a-refinement-changed": "sha256:70a6d1e297b763cf3953693ce0a67e5ab287ccbe39f24897ae3cfe3a23b01851"
  "doc/product/feat-planner/section_10.md#the-loop-in-one-paragraph": "sha256:0b5811523a152b73604bceba0e991b0e61ffc73f41f3a2dd09537763abaac3b7"
  "doc/product/feat-planner/section_10.md#undoing-a-turns-edit": "sha256:b4316a4c295cf13c79f41328aaa5114473182ba4c5808a50e365604187572dc7"
  "doc/product/feat-planner/section_10.md#what-changed-since-you-last-looked": "sha256:045061d40922e68956f91f53d5ec9151f6f101b06f56d120eec001b134c73baf"
  "doc/product/feat-planner/section_10.md#what-the-session-receives": "sha256:11a6d8f084b144b6a74101371d7a0a8a8b7a319b00ed94b1087aeb1e711d2c58"
  "doc/product/feat-planner/section_10.md#when-the-summary-is-wrong": "sha256:7fcb4f2687b87d8c5fd886bb63d83b7cb52e052bdee88fee98fd97322902cd20"
---

# Review and Refinement Loop

## The loop in one paragraph

A reviewer attaches a doubtful passage with its exact source to a chat message, and the session either answers or edits the documents. [The loop](../../../../product/feat-planner/section_10.md#the-loop-in-one-paragraph)

## What the session receives

The server appends one fixed block to the user's message, listing each attachment's kind, address, and fields such as lines, rests-on, lock state, and whether the document changed after selection. [What the session receives](../../../../product/feat-planner/section_10.md#what-the-session-receives)

## When the summary is wrong

When a summary sentence misstates a right document, the session leaves the document alone and rebuilds that summary with a note giving the reason. [When the summary is wrong](../../../../product/feat-planner/section_10.md#when-the-summary-is-wrong)

A reviewer who presses rebuild on a summary gives no note, so that rebuild fixes a wrong sentence only by chance. [When the summary is wrong](../../../../product/feat-planner/section_10.md#when-the-summary-is-wrong)

## Seeing what a refinement changed

The turn footer lists every changed file and every rebuilt summary, and each attached chip says whether its heading changed in that turn. [Seeing what a refinement changed](../../../../product/feat-planner/section_10.md#seeing-what-a-refinement-changed)

## Undoing a turn's edit

A reviewer rejects an edit by asking the session in chat or by restoring the file to its text from before the turn, which costs no tokens. [Undoing a turn's edit](../../../../product/feat-planner/section_10.md#undoing-a-turns-edit)

Restore is refused when the file changed after the turn ended, and it is offered only while no turn is running. [Undoing a turn's edit](../../../../product/feat-planner/section_10.md#undoing-a-turns-edit)

## Approval

The app records no approval, because reviewing is the user reading, which has no step in the workflow, and a team's sign-off uses a pull request review of the committed doc tree. [Approval](../../../../product/feat-planner/section_10.md#approval)

## Refinement after plans ran

Locked plans stay read-only in a refinement, so a change to done or running work becomes a new plan with its own slug, Implements line, and entry in a metaplan. [Refinement after plans ran](../../../../product/feat-planner/section_10.md#refinement-after-plans-ran)
A new plan for running work runs after the running one, and correcting a running stage's verification is the exception, done with `loom stage amend` in a terminal outside the app. [Refinement after plans ran](../../../../product/feat-planner/section_10.md#refinement-after-plans-ran)

## Refining across product and plans

The turn footer lists the plans that may need updating under each changed product section, because only Implements lines link product sections to plans. [Refining across product and plans](../../../../product/feat-planner/section_10.md#refining-across-product-and-plans)

## Read-only viewers

A read-only browser can send a passage and a note to the holder, and nothing reaches the session unless the holder sends it. [Read-only viewers](../../../../product/feat-planner/section_10.md#read-only-viewers)

## Open questions in documents

Sessions write unresolved points under a heading named `Open questions`, and the open question count appears in the checklist beside each product section and on the summary page header and file mode tree entry of any document with that heading. [Open questions in documents](../../../../product/feat-planner/section_10.md#open-questions-in-documents)

## What changed since you last looked

Each browser keeps in local storage, per project, a checkpoint holding the commit HEAD pointed to and the content hash of every document, and the changed-documents list compares current hashes with that checkpoint or with a git ref the reviewer types. [What changed since you last looked](../../../../product/feat-planner/section_10.md#what-changed-since-you-last-looked)

The browser also keeps the text it last showed of each document, which is the before text of that document's diff while its hash matches the checkpoint. [What changed since you last looked](../../../../product/feat-planner/section_10.md#what-changed-since-you-last-looked)
