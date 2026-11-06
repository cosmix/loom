---
kind: document
of: doc/product/feat-planner/section_02.md
file: sha256:7655d1e862c6190d97d455aa5eced262186043db75a58ca446e46d2fc8d80864
cites:
  "doc/product/feat-planner/section_02.md#access": "sha256:9c2856ed73983bca92b81988da472dbf4cb1c9784fe6f32162c9e2e09a1daed5"
  "doc/product/feat-planner/section_02.md#command-line": "sha256:10526af42dee3edbb90ae07460e5987d2c60ad8d7cb0a94bca7633aef4445868"
  "doc/product/feat-planner/section_02.md#lifetime": "sha256:6fd90dcf2e886fa376becfeb06027c6f17e506849cc46858bb9d48a3d8e93a91"
  "doc/product/feat-planner/section_02.md#missing-prerequisites": "sha256:41e1db5e0a6cab0c2ab70a6dfa327e7366def0c24429781da2adbc0e6fe60927"
  "doc/product/feat-planner/section_02.md#one-server-per-project": "sha256:b3482816ccda70b6f9e47ae912a5a41899c183138f85e6ca46461ff39c584fba"
  "doc/product/feat-planner/section_02.md#opening-the-browser": "sha256:f9f70b3df14b4e5163d8b46ddffc90402d3e6bfa7c12e5e8a47581038aea97a8"
  "doc/product/feat-planner/section_02.md#port": "sha256:d05216b7e0df75b67b9326c017508f7a2b07fa0bf2ff1e52c5451b8d3de53c65"
  "doc/product/feat-planner/section_02.md#project-root": "sha256:dae972863e28e58b896ffae9cc42f1d2161bb0c0d7c53d41658a76d24c43ebdc"
  "doc/product/feat-planner/section_02.md#runtime-state": "sha256:5151951f6085287a6e5fa8a367ede7b98e4177cffba422d97dac5503d7f14c78"
  "doc/product/feat-planner/section_02.md#several-people-one-author": "sha256:ee4602da693393aab0427c4264dffc27a88a2c0949d6048079f00718a935f9e3"
  "doc/product/feat-planner/section_02.md#shared-code-with-loom-status---web": "sha256:cad537821432747a4ee3f9e05bacb02ec574188db5ef8cf30c79a782160614f2"
  "doc/product/feat-planner/section_02.md#without-loomwork": "sha256:ce9af4a1928d56a2e368ffdfbe9c309cceccabd80da65eb52d6ea9a0f571b5c4"
---

# 02 — `loom plan` Command and Server

## Command line

Bare `loom plan` starts the app, and the existing `loom plan verify` keeps its arguments and behaviour. [Command line](../../../../product/feat-planner/section_02.md#command-line)

The `--port` flag binds exactly the given port, and without it the server tries 7272 and moves to the next port only when the address is in use. [Port](../../../../product/feat-planner/section_02.md#port)

The `--host` flag binds an address other than `127.0.0.1`, and `--no-open` prints the URL without opening a browser. [Command line](../../../../product/feat-planner/section_02.md#command-line)

## Project root and servers

`loom plan` works on the repository containing the current directory and exits with an error outside a git repository. [Project root](../../../../product/feat-planner/section_02.md#project-root)

Inside a loom stage worktree it refuses to start and names the main checkout to run it from. [Project root](../../../../product/feat-planner/section_02.md#project-root)

A second `loom plan` on the same project finds the lock held and exits non-zero with the running server's URL. [One server per project](../../../../product/feat-planner/section_02.md#one-server-per-project)

At most one browser holds authoring at a time, every other browser can only read and browse, and an unheld authoring slot goes to whichever browser first starts a chat, sends a message, saves a file, or starts a pressure run. [Several people, one author](../../../../product/feat-planner/section_02.md#several-people-one-author)

## Runtime and access

Runtime state lives in a per-user directory under `~/.loom/plan/`, so the project needs no `.gitignore` entry and no `loom init`. [Runtime state](../../../../product/feat-planner/section_02.md#runtime-state)

Every start mints a token, and the printed and opened URL carries it. [Access](../../../../product/feat-planner/section_02.md#access)

The browser opens with the platform opener unless `--no-open`, an SSH session, or a missing display prevents it. [Opening the browser](../../../../product/feat-planner/section_02.md#opening-the-browser)

Closing every browser tab does not stop the server, which keeps serving after the authoring session ends. [Lifetime](../../../../product/feat-planner/section_02.md#lifetime)

The first Ctrl-C asks running sessions to write their final state and waits for them, and a second Ctrl-C exits at once. [Lifetime](../../../../product/feat-planner/section_02.md#lifetime)

## Dashboard sharing and prerequisites

`loom plan` runs its own server built from the dashboard's shared parts, such as the listener, access, and HTTP modules. [Shared code with `loom status --web`](../../../../product/feat-planner/section_02.md#shared-code-with-loom-status---web)

`loom plan` works in a project that has never run `loom init`, unlike `loom status --web`, which exits before binding without `.loom/work`. [Without `.loom/work`](../../../../product/feat-planner/section_02.md#without-loomwork)

When `tmux`, the `claude` CLI, or Claude Code login is missing, the server starts in read-only mode and no session starts. [Missing prerequisites](../../../../product/feat-planner/section_02.md#missing-prerequisites)
