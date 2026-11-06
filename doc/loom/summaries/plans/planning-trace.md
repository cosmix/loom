---
kind: document
of: doc/plans/PLAN-planning-trace.md
file: sha256:8fa8ba1a1fee6cf98d3588356b6b468d9993afcccbc74d703e96df6c5fec5eb6
cites:
  "doc/plans/PLAN-planning-trace.md#1-trace-core": "sha256:d1a7470f2a0205d81a9f0d2511ade1c4457cf3951aa6ad3e9eb3cfaea8e84e62"
  "doc/plans/PLAN-planning-trace.md#2-bundle-summaries": "sha256:b2c7796837ab70fab997a464743eab4ece81ee39047bca1c27ba418363a51bcc"
  "doc/plans/PLAN-planning-trace.md#3-trace-server": "sha256:d2231482d7db98f8267f4d3bb32713cf20f4eb639c194db816afa9677a38daff"
  "doc/plans/PLAN-planning-trace.md#4-trace-web-documents": "sha256:a2746a8e09f42eefc2861cf7d1e760b37a6b314aa35f28df44230f4f0d901b9a"
  "doc/plans/PLAN-planning-trace.md#5-trace-web-maps": "sha256:d57dd3ac1535234cc6de2d703a12b36dce981a8bf7e503d515f0d93adf458615"
  "doc/plans/PLAN-planning-trace.md#after-the-run-operator-checks": "sha256:89f936a782aab043d69902f3f82216c3040407db18cdaecef1c6f454ecffa576"
  "doc/plans/PLAN-planning-trace.md#before-loom-run": "sha256:38a168b4c08bec7a268e566697c0622ed847858b09b7d9985926d8a4ef0b2e67"
  "doc/plans/PLAN-planning-trace.md#environment-inventory": "sha256:a89c3796dccc568b7e0b31d8aaa1936e96bb1b006777657ac4e18a6d5f72ebff"
  "doc/plans/PLAN-planning-trace.md#execution-diagram": "sha256:fee19cf9f33d04663f8145228886c330f1c42aa163047c1f2056ac1fdfc51297"
  "doc/plans/PLAN-planning-trace.md#goals": "sha256:621715255cabf56e8bac0a431376e601f53e0fd54253befa715caf9942fdea3e"
  "doc/plans/PLAN-planning-trace.md#integration-verification": "sha256:ff3f18016456ef52b0c2aa9519589fa66fdba12e5d0edfe237d877169ef7d330"
  "doc/plans/PLAN-planning-trace.md#knowledge-distillation": "sha256:86c0d4f9b9c614e45dbff20402a45afae77103dcf1093ecadbf48425c3d5fa7f"
  "doc/plans/PLAN-planning-trace.md#overview": "sha256:5d125fd88bfdb376837ca3b64a34b5e0b66d8e8247dd749610ea8dcf8c9284ad"
  "doc/plans/PLAN-planning-trace.md#paths-and-gates": "sha256:108b51ea5e78a159817e71e6c6cb26a4e8dd51d66dd2fc4205cef223cdd5337d"
  "doc/plans/PLAN-planning-trace.md#requirement-to-gate-map": "sha256:fb69b325ca3b554b8dd8941c22321f8f16c711260635eecd9d348b8200efd0fc"
  "doc/plans/PLAN-planning-trace.md#sibling-plans": "sha256:7cd3aa0771d6f82afb65cd5d429e1cd06cd485f12193e1ffa498011d34f541da"
  "doc/plans/PLAN-planning-trace.md#stages": "sha256:0d38c614ea6b9e8c57ecce869a0951b76e5dbb2ea24a8ffc89821833a679171d"
  "doc/plans/PLAN-planning-trace.md#user-decisions-2026-11-05-do-not-reopen": "sha256:62e7233bb2751cad5352dc956925a649cbd4960f1c38e9abdf97939aff11dd68"
---

# Planning trace, settled documents, brief summaries and the findings loop

The planning trace moves the planner from whole-document steps to a per-feature model of headings, edges and gaps. [d](#s-6ea6603f) [d](#s-8b4b755a)

The goals, non-goals and settled user decisions fix what the trace must prove and what stays out of scope. [d](#s-50b362a5) [d](#s-31210108) [d](#s-550fb38c)

Running the plan waits on its predecessor, and every implementation stage passes the same module, ratchet and lint gates. [d](#s-4e926189) [d](#s-daaf1765)

The stages run in a fixed order: the model first, then two branches that run side by side and meet at verification. [d](#s-f742feea)

The model, the brief-aware summaries and the server routes are built first, each on its own files. [d](#s-169c2847) [d](#s-931de725) [d](#s-f8a9c836)

The browser stages draw the trace on existing screens and add the Coverage and Findings pages. [d](#s-ca1a9890) [d](#s-98265e63)

Verification in the running app and knowledge distillation close the plan. [d](#s-0d34b19d)

The plan lists its own limits, so a reader can see which proofs need a person or a host tool. [d](#s-9d93c714)

The planner models each step as a whole document, so it cannot say which spec heading a plan delivers or which documents the user has agreed to. [Overview](../../../plans/PLAN-planning-trace.md#overview)
The trace is a per-feature model of documents, headings and plain-text edge lines that point at headings, kept apart from the source-code graph of loom map. [Overview](../../../plans/PLAN-planning-trace.md#overview)

The goals require every facet of the requirement map to be built and proven, and documents to stay plain Markdown with edges, settle marks and findings as lines in the files. [Goals](../../../plans/PLAN-planning-trace.md#goals)
The non-goals leave out a force-directed graph, accepting a session's edits one piece at a time, migrating existing documents and anything that touches the source-code graph. [Goals](../../../plans/PLAN-planning-trace.md#goals)
The user decisions of 2026-11-05 put stubs before documents, settle at document level with a plan settling alongside its briefs, and keep one findings ledger per feature fed from run reviews. [User decisions (2026-11-05, do not reopen)](../../../plans/PLAN-planning-trace.md#user-decisions-2026-11-05-do-not-reopen)

The plan runs only after PLAN-planner-follow-ups is done, and no other plan that writes the planning, plan server or plan web source may run beside it. [Before loom run](../../../plans/PLAN-planning-trace.md#before-loom-run) [Sibling plans](../../../plans/PLAN-planning-trace.md#sibling-plans)
Each implementation stage runs its own module-filtered tests, the maintainability ratchet, clippy, fmt and rustdoc, and the full suites run once in integration verification. [Paths and gates](../../../plans/PLAN-planning-trace.md#paths-and-gates) [Integration Verification](../../../plans/PLAN-planning-trace.md#integration-verification)

The execution diagram runs trace-core first, then bundle-summaries and trace-server side by side, with the two web stages following trace-server, and both branches meet at integration verification. [Execution Diagram](../../../plans/PLAN-planning-trace.md#execution-diagram) [Stages](../../../plans/PLAN-planning-trace.md#stages)

trace-core adds the trace model, the Settled line, coverage gaps and the findings ledger with its import from run reviews, and it rewrites the product spec to the new truth. [1. trace-core](../../../plans/PLAN-planning-trace.md#1-trace-core)
bundle-summaries lets a plan summary cite and track every file of its plan's brief bundle. [2. bundle-summaries](../../../plans/PLAN-planning-trace.md#2-bundle-summaries)
trace-server adds the trace, settle and findings routes, the Review the feature session and the planning skill updates for link lines. [3. trace-server](../../../plans/PLAN-planning-trace.md#3-trace-server)

trace-web-documents draws settle state, stubs, frontier hints, edge chips and downstream impact on the screens that exist already. [4. trace-web-documents](../../../plans/PLAN-planning-trace.md#4-trace-web-documents)
trace-web-maps adds the Coverage and Findings pages, their spine entries and their help topics. [5. trace-web-maps](../../../plans/PLAN-planning-trace.md#5-trace-web-maps)

Integration verification drives a scratch feature through settling, coverage, findings and review in the browser, and knowledge distillation then records the trace in the knowledge base. [Integration Verification](../../../plans/PLAN-planning-trace.md#integration-verification) [Knowledge Distillation](../../../plans/PLAN-planning-trace.md#knowledge-distillation)

The environment inventory names what a stage cannot check alone, such as a Claude login or tmux, and the operator checks after the run leave those proofs to a person, while the requirement-to-gate map ties each facet to its stage and proof. [Environment inventory](../../../plans/PLAN-planning-trace.md#environment-inventory) [After the run (operator checks)](../../../plans/PLAN-planning-trace.md#after-the-run-operator-checks) [Requirement to gate map](../../../plans/PLAN-planning-trace.md#requirement-to-gate-map)
