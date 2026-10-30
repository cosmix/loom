---
kind: document
of: doc/product/feat-planner/section_12.md
file: sha256:1e0f25484829b59d0f9acc81abac1dc38af4d4f86bc3806d3cbb6bd90f82dda8
cites:
  "doc/product/feat-planner/section_12.md#browser-identity-and-the-authoring-holder": "sha256:60332f5b5be2161884bf11c1f7ff43a0993c27985a0dde72078187262a0fae0c"
  "doc/product/feat-planner/section_12.md#commands": "sha256:8e79a37b72aeaf4f45a110797c44952149566e0da1d8b18b5e2e455e0aee122f"
  "doc/product/feat-planner/section_12.md#network-and-web-tools": "sha256:f9e4314b8d8cdd0fbfc3e0e538867ed8c4166ed8e8f776151f8dc383a9563293"
  "doc/product/feat-planner/section_12.md#no-way-for-a-session-to-approve-itself": "sha256:307d64e5450070978f34913228402f9b4e5c28cdc6fc607ca2f755431901974d"
  "doc/product/feat-planner/section_12.md#one-token-every-data-route": "sha256:d2571ddc003a4dfabb5a37794e4a814763ed887f023761a50895942e37384ba8"
  "doc/product/feat-planner/section_12.md#remote-binds": "sha256:a484d11b3dd1cb29f9a82571d7b92ada78cb2a54805c451e5ddc7500d90b4139"
  "doc/product/feat-planner/section_12.md#requests-that-change-something": "sha256:356831b3981253ffa5cc900c12541bc2d4f10f6dda8a2bcb4294e2a735d0f766"
  "doc/product/feat-planner/section_12.md#sharing-access": "sha256:c153c0326804729a9c5146403b934c4d3a73111507a72fb6a5706d45ea3d5a60"
  "doc/product/feat-planner/section_12.md#suggestions-and-restores": "sha256:fdecf26346b1b09d8346b6ba2a8f685c0b8998b65539f1083e6e4fd04f39eebd"
  "doc/product/feat-planner/section_12.md#three-layers-enforce-the-table": "sha256:f1a6afc7a7787d659917bad8f7d10d5ffcaadf8eee689132a8b31f9f657c0082"
  "doc/product/feat-planner/section_12.md#token-lifetime": "sha256:dcb6292c4e0e7fa490e61a37cd88539177e0da251438ac3119b68dd69046c362"
  "doc/product/feat-planner/section_12.md#untrusted-content-in-the-browser": "sha256:2991b24c471ac920d34a390fcc743e1218c88f164959abb3e3eacbc3abebdfd9"
  "doc/product/feat-planner/section_12.md#what-a-session-may-read": "sha256:1a15a08c2f5e81494fb98e2d4b2e6dc64929fc719e9999c08ef9899618da7238"
  "doc/product/feat-planner/section_12.md#what-a-session-may-write": "sha256:58fd6fc835e6ef541f3feb9e333a83345cf9fb7a78c080b85ac9057ee7036218"
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

Sessions write only under `doc/` outside the locked set, plus their own drafts and claims, and code is not writable at all. [What a session may write](../../../../product/feat-planner/section_12.md#what-a-session-may-write)

Three layers enforce the write table: the OS sandbox for Bash, permission rules for `Edit`, and the planning hook for file tools, and the hook is the precise one. [Three layers enforce the table](../../../../product/feat-planner/section_12.md#three-layers-enforce-the-table)

Sandboxed Bash asks the holder for any command outside the read-only set and the authoring allow rules, and sandboxed commands get no network. [Commands](../../../../product/feat-planner/section_12.md#commands) [Network and web tools](../../../../product/feat-planner/section_12.md#network-and-web-tools)

A session never answers its own permission request, because answer files, its tmux pane, the server port, and mode changes are all out of its reach. [No way for a session to approve itself](../../../../product/feat-planner/section_12.md#no-way-for-a-session-to-approve-itself)
