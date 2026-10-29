---
kind: document
of: doc/product/section_12.md
file: sha256:81359fbb9ddb87867bf7989eb014498a2b3ad022d4235f12f07314a09faa91be
cites:
  "doc/product/section_12.md#browser-identity-and-the-authoring-holder": "sha256:60332f5b5be2161884bf11c1f7ff43a0993c27985a0dde72078187262a0fae0c"
  "doc/product/section_12.md#display-names": "sha256:f678721bc9b223313378c303546adf00f9b0895025da288c5b8f7203e2318f22"
  "doc/product/section_12.md#network-and-web-tools": "sha256:f9e4314b8d8cdd0fbfc3e0e538867ed8c4166ed8e8f776151f8dc383a9563293"
  "doc/product/section_12.md#no-way-for-a-session-to-approve-itself": "sha256:307d64e5450070978f34913228402f9b4e5c28cdc6fc607ca2f755431901974d"
  "doc/product/section_12.md#one-token-every-data-route": "sha256:d2571ddc003a4dfabb5a37794e4a814763ed887f023761a50895942e37384ba8"
  "doc/product/section_12.md#planning-sessions": "sha256:1a214bde86e51c2df14405db31fc6172ff60251400aad4a3f3905c95403d81a4"
  "doc/product/section_12.md#remote-binds": "sha256:a484d11b3dd1cb29f9a82571d7b92ada78cb2a54805c451e5ddc7500d90b4139"
  "doc/product/section_12.md#requests-that-change-something": "sha256:356831b3981253ffa5cc900c12541bc2d4f10f6dda8a2bcb4294e2a735d0f766"
  "doc/product/section_12.md#sharing-access": "sha256:c153c0326804729a9c5146403b934c4d3a73111507a72fb6a5706d45ea3d5a60"
  "doc/product/section_12.md#suggestions-and-restores": "sha256:fdecf26346b1b09d8346b6ba2a8f685c0b8998b65539f1083e6e4fd04f39eebd"
  "doc/product/section_12.md#three-layers-enforce-the-table": "sha256:025bd5f946abf8bd2d286f018ea3580a72aaafae1ac1e20871843d014000c871"
  "doc/product/section_12.md#token-lifetime": "sha256:dcb6292c4e0e7fa490e61a37cd88539177e0da251438ac3119b68dd69046c362"
  "doc/product/section_12.md#what-a-session-may-read": "sha256:1a15a08c2f5e81494fb98e2d4b2e6dc64929fc719e9999c08ef9899618da7238"
  "doc/product/section_12.md#what-a-session-may-write": "sha256:5f26d18709222e3fa5a32f8d34b378a137673ea97a685e9653ba778603bccb25"
---

# 12 — Access and Safety

## Access to the server

The access cookie is required on every API route and WebSocket in every posture, loopback included, and only the static shell is served without it. [One token, every data route](../../../product/section_12.md#one-token-every-data-route)

A wrong token answers 403, and a token in the query of any other path does nothing. [One token, every data route](../../../product/section_12.md#one-token-every-data-route)

A request that changes anything must pass the Host check, an exact Origin match, the access cookie, a JSON content type for POST, and the role check. [Requests that change something](../../../product/section_12.md#requests-that-change-something)

A remote bind keeps every write lane over plain HTTP behind a startup warning and a banner. [Remote binds](../../../product/section_12.md#remote-binds)

The token lives as long as the process, so restarting loom plan revokes every cookie. [Token lifetime](../../../product/section_12.md#token-lifetime)

## People and authoring

Anyone holding the link has the same rights, and any authenticated browser can copy it from the Share control. [Sharing access](../../../product/section_12.md#sharing-access)

The authoring holder is identified by the browser id cookie, which never leaves the server and the browser that owns it. [Browser identity and the authoring holder](../../../product/section_12.md#browser-identity-and-the-authoring-holder)

Send to author is refused while nobody holds authoring and reaches only the holder's connections. [Suggestions and restores](../../../product/section_12.md#suggestions-and-restores)

Display names are at most 32 characters and render as text. [Display names](../../../product/section_12.md#display-names)

## Planning sessions

Planning sessions run in the main checkout under the user's login, and the strict MCP config keeps project MCP servers from starting. [Planning sessions](../../../product/section_12.md#planning-sessions)

A session may read the whole repository but not credentials, daemon tokens, or the runtime directory beyond its own drafts, claims, and summaries lock. [What a session may read](../../../product/section_12.md#what-a-session-may-read)

A session may write only the document paths in the write table, its drafts, and claims through the summary CLI, and code is never writable. [What a session may write](../../../product/section_12.md#what-a-session-may-write)

Three layers enforce the write table: the OS sandbox, file-tool permission rules, and the planning hook. [Three layers enforce the table](../../../product/section_12.md#three-layers-enforce-the-table)

Sandboxed commands get no network, and WebFetch asks the holder before each fetch. [Network and web tools](../../../product/section_12.md#network-and-web-tools)

A session never approves its own permission requests, because answer files, tmux, and the server port are out of its reach. [No way for a session to approve itself](../../../product/section_12.md#no-way-for-a-session-to-approve-itself)
