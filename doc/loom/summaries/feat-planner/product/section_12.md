---
kind: document
of: doc/product/feat-planner/section_12.md
file: sha256:dd3b397287e372edcd02110c21759d316aa16c072caf3505b24bd1d6513a54ab
cites:
  "doc/product/feat-planner/section_12.md#browser-identity-and-the-authoring-holder": "sha256:60332f5b5be2161884bf11c1f7ff43a0993c27985a0dde72078187262a0fae0c"
  "doc/product/feat-planner/section_12.md#commands": "sha256:90f00e579d2554c0d1327b915ae1681ba3022117dccf130117bcf394e971685b"
  "doc/product/feat-planner/section_12.md#network-and-web-tools": "sha256:f9e4314b8d8cdd0fbfc3e0e538867ed8c4166ed8e8f776151f8dc383a9563293"
  "doc/product/feat-planner/section_12.md#no-way-for-a-session-to-approve-itself": "sha256:9696a473bb43790db2596d0a45dd4527e640380dbee9bc153d8e6918d7c7117d"
  "doc/product/feat-planner/section_12.md#one-token-every-data-route": "sha256:d2571ddc003a4dfabb5a37794e4a814763ed887f023761a50895942e37384ba8"
  "doc/product/feat-planner/section_12.md#remote-binds": "sha256:a484d11b3dd1cb29f9a82571d7b92ada78cb2a54805c451e5ddc7500d90b4139"
  "doc/product/feat-planner/section_12.md#requests-that-change-something": "sha256:356831b3981253ffa5cc900c12541bc2d4f10f6dda8a2bcb4294e2a735d0f766"
  "doc/product/feat-planner/section_12.md#sharing-access": "sha256:c153c0326804729a9c5146403b934c4d3a73111507a72fb6a5706d45ea3d5a60"
  "doc/product/feat-planner/section_12.md#suggestions-and-restores": "sha256:fdecf26346b1b09d8346b6ba2a8f685c0b8998b65539f1083e6e4fd04f39eebd"
  "doc/product/feat-planner/section_12.md#three-layers-enforce-the-table": "sha256:6070c237a48c1b1e0dc79afdd3da5b07773762e23d31711e8b4e85c13a0ea75a"
  "doc/product/feat-planner/section_12.md#token-lifetime": "sha256:dcb6292c4e0e7fa490e61a37cd88539177e0da251438ac3119b68dd69046c362"
  "doc/product/feat-planner/section_12.md#untrusted-content-in-the-browser": "sha256:2991b24c471ac920d34a390fcc743e1218c88f164959abb3e3eacbc3abebdfd9"
  "doc/product/feat-planner/section_12.md#what-a-session-may-read": "sha256:1a15a08c2f5e81494fb98e2d4b2e6dc64929fc719e9999c08ef9899618da7238"
  "doc/product/feat-planner/section_12.md#what-a-session-may-write": "sha256:06c14573b2432aef7b31b5e114ed2570f36d0b845477eded407d750d0e15e042"
---

# 12 — Access and Safety

## Access to the server

Every start mints a fresh 64-character lowercase hex token, and opening the URL with that token sets an access cookie and a browser-id cookie, both `HttpOnly` and `SameSite=Strict`. [One token, every data route](../../../../product/feat-planner/section_12.md#one-token-every-data-route)

The access cookie is required on every API route and WebSocket in every posture, loopback included, and only the embedded static shell is served without it. [One token, every data route](../../../../product/feat-planner/section_12.md#one-token-every-data-route)

A request that changes anything passes the `Host` check, the exact `Origin` check, the access cookie, a JSON content-type check for `POST`, and the role check, in that order. [Requests that change something](../../../../product/feat-planner/section_12.md#requests-that-change-something)

The server sends no CORS headers and has no double-submit CSRF token, because the cookie is required on every write and is `SameSite=Strict`. [Requests that change something](../../../../product/feat-planner/section_12.md#requests-that-change-something)

A non-loopback bind keeps every write lane over plain HTTP behind the same cookie and checks, and it shows a warning that anyone who can see the traffic can read the token. [Remote binds](../../../../product/feat-planner/section_12.md#remote-binds)

The token has no expiry or rotation, so restarting `loom plan` revokes every cookie. [Token lifetime](../../../../product/feat-planner/section_12.md#token-lifetime)

## People and authoring

Everyone with the tokenized link has the same rights, and a separate read-only link was left out. [Sharing access](../../../../product/feat-planner/section_12.md#sharing-access)

The authoring holder is the browser whose browser-id cookie the server recorded when it took authoring, and a read-only browser cannot take authoring while it is held. [Browser identity and the authoring holder](../../../../product/feat-planner/section_12.md#browser-identity-and-the-authoring-holder)

Send to author is accepted only while someone holds authoring, keeps at most 20 pending suggestions in memory, and reaches only the holder's connections. [Suggestions and restores](../../../../product/feat-planner/section_12.md#suggestions-and-restores)

Documents, transcripts, and display names are untrusted: raw HTML never renders, only `http`, `https`, `mailto`, relative and anchor links become links, and the CSP forbids script. [Untrusted content in the browser](../../../../product/feat-planner/section_12.md#untrusted-content-in-the-browser)

## Planning sessions

A planning session may read the whole repository, but not credential files, loom's daemon tokens or terminal history, or any project's planning runtime directory beyond its own drafts and claims. [What a session may read](../../../../product/feat-planner/section_12.md#what-a-session-may-read)

Sessions write under `doc/` outside the locked set, reach `doc/loom/` only through loom's own commands, and write their drafts; code is not writable at all. [What a session may write](../../../../product/feat-planner/section_12.md#what-a-session-may-write)

Three layers enforce the write table: the OS sandbox for Bash, permission rules for `Edit`, and the planning hook for file tools; the hook is the precise layer. [Three layers enforce the table](../../../../product/feat-planner/section_12.md#three-layers-enforce-the-table)

Sandboxed Bash runs without a prompt for the read-only set and allow rules, an ask rule sends a command to the holder, the classifier decides the rest in authoring and pressure sessions, and sandboxed commands get no network. [Commands](../../../../product/feat-planner/section_12.md#commands) [Network and web tools](../../../../product/feat-planner/section_12.md#network-and-web-tools)

A session never answers its own permission request, because answer files, its tmux pane, the server port, and mode changes are all out of its reach. [No way for a session to approve itself](../../../../product/feat-planner/section_12.md#no-way-for-a-session-to-approve-itself)
