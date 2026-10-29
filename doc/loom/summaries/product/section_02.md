---
kind: document
of: doc/product/section_02.md
file: sha256:cab198a11c48ed127a272cd19baa760de5136375345747eed8c9873cf0a9e0c6
cites:
  "doc/product/section_02.md#access": "sha256:9c2856ed73983bca92b81988da472dbf4cb1c9784fe6f32162c9e2e09a1daed5"
  "doc/product/section_02.md#command-line": "sha256:10526af42dee3edbb90ae07460e5987d2c60ad8d7cb0a94bca7633aef4445868"
  "doc/product/section_02.md#lifetime": "sha256:6fd90dcf2e886fa376becfeb06027c6f17e506849cc46858bb9d48a3d8e93a91"
  "doc/product/section_02.md#missing-prerequisites": "sha256:41e1db5e0a6cab0c2ab70a6dfa327e7366def0c24429781da2adbc0e6fe60927"
  "doc/product/section_02.md#one-server-per-project": "sha256:b3482816ccda70b6f9e47ae912a5a41899c183138f85e6ca46461ff39c584fba"
  "doc/product/section_02.md#opening-the-browser": "sha256:f9f70b3df14b4e5163d8b46ddffc90402d3e6bfa7c12e5e8a47581038aea97a8"
  "doc/product/section_02.md#port": "sha256:d05216b7e0df75b67b9326c017508f7a2b07fa0bf2ff1e52c5451b8d3de53c65"
  "doc/product/section_02.md#project-root": "sha256:dae972863e28e58b896ffae9cc42f1d2161bb0c0d7c53d41658a76d24c43ebdc"
  "doc/product/section_02.md#runtime-state": "sha256:5151951f6085287a6e5fa8a367ede7b98e4177cffba422d97dac5503d7f14c78"
  "doc/product/section_02.md#several-people-one-author": "sha256:581db94986d863b0e9b732a6ef619154509b39956ee299ef13cd658a073c60de"
---

# Loom plan command and server

## Command line

Bare `loom plan` starts the app, and the existing `loom plan verify` subcommand keeps its arguments and behaviour. [Command line](../../../product/section_02.md#command-line)

The app's flags `--port`, `--host` and `--no-open` belong to `loom plan` itself, and no config-file keys exist for them. [Command line](../../../product/section_02.md#command-line)

## Port

Bare `loom plan` tries port 7272 and moves to the next port only when the address is in use. [Port](../../../product/section_02.md#port)

An explicit `--port` binds exactly that port and reports the bind error as is. [Port](../../../product/section_02.md#port)

## Project root

`loom plan` works on the repository that contains the current directory and exits with an error outside git. [Project root](../../../product/section_02.md#project-root)

It also refuses to run inside a loom stage worktree and names the main checkout to run it from. [Project root](../../../product/section_02.md#project-root)

## One server per project

A project has at most one `loom plan` server, held by an exclusive OS file lock for the life of the process. [One server per project](../../../product/section_02.md#one-server-per-project)

## Several people, one author

Any number of people can open the server's URL, but at most one browser holds authoring at a time. [Several people, one author](../../../product/section_02.md#several-people-one-author)

## Runtime state

`loom plan` writes nothing into the project except documents under `doc/`, and its runtime state lives in a per-user directory. [Runtime state](../../../product/section_02.md#runtime-state)

## Opening the browser

After the server binds, `loom plan` prints the URL and opens it unless `--no-open`, an SSH session, or a missing display applies. [Opening the browser](../../../product/section_02.md#opening-the-browser)

## Access

Every start mints a token, and every write lane into the repository needs the cookie, even on loopback. [Access](../../../product/section_02.md#access)

## Lifetime

Closing every browser tab does not stop the server, and the first Ctrl-C asks running sessions to write their final state. [Lifetime](../../../product/section_02.md#lifetime)

## Missing prerequisites

`loom plan` starts in read-only mode when `tmux`, the `claude` CLI, a Claude Code login, or the sandbox prerequisites are missing. [Missing prerequisites](../../../product/section_02.md#missing-prerequisites)
