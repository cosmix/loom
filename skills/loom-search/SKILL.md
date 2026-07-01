---
name: loom-search
description: Full-text search and search engine implementation. Use when implementing search functionality, autocomplete, faceted search, relevance tuning, or working with search indexes like Elasticsearch, OpenSearch, Meilisearch, or Typesense.
triggers:
  - search
  - full-text search
  - Elasticsearch
  - OpenSearch
  - Meilisearch
  - Typesense
  - fuzzy search
  - autocomplete
  - faceted search
  - facets
  - inverted index
  - relevance
  - ranking
  - scoring
  - tokenizer
  - analyzer
  - search-as-you-type
  - aggregations
  - synonyms
  - indexing
  - query
  - filtering
  - highlighting
  - search UI
  - typeahead
  - suggestions
---

# Search

## Overview

Full-text search on Lucene-based engines (Elasticsearch/OpenSearch) plus the leaner alternatives (Meilisearch/Typesense). The engine is easy to stand up and easy to get subtly wrong: **analyzer mismatches**, **facet counts that fight their own filters**, **deep pagination that falls over at 10k**, and **relevance that looks fine on your three test queries and terrible in production.** This skill targets those.

`text` (analyzed, full-text, scored) vs `keyword` (exact, aggregatable, sortable, filterable) is the decision under most of these. Get the mapping right first.

## Analysis: the root of most bugs

An **analyzer** = optional char filters → one tokenizer → token filters. It runs at **index time** (on the stored field) and at **query time** (on the search string). The inverted index only ever contains *analyzed* tokens.

⚠ **Index-time / query-time analyzer mismatch is the #1 silent search bug.** If you index with an `edge_ngram` analyzer and *also* analyze the query with it, searching "cat" expands to `c, ca, cat` and matches "category", "catalog", "cathedral" — garbage relevance. The fix is almost always: aggressive analyzer at index time, plain analyzer at search time.

```json
"name": {
  "type": "text",
  "analyzer": "autocomplete",          // edge_ngram — index time only
  "search_analyzer": "autocomplete_search"  // just lowercase — query time
}
```

- **Reindex required to change the index-time analyzer** (existing tokens are already committed). `search_analyzer` can change without reindex.
- **normalizer** = the `keyword` equivalent of an analyzer (lowercase/asciifold only, no tokenizer) so exact-match/aggregation fields can be case-insensitive.
- Verify what a field actually produces with `GET /index/_analyze` — don't guess.

```json
POST /products/_analyze
{ "field": "name", "text": "Wireless Headphones" }   // shows the exact tokens indexed
```

**Common token filters:** `lowercase`, `asciifolding` (café→cafe), `stop` (drop the/a/is — omit for short-field/name search), `stemmer`/`snowball` (running→run), `synonym`/`synonym_graph`.

## Mapping

```json
PUT /products
{
  "settings": { "number_of_shards": 1, "number_of_replicas": 1 },
  "mappings": { "properties": {
    "name":        { "type": "text", "analyzer": "english",
                     "fields": { "raw": { "type": "keyword" } } },   // multi-field: search AND aggregate
    "category":    { "type": "keyword" },
    "price":       { "type": "float" },
    "created_at":  { "type": "date" }
  }}
}
```

- **Multi-fields** (`name` + `name.raw`) index one source field two ways — full-text on `name`, exact/sort/agg on `name.raw`. The standard pattern for "searchable and facetable."
- Use **explicit mappings in production.** Dynamic mapping infers types from the first doc seen — one stray `"12"` string makes a numeric field `text` forever (reindex to fix).
- **Shard sizing:** aim 10–50 GB/shard. Over-sharding (default was once 5) wastes heap and slows queries on small indices; start with 1 and scale by data size, not habit.

## Query vs Filter Context (performance)

- **`filter`** — yes/no, **no scoring, cached** (bitset). Use for exact terms, ranges, booleans. Fast and reused.
- **`must`/`should`** — contribute to `_score`, not cached. Use only when relevance ranking matters.

```json
{ "query": { "bool": {
  "must":   [ { "match": { "name": "headphones" } } ],          // scored
  "filter": [ { "range": { "price": { "gte": 50, "lte": 200 } } },
              { "term":  { "category": "electronics" } } ],       // cached, unscored
  "must_not": [ { "term": { "status": "discontinued" } } ]
}}}
```

⚠ Putting exact filters in `must` bloats scoring work and forfeits the filter cache for no relevance benefit. Ranges, terms, and boolean gates belong in `filter`.

## Relevance

Default similarity is **BM25** (replaced TF-IDF in ES 5+). Two knobs on the field's similarity:

- **`k1`** (default 1.2) — term-frequency saturation. Higher = repeated terms keep helping; lower = TF saturates fast.
- **`b`** (default 0.75) — length normalization. Higher penalizes long fields more. For short fields (titles/names) a *lower* `b` often helps; `b=0` ignores length entirely.

Tune via a custom similarity in settings only after boosting/field-weighting is exhausted — most "bad relevance" is a query/mapping problem, not a BM25 problem.

**Field boosting** (`^`) — cheap first lever: `"fields": ["name^5", "brand^3", "description", "tags"]`.

**`multi_match` types (pick deliberately):**

| Type | Behavior | Use for |
| --- | --- | --- |
| `best_fields` (default) | Score = single best-matching field | Terms expected together in one field |
| `most_fields` | Sum across fields | Same text analyzed multiple ways (stem + exact) |
| `cross_fields` | Treats fields as one big field, term-centric | "first last" across `first_name`/`last_name` |
| `phrase` | Phrase match per field | Exact phrase |

**`function_score`** — inject business signal (popularity, recency) into `_score`:

```json
{ "function_score": {
  "query": { "match": { "name": "laptop" } },
  "functions": [
    { "field_value_factor": { "field": "sales_count", "modifier": "log1p", "factor": 0.1 } },
    { "gauss": { "created_at": { "origin": "now", "scale": "30d", "decay": 0.5 } } }   // recency decay
  ],
  "score_mode": "sum", "boost_mode": "multiply"
}}
```

⚠ `field_value_factor` on a doc where the field is **missing** throws unless you set `"missing": <default>`. And an unbounded factor (raw `sales_count`) can swamp text relevance — always dampen with `log1p`/`sqrt`. Prefer `gauss`/`exp` decay for recency over hand-rolled math.

Tip: `_search?explain=true` (or `_explain`) shows the full score breakdown — use it instead of guessing why a doc ranks where it does. Newer ES also supports learning-to-rank / vector `knn` for hybrid semantic + lexical search.

## Faceted Search — the post_filter gotcha

The classic faceted-UI requirement: when a user checks **Category: Electronics**, the result list filters to electronics, but the **Category facet still shows counts for all categories** (so they can switch). If you put the category filter in the main `query`, your category aggregation only counts electronics — the other options vanish and the UI breaks.

**Correct pattern:** filter the *results* with `post_filter` (applied after aggs are computed), and scope each *aggregation* with a `filter` agg that excludes its own facet's filter.

```json
{
  "query": { "bool": { "filter": [ { "range": { "price": { "gte": 50 } } } ] } },  // filters that affect ALL facets
  "aggs": {
    "categories": { "terms": { "field": "category" } }   // NOT filtered by category → shows all options
  },
  "post_filter": { "term": { "category": "electronics" } } // narrows hits only, after aggs
}
```

- Rule of thumb: a filter that should affect a facet's own counts goes in the main `query`; a filter driven *by that facet* goes in `post_filter` (single facet) or per-agg `filter` (multiple interacting facets).
- `size: 0` when you only need aggregation counts (skip fetching hits).
- ⚠ `terms` aggregation `doc_count` can be **approximate** on sharded indices (each shard returns its top-N, then merges). Raise `shard_size` or accept the small error; don't build billing on facet counts.

## Pagination — deep-paging cliff

⚠ `from`/`size` is **capped at 10,000** (`index.max_result_window`) and gets quadratically expensive before then: every shard must collect `from + size` hits and the coordinator sorts them all. Fine for page 1–20 of a UI; fatal for scrolling/export.

| Method | Use | Notes |
| --- | --- | --- |
| `from`/`size` | Shallow UI paging (< a few thousand) | Simple; hard 10k wall; no stable ordering under writes |
| **`search_after`** | Deep paging, infinite scroll | Stateless cursor on a **unique tiebreaker sort** (e.g. `[score, _id]`); no offset cost |
| **PIT + `search_after`** | Consistent deep paging / export | Point-in-time snapshot freezes the index view across pages |
| `scroll` | Legacy bulk export | Deprecated for paging; holds a snapshot/context — heavy. Prefer PIT+search_after |

`search_after` requires a **deterministic total sort** including a unique field (usually `_id` or a tiebreaker) — otherwise pages overlap or skip.

## Autocomplete — pick the right mechanism

| Mechanism | Matches | Cost | Use when |
| --- | --- | --- | --- |
| **Completion suggester** | Prefix only, in-memory FST | Fastest, low latency | Pure typeahead over a curated field; supports contexts (category/geo filtering) |
| **`search_as_you_type` field** | Prefix + infix (shingles) | Moderate | Mid-word matching without hand-built n-grams |
| **`edge_ngram` analyzer** | Prefix, fully queryable | Higher index size | Prefix match combined with normal bool/filter queries and scoring |
| **`match_phrase_prefix`** | Last-term prefix on a normal field | No extra mapping | Quick-and-dirty; ⚠ slow, expands last term to many; not for high QPS |

- **Completion suggester** rebuilds from a dedicated `completion` field and doesn't reflect deletes until reindex/rebuild — it's a *suggestion* structure, not your live index.
- **`edge_ngram`:** set `min_gram`/`max_gram` and **only at index time** (query analyzer = lowercase), or you hit the mismatch bug above.
- Add **fuzziness** for typos: completion suggester `fuzzy.fuzziness: 1`, or `match` `fuzziness: AUTO` (0 edits for ≤2 chars, 1 for 3–5, 2 for >5 — Levenshtein/Damerau distance). Fuzziness is expensive; cap `max_expansions` and avoid `fuzziness: 2` on long high-QPS queries.
- Client: **debounce 200–300 ms**, require ≥2 chars, cancel stale in-flight requests.

## Zero-Downtime Reindex (alias pattern)

Never point your app at a concrete index name — always at an **alias.** Reindexing (new analyzer, mapping change, shard count) then becomes atomic and reversible.

```text
1. Create products_v2 with the new mapping/settings.
2. POST /_reindex  { source: products_v1, dest: products_v2 }   (optionally with a transform script)
3. Atomically swap the alias:
```

```json
POST /_aliases
{ "actions": [
  { "remove": { "index": "products_v1", "alias": "products" } },
  { "add":    { "index": "products_v2", "alias": "products" } }
]}
```

- The alias swap is a single atomic step — no window where `products` points at nothing.
- ⚠ Writes arriving *during* the reindex land only in v1. For live indexing, dual-write to both (or the alias) during the migration, or reindex a quiesced/append-only snapshot then catch up by `_reindex` with a `range` on `updated_at`.
- Roll back by swapping the alias back — v1 is untouched.
- Use **index templates** so time-series indices (`logs-*`) get consistent mappings automatically on rollover.

## Indexing Throughput

- **Bulk API** (`_bulk`, or `helpers.bulk` in Python) for imports — never one doc per request. Tune batch size to ~5–15 MB per bulk.
- During heavy bulk load: set `number_of_replicas: 0` and `refresh_interval: -1`, then restore afterward — refresh-per-doc is a major throughput killer.
- Provide explicit `_id`s only if you need upserts/dedup; letting ES auto-generate is faster (skips a get-before-write).
- `refresh_interval` (default 1s) is why new docs aren't searchable instantly — that's *near*-real-time, not real-time. Force with `?refresh=wait_for` only in tests.

## Engine Choice

- **Elasticsearch/OpenSearch** — max flexibility, aggregations, scale, vector search; operationally heavy. OpenSearch is the Apache-2.0 fork after ES's license change; APIs largely overlap but have diverged — check version docs.
- **Meilisearch / Typesense** — typo-tolerant, instant-search-first, sane defaults, tiny ops footprint; far less flexible aggregation/relevance control. Excellent for product/site search and autocomplete where you don't need ES's analytics.

## Reference Implementation (Node, bool + facets + highlight)

```javascript
async search(query, filters = {}, page = 1, pageSize = 20) {
  const must = query ? [{ multi_match: {
    query, fields: ["name^3", "description", "tags^2"], type: "best_fields", fuzziness: "AUTO" } }]
    : [{ match_all: {} }];
  const filter = [];
  if (filters.category) filter.push({ term: { category: filters.category } });
  if (filters.priceMin || filters.priceMax) filter.push({ range: { price: {
    ...(filters.priceMin && { gte: filters.priceMin }),
    ...(filters.priceMax && { lte: filters.priceMax }) } } });

  const res = await this.client.search({ index: "products", body: {
    from: (page - 1) * pageSize, size: pageSize,      // ⚠ shallow paging only (<10k)
    query: { bool: { must, filter } },
    aggs: { categories: { terms: { field: "category", size: 20 } },
            price_stats: { stats: { field: "price" } } },
    highlight: { fields: { name: {}, description: { fragment_size: 150 } } },
  }});
  return {
    hits: res.hits.hits.map(h => ({ ...h._source, _score: h._score, highlight: h.highlight })),
    total: res.hits.total.value, aggregations: res.aggregations,
  };
}
```

⚠ Highlight fragments go straight into `dangerouslySetInnerHTML` in most UIs — ES escapes the surrounding text, but confirm you're not concatenating unescaped user content around it (XSS).

## Verification Checklist

- [ ] `text` vs `keyword` chosen per field; facet/sort fields are `keyword` (or `.raw` multi-field)
- [ ] Explicit mapping in prod; no dynamic-mapping type drift
- [ ] Index-time vs query-time analyzers deliberate (autocomplete/ngram analyzed at index time *only*); verified with `_analyze`
- [ ] Exact/range/boolean predicates in `filter` context (cached), not `must`
- [ ] Facets show all options while results narrow — `post_filter`/per-agg filter, not everything in the main query
- [ ] Pagination beyond a few thousand uses `search_after` (+PIT for consistency), not `from`/`size`
- [ ] Autocomplete mechanism matches the need; edge_ngram not applied at query time; debounced client
- [ ] Reindex goes through an **alias** with an atomic swap; in-flight writes handled
- [ ] Bulk indexing with replicas/refresh tuned; not one-doc-per-request
- [ ] `function_score` fields handle `missing` and dampen magnitude (`log1p`/decay); relevance tested on representative queries, not 3 happy-path ones
- [ ] `terms` agg count approximation understood; not used where exact counts are load-bearing
