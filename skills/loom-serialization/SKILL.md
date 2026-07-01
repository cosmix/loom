---
name: loom-serialization
description: Data serialization and deserialization patterns across formats. Use when implementing data exchange, API payloads, storage formats, encoding/decoding, schema evolution, or cross-language communication with JSON, YAML, TOML, Protocol Buffers, MessagePack, CBOR, Avro, or serde.
triggers:
  - serialize
  - deserialize
  - serialization
  - deserialization
  - JSON
  - YAML
  - TOML
  - XML
  - Protocol Buffers
  - protobuf
  - MessagePack
  - CBOR
  - Avro
  - serde
  - encoding
  - decoding
  - schema
  - schema evolution
  - versioning
  - backward compatibility
  - forward compatibility
  - binary format
  - text format
  - data interchange
  - gRPC
  - API contracts
  - canonical serialization
  - deterministic serialization
---

# Serialization

## Overview

Converting in-memory data to bytes for storage/transport and back. The hard problems are not encode/decode calls — they are **schema evolution** (old and new code exchanging data safely), **format choice** (self-describing vs schema'd, text vs binary), and **correctness traps** (int64 precision, NaN, non-deterministic output, unknown-field handling). This skill is mechanism-level; assume you can call the library.

## Format Selection

| Format | Schema | Self-describing | Cross-lang | Notes / when to reach for it |
| --- | --- | --- | --- | --- |
| JSON | none | yes | yes | Debuggable, ubiquitous. No int64/binary/date types; slow; large. Default for public HTTP APIs. |
| Protobuf | IDL (`.proto`) | no (tag numbers only) | excellent | Compact, tag-based evolution, gRPC. Needs `.proto` to read bytes. |
| Avro | required (writer+reader) | schema travels or via registry | good | No per-field tags/names in payload → smallest tagged binary; schema-resolution evolution. Kafka/Hadoop. |
| MessagePack | none | yes | yes | "binary JSON": same data model, ~2x smaller/faster. Dynamic data without a schema. |
| CBOR (RFC 8949) | none | yes | yes | IETF-standard MessagePack cousin; has a **canonical** form + tags. COSE/WebAuthn/DTLS. |
| FlatBuffers / Cap'n Proto | IDL | no | good | Zero-copy: read fields without parsing; mmap-able. Games, low-latency IPC. |
| bincode / postcard | Rust type layout | no | no (Rust↔Rust) | Fastest+smallest for Rust-only. **No evolution** — field order/type is the contract. |
| TOML / YAML | none | yes | yes | Config, human-authored. Not for hot-path data (YAML esp. slow + footguns). |

Decision heuristics: **public API / must be human-readable → JSON**; **internal RPC, polyglot, evolving → protobuf (gRPC)**; **event stream with a registry → Avro**; **Rust-to-Rust cache/IPC, no evolution → bincode/postcard**; **need to read one field of a huge blob → FlatBuffers**; **cryptographic canonicalization → CBOR canonical or JCS**.

## Schema Evolution & Compatibility

The discipline that lets producers and consumers deploy independently. Precise definitions:

- **Backward compatible** = *new* reader can read data written by *old* writer. (You upgraded the consumer.)
- **Forward compatible** = *old* reader can read data written by *new* writer. (You upgraded the producer first; consumers lag.) Requires readers to **ignore unknown fields**.
- **Full** = both. Aim for full on anything with independent deploy cadence (events, shared queues, mobile clients you can't force-update).

| Change | Backward | Forward | Notes |
| --- | --- | --- | --- |
| Add optional field w/ default | ✅ | ✅ | The safe change. Old reader ignores it; new reader defaults it. |
| Remove optional field | ✅ | ✅ | Reserve its tag/name (protobuf). |
| Add enum value | ✅ | ⚠️ | Old reader must tolerate unknown → open enums (proto3) or a catch-all case. |
| Widen int (int32→int64) | ✅ | ⚠️ | Forward-breaks if new values exceed old type's range. |
| Rename field (JSON) | ❌ | ❌ | JSON keys by name; add-new + dual-write + drop-old instead. |
| Change field type / tag number | ❌ | ❌ | Wire garbage or silent misparse. New field number instead. |
| optional→required | ❌ | — | Never in proto3 (`required` doesn't exist — by design). |

**Expand/contract (parallel change) is the only safe rename/retype:** (1) add the new field, (2) write both old+new, (3) migrate readers to new, (4) stop writing old, (5) remove + reserve old. Same pattern as DB migrations.

## Protocol Buffers

### Field numbers are the permanent, load-bearing identity

The `.proto` field *name* is cosmetic; the **tag number** is what's on the wire. Rules that bite hard if violated:

- **Never reuse a field number.** A recycled number makes new readers misinterpret old bytes as the new field — silent corruption, no error. When deleting, `reserved` the number **and** the old name so nobody re-adds them.
- Numbers **1–15** use a 1-byte tag; **16–2047** use 2 bytes. Put hot/repeated fields in 1–15.
- Compatible type swaps (same wire type) are safe: `int32`/`int64`/`uint32`/`uint64`/`bool`/enum are all varint and interchangeable *with sign caveats* (negative `int32` is 10 bytes; range overflow silently truncates). `sint32/64` (zigzag) and `fixed32/64` are **different wire types** — not interchangeable with `int*`. `string`↔`bytes` compatible when bytes are valid UTF-8.

```protobuf
syntax = "proto3";
package users.v1;                 // version in the package, not field names

message User {
  string id = 1;                  // 1–15: 1-byte tags for hot fields
  string email = 2;
  optional string phone = 4;      // `optional` = explicit presence (see below)
  repeated string roles = 5;      // packed by default in proto3
  map<string, string> metadata = 6;
  google.protobuf.Timestamp created_at = 7;

  reserved 3, 50 to 60;           // retired numbers — never reuse
  reserved "legacy_name";         // retired name
}
```

### proto3 presence, enums, unknown fields

- **Implicit presence (default proto3):** a scalar at its zero value (`0`, `""`, `false`) is indistinguishable from unset and is *not serialized*. If you must tell "0" from "absent" (PATCH semantics, tri-state), mark the field `optional` (explicit presence, since protoc 3.15) or wrap in `google.protobuf.Int32Value`. Message-typed fields always have presence.
- **Open enums:** proto3 enums are open — an unknown numeric value round-trips as the raw int rather than erroring. The zero value **must** be `*_UNSPECIFIED` (safe default + forward-compat sentinel). Never renumber existing values.
- **Unknown fields are preserved on round-trip** in modern protobuf (proto3 dropped this in 3.0–3.4, restored in 3.5 / 2017). This is what makes a proxy that decodes→re-encodes forward-compatible. ⚠️ **Protobuf JSON mapping drops unknown fields by default** — a JSON gateway is *not* forward-compatible the way the binary form is.

### Wire-format gotchas

- Serialization is **not canonical**: map entry order is unspecified, unknown fields are appended, and repeated `SetSerializationDeterministic` is per-process best-effort, not a cross-language guarantee. **Do not hash/sign raw protobuf bytes** expecting stability. Sign the exact received bytes, or canonicalize explicitly.
- `required` (proto2 only) is a permanent trap: you can never safely remove it (old readers reject messages missing it) — proto3 removed it deliberately.
- Large `repeated`/`map` have no built-in size cap; set decoder recursion/size limits to avoid decompression-bomb DoS on untrusted input.

### gRPC & API contracts

- **Partial update (PATCH) needs a `google.protobuf.FieldMask`**, not zero-value sniffing — the mask names exactly which fields to touch, so clearing a value is expressible and unset ≠ "leave alone". Alternatively wrap fields in wrapper types.
- Prefer **well-known types** (`Timestamp`, `Duration`, `Empty`, `Struct`, `Any`, `FieldMask`) over hand-rolled equivalents — they have canonical JSON mappings and cross-language support.
- Choose streaming shape up front (unary / server-stream / client-stream / bidi); it is part of the wire contract and can't be changed compatibly. List responses should carry `next_page_token`, not rely on stream length.
- Version in the **package** (`users.v1`) so a `v2` can coexist; never mutate a shipped message's field meanings.

## JSON: pitfalls experts guard against

- **int64/uint64 lose precision in JavaScript.** JS numbers are IEEE-754 doubles; integers above `2^53−1` (`Number.MAX_SAFE_INTEGER`) round silently. This is why the **protobuf→JSON mapping encodes 64-bit ints as strings**. Transport large IDs/amounts as strings; parse with BigInt.
- **NaN / Infinity are not valid JSON.** `JSON.stringify(NaN)` → `null` (silent data loss); most strict parsers reject `Infinity`. Guard float fields before encoding.
- **No canonical form.** Key order, whitespace, number formatting (`1e2` vs `100`, `-0`, trailing zeros), and Unicode escaping all vary → re-serialized JSON ≠ byte-identical. Never compare/sign JSON by re-encoding (see canonical section).
- **Duplicate keys are undefined.** Spec allows them; most parsers keep last-wins, some first-wins → a smuggling vector across two services using different parsers. Reject duplicates on security boundaries.
- **Missing types:** no date (use ISO-8601 strings), no binary (base64), no integer/float distinction, no comments/trailing commas. Parsing `2024-12-19` back to a Date is your job.
- **null vs undefined vs omitted** are three distinct states. Decide per field: omit unknown-but-optional (`skip_serializing_if`), send `null` for known-absent. Emitting `undefined` in JS just omits the key — align both ends or PATCH semantics break.

## Canonical / Deterministic Serialization

Needed whenever bytes are **hashed, signed, deduplicated, or content-addressed**. The rule: *serialize once, then treat the bytes as opaque* — never re-encode and expect equality.

- **JSON:** use **RFC 8785 JCS** (JSON Canonicalization Scheme) — lexicographic key sort, minimal number formatting, fixed Unicode escaping. For signing, prefer signing the raw payload bytes (as JWS/COSE do over the exact octet string) rather than JCS-normalizing, to avoid round-trip drift.
- **CBOR:** has a defined canonical/deterministic encoding (RFC 8949 §4.2) — used by COSE (`cose-sign1`) and WebAuthn. Reach for CBOR over JSON when you need standardized determinism.
- **Protobuf:** *not* canonical (see above). If you must, define an explicit canonical byte layout or hash a canonicalized projection, not the wire bytes.
- **Common bug:** verifying a signature by re-serializing the parsed object. Any field reorder, default-omission, or float reformat breaks it. Keep and verify against the original bytes.

## Rust serde

Primary language of this project. Derive covers the common cases; the value is in the attributes and the enum-representation trade-offs.

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]   // JSON keys: userId, createdAt
struct Order {
    id: String,
    #[serde(default)]                                     // backward-compat: tolerate missing
    #[serde(skip_serializing_if = "Vec::is_empty")]      // omit empty on the wire
    items: Vec<Item>,
    #[serde(with = "time::serde::rfc3339")]               // module supplies ser+de
    created_at: OffsetDateTime,
    #[serde(skip)]                                        // never (de)serialized; needs Default
    cache: Cache,
}
```

Key attributes: `rename` / `rename_all` (`camelCase`,`snake_case`,`SCREAMING_SNAKE_CASE`,`kebab-case`); `default` / `default = "path"`; `skip_serializing_if = "Option::is_none"`; `with`/`serialize_with`/`deserialize_with`; `flatten`; `tag`/`content`/`untagged`; `borrow` (zero-copy `&'a str`); `deny_unknown_fields`; `alias` (accept an old key name on read).

### Enum representations — pick deliberately, each has a trap

```rust
#[serde(tag = "type")]                      // internally tagged: {"type":"Text","content":"hi"}
#[serde(tag = "t", content = "c")]          // adjacently tagged: {"t":"Text","c":"hi"}
#[serde(untagged)]                          // by shape: "hi"  or  {"url":...}
// default (externally tagged):             {"Text":"hi"}
```

- **`untagged` and internally-tagged buffer the whole input** into an intermediate `Content` value: (1) measurably slower, (2) **breaks zero-copy** — `#[serde(borrow)] &'a str` can't borrow from a buffer, forcing owned `String`; (3) untagged tries variants **in declaration order** and reports only a generic "did not match any variant" — order variants specific→general and expect poor errors.
- **Internally tagged can't represent newtype-of-primitive variants** (`Text(String)`) — the tag needs a map/struct to live in; use struct variants (`Text { content: String }`) or adjacent tagging.
- **`flatten` forces a map-based deserialize path.** It silently breaks non-self-describing formats (bincode, most binary), is **incompatible with `deny_unknown_fields`** (flatten needs to see leftover keys), and disables serde's field-count fast path. Prefer explicit fields when the format is binary.
- **`deny_unknown_fields` trades forward-compat for strictness** — safe for internal configs, wrong for evolving external payloads where you *want* to ignore new fields.
- Use `#[serde(other)]` on a unit variant as an enum catch-all (tagged enums) so new server-side variants don't hard-fail old clients.

Non-self-describing formats (bincode/postcard) can't drive internally/adjacently/untagged enums or `flatten`, and have **no schema evolution** — reordering fields or changing a type silently misreads. Use them only for same-version Rust↔Rust.

## MessagePack & CBOR

- Same data model as JSON plus real integers, binary, and (via extensions) custom types. ~2× smaller and faster than JSON; still self-describing (no schema needed).
- **CBOR over MessagePack** when you need an IETF standard, a canonical form (signing), or tag-based extensibility (datetime tag 0/1, bignum). MessagePack when you just want compact JSON with the widest library support.
- Both preserve unknown map keys → naturally forward-compatible for additive changes.
- ⚠️ Untrusted input: set nesting-depth and length limits; a hostile 5-byte header can claim a multi-GB array/map and OOM a naive decoder.

## Performance

- **Cost order (Rust, rough):** bincode/postcard < protobuf ≈ MessagePack/CBOR < JSON ≪ YAML. Binary saves parse time *and* bytes.
- **Amortize setup:** reuse encoders/decoders and, for protobufjs/Ajv-style libs, load/compile the schema once — recompiling per message dominates cost.
- **Stream large payloads** with JSON Lines (`{...}\n{...}`) or length-delimited protobuf instead of one giant document — bounds memory and enables backpressure. Skip malformed JSONL lines rather than failing the whole stream.
- **Compress at the transport, not per-field** (gzip/zstd over the response); mining bytes by hand-shortening keys hurts readability for marginal gain vs a binary format.
- Measure with representative payloads — small-object microbenchmarks mislead; allocation/GC pressure often dominates raw encode time.

## Gotchas (cross-format)

- **Floats are not exact.** `0.1+0.2 ≠ 0.3`; round-tripping money as float drifts. Serialize currency as integer minor units (cents) or a decimal string, never `f64`.
- **Timezones:** always emit UTC ISO-8601 with offset (`...Z`); a bare `2024-12-19T14:30:00` is ambiguous across systems.
- **Endianness / integer width** only matter for hand-rolled binary — every format above handles it, but a custom `DataView`/`byteorder` layout must fix both ends.
- **YAML footguns:** `NO`/`no`/`on`/`off` parse as booleans (Norway problem), `1.0` may become a float, unquoted large numbers overflow. Quote ambiguous scalars; use `safe_load`, never `yaml.load` on untrusted input (arbitrary object construction).
- **Trusting length prefixes / recursion depth from untrusted input** = decompression-bomb / stack-overflow DoS. Cap both.

## Verification checklists

Schema change:

- [ ] New protobuf fields use **new numbers**; removed numbers+names are `reserved`
- [ ] Change classified against the compatibility table (backward/forward/full) for this data's deploy cadence
- [ ] Renames/retypes done via expand/contract (add-new, dual-write, migrate, drop-old), not in place
- [ ] Readers ignore unknown fields (forward-compat) OR strictness is a deliberate internal choice
- [ ] proto3 tri-state fields use `optional`/wrappers, not zero-value-as-absent
- [ ] Round-trip tested: old writer→new reader **and** new writer→old reader

Correctness:

- [ ] 64-bit ints crossing a JSON boundary are strings (no `2^53` precision loss)
- [ ] Floats guarded for NaN/Infinity; money is integer-cents or decimal-string
- [ ] Dates are UTC ISO-8601 with offset
- [ ] Signed/hashed payloads use the **original bytes** or a canonical form (JCS/CBOR-canonical), never a re-serialization
- [ ] Untrusted decode has depth/length caps; YAML uses safe loader
