# Architecture

> The architecture knowledge fixture describes the context retrieval shape.

## Overview

The catalog turns curated prose into deterministic chunks for retrieval. The
store persists the derived catalog while the source files remain untouched.
The implementation is coordinated by `ContextStore` and `src/context/pack.rs`.

## Retrieval

Ranking and packing operate on stable chunk identities. The packer lives in
`src/context/pack.rs`, and the catalog can link to [Duplicate Headings](architecture/duplicate-headings.md)
for a focused example of repeated section names.

## Boundaries

The context boundary reads markdown and reports diagnostics without repairing
curated content. `ContextStore` owns cache files outside this fixture tree.
