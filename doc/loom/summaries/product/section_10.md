---
kind: document
of: doc/product/section_10.md
file: sha256:aac2360e199d700fca51ed37735fc5281903c2e529a56d98fdf23db08a88f418
cites:
  "doc/product/section_10.md#approval": "sha256:369161c5a7f88e23626859e225c45b42ee9409fa6d7ec15e8bb36bfa91f32dc9"
  "doc/product/section_10.md#open-questions-in-documents": "sha256:ede070ca0d5d91f2eabb9c92f435e48b2bc7558e99be4ff67f20ca497c8bfc27"
  "doc/product/section_10.md#out-of-scope-for-this-section": "sha256:a0f11487f91cecebd763e94763a6ecd614ef7efc284c037ac6d49e9cb53f7672"
  "doc/product/section_10.md#read-only-viewers": "sha256:9dc6ddf107b75783bc63f7848263336995a9f4bc4f3edd644baa6c52b9d95598"
  "doc/product/section_10.md#refinement-after-plans-ran": "sha256:6ac9dae34f1eac78ab717a9f2ecad3fb363e776aaf0840f89dda413522d8534f"
  "doc/product/section_10.md#refining-across-product-and-plans": "sha256:87b7c83f3bcfc4960d2407824e5b4d94c8a77356b32ba6529631189b75cfd795"
  "doc/product/section_10.md#seeing-what-a-refinement-changed": "sha256:70a6d1e297b763cf3953693ce0a67e5ab287ccbe39f24897ae3cfe3a23b01851"
  "doc/product/section_10.md#taking-a-passage-into-chat": "sha256:c1a570ac5152b35ef75384c30d8416cb90a27dc83b641c8d11a07121071f0707"
  "doc/product/section_10.md#the-loop-in-one-paragraph": "sha256:0b5811523a152b73604bceba0e991b0e61ffc73f41f3a2dd09537763abaac3b7"
  "doc/product/section_10.md#the-steps": "sha256:1be37fa323aa8255ee103bfcb271632e7744d0bbd7220c5e9e838db1d3f366c6"
  "doc/product/section_10.md#undoing-a-turns-edit": "sha256:b4316a4c295cf13c79f41328aaa5114473182ba4c5808a50e365604187572dc7"
  "doc/product/section_10.md#what-changed-since-you-last-looked": "sha256:d443f227f52edb3c960a17bcbe1bd3569f13711e1ffbfd59886f31f48e4af413"
  "doc/product/section_10.md#what-the-session-receives": "sha256:11a6d8f084b144b6a74101371d7a0a8a8b7a319b00ed94b1087aeb1e711d2c58"
  "doc/product/section_10.md#when-the-summary-is-wrong": "sha256:7fcb4f2687b87d8c5fd886bb63d83b7cb52e052bdee88fee98fd97322902cd20"
---

# Review and Refinement Loop

The loop connects reviewing and authoring: a reader attaches a doubted passage to chat, the session edits the documents, and the reader sees what changed. [The loop in one paragraph](../../../product/section_10.md#the-loop-in-one-paragraph)

A table maps each reviewer step to what the app shows and which section owns it. [The steps](../../../product/section_10.md#the-steps)

Attachments reach the session as a fixed block appended to the user's message, with one entry per passage giving its kind, address and quoted text. [What the session receives](../../../product/section_10.md#what-the-session-receives)

Any number of passages from either mode and several documents can be attached before one message is sent, and attaching never starts a session. [Taking a passage into chat](../../../product/section_10.md#taking-a-passage-into-chat)

When a summary sentence misstates a document that is right, the session leaves the document alone and rebuilds the summary with a note giving what the source says. [When the summary is wrong](../../../product/section_10.md#when-the-summary-is-wrong)

After a turn the reviewer has four views of its effect: the turn footer, the turn diff, the chips on the sent message, and the reading column. [Seeing what a refinement changed](../../../product/section_10.md#seeing-what-a-refinement-changed)

A reviewer rejects an edit in the chat or with Restore, and Restore is refused when the file changed after the turn ended. [Undoing a turn's edit](../../../product/section_10.md#undoing-a-turns-edit)

There is no approve button and no reviewed flag, so a team records sign-off through a pull request review of the committed doc tree. [Approval](../../../product/section_10.md#approval)

Locked plans stay read-only in a refinement, so a change to a done or running plan goes into a new plan with its own slug. [Refinement after plans ran](../../../product/section_10.md#refinement-after-plans-ran)

Product sections and plans are linked only by Implements lines, and the app never edits a plan on its own. [Refining across product and plans](../../../product/section_10.md#refining-across-product-and-plans)

A read-only browser can send a passage and a note to the author, and nothing reaches the session until the holder sends it. [Read-only viewers](../../../product/section_10.md#read-only-viewers)

Sessions write unresolved points under an Open questions heading, and its count shows on the document's summary page and file tree entry. [Open questions in documents](../../../product/section_10.md#open-questions-in-documents)

Each browser keeps its own checkpoint in local storage, and the changed-since list compares current documents with it or with a chosen git ref. [What changed since you last looked](../../../product/section_10.md#what-changed-since-you-last-looked)

Pressure testing, starting a run and enforcing locked paths belong to other sections. [Out of scope for this section](../../../product/section_10.md#out-of-scope-for-this-section)
