---
name: loom-sql-optimization
description: Analyzes and optimizes SQL queries for performance. Use for index design, query rewriting, EXPLAIN/EXPLAIN ANALYZE interpretation, PostgreSQL tuning, N+1 prevention, CTE and window function optimization, join strategies, and common SQL anti-patterns.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Bash
triggers:
  - SQL
  - query optimization
  - EXPLAIN
  - EXPLAIN ANALYZE
  - index
  - slow query
  - execution plan
  - query plan
  - join optimization
  - subquery
  - CTE
  - common table expression
  - window function
  - partition
  - N+1
  - query cache
  - database performance
  - sequential scan
  - index scan
  - bitmap scan
  - nested loop
  - hash join
  - merge join
  - PostgreSQL
  - query tuning
  - table scan
  - cardinality
  - statistics
  - vacuum
  - analyze
---

# SQL Optimization

## Overview

This skill focuses on analyzing and optimizing SQL queries for improved performance. It covers query analysis, index optimization, execution plan interpretation, query rewriting strategies, PostgreSQL-specific optimizations, and common anti-patterns. Use this skill for slow queries, N+1 problems, join optimization, index design, and database performance tuning.

## Instructions

### 1. Analyze Query Performance

- Identify slow queries from logs
- Run EXPLAIN/EXPLAIN ANALYZE
- Measure query execution time
- Check resource utilization

### 2. Understand Execution Plans

- Identify scan types (Sequential Scan, Index Scan, Bitmap Scan)
- Check join algorithms (Nested Loop, Hash Join, Merge Join)
- Analyze index usage and selectivity
- Find bottleneck operations (sorts, filters, aggregations)
- Understand cost estimates vs actual rows
- Check buffer usage and I/O patterns

### 3. Apply Optimizations

- Design appropriate indexes (B-tree, Hash, GiST, GIN)
- Rewrite inefficient queries (subqueries to JOINs, CTEs)
- Optimize join order and algorithms
- Use window functions for complex aggregations
- Leverage partial indexes and covering indexes
- Consider denormalization for read-heavy workloads
- Update table statistics (ANALYZE)
- Tune PostgreSQL configuration parameters

### 4. Validate Improvements

- Compare before/after metrics
- Test with production-like data
- Verify correctness
- Monitor after deployment

## Best Practices

1. **Index Strategically**: Index columns in WHERE, JOIN, ORDER BY
2. **Avoid SELECT \***: Select only needed columns
3. **Use EXPLAIN ANALYZE**: Always analyze execution plans with actual timing
4. **Limit Results**: Use pagination for large datasets
5. **Avoid N+1**: Use JOINs or batch queries
6. **Use `NOT EXISTS`, not `NOT IN`, on nullable subqueries**: `NOT IN` returns zero rows — silently — if any NULL appears in the subquery result, because SQL three-valued logic makes the predicate UNKNOWN for every row. `NOT EXISTS` is NULL-safe and the planner can execute it as an efficient hash anti-join; `NOT IN` with nullable input cannot be turned into an anti-join. (For positive/inclusion tests, `IN`, `ANY`, and `EXISTS` produce identical plans in modern PostgreSQL — pick the most readable; there is no performance reason to rewrite `IN` as `EXISTS`.)
7. **Update Statistics**: Run ANALYZE after bulk operations
8. **CTEs are not optimization fences by default (PG12+)**: A non-recursive, side-effect-free CTE referenced once is inlined (predicates push down, indexes are used); a CTE referenced more than once is materialized. Force behavior with `AS MATERIALIZED` (fence/single evaluation) or `AS NOT MATERIALIZED` (force inlining). The old `OFFSET 0` fence trick is obsolete — verify with EXPLAIN.
9. **Avoid Functions on Indexed Columns**: Prevents index usage
10. **Monitor Continuously**: Track query performance over time

## PostgreSQL-Specific Optimizations

### Execution Plan Operators

**Scan Types:**

- **Sequential Scan**: Full table scan (slow for large tables)
- **Index Scan**: Walks the index then fetches matching heap rows — best when the predicate matches only a SMALL fraction of the table. When many rows match, the planner switches to a Sequential Scan (or Bitmap Heap Scan for a middling fraction) because the random heap I/O of many index lookups costs more than reading the table sequentially.
- **Index Only Scan**: Uses covering index (fastest)
- **Bitmap Index Scan**: Multiple index scans combined (good for OR conditions)

**Join Algorithms:**

- **Nested Loop**: Best for small tables or index lookups
- **Hash Join**: Best for medium-sized tables with equality joins
- **Merge Join**: Best for large pre-sorted tables

### Statistics and Maintenance

```sql
-- Update table statistics for better query plans
ANALYZE table_name;

-- Check statistics freshness
SELECT schemaname, tablename, last_analyze, last_autoanalyze
FROM pg_stat_user_tables
WHERE schemaname = 'public'
ORDER BY last_analyze NULLS FIRST;

-- Find bloated tables
SELECT schemaname, tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size,
    n_dead_tup, n_live_tup,
    round(n_dead_tup * 100.0 / NULLIF(n_live_tup + n_dead_tup, 0), 2) AS dead_pct
FROM pg_stat_user_tables
WHERE n_dead_tup > 1000
ORDER BY n_dead_tup DESC;

-- Vacuum bloated tables
VACUUM ANALYZE table_name;
```

### Configuration Tuning

```sql
-- Key parameters to check
SHOW shared_buffers;        -- Should be 25% of RAM
SHOW effective_cache_size;  -- Should be 50-75% of RAM
SHOW work_mem;              -- Per sort/hash operation AND per parallel worker
SHOW random_page_cost;      -- Lower for SSDs (1.1-2.0)
```

`work_mem` is the limit for each sort/hash operation and each parallel worker — a single query with several such nodes can use N × `work_mem` simultaneously, and concurrent sessions multiply that further. Since PG14, hash operations use up to `hash_mem_multiplier` × `work_mem` (default 2.0). A large global `work_mem` on a high-concurrency server is an OOM risk: keep the global value modest and raise it per-session with `SET LOCAL work_mem` for known heavy reports. Set `log_temp_files = 0` and watch for `Batches > 1` on Hash nodes in `EXPLAIN ANALYZE` to detect spills.

## Common Anti-Patterns

### 1. SELECT \* in Application Code

```sql
-- BAD: Fetches unnecessary columns
SELECT * FROM users WHERE id = 1;

-- GOOD: Fetch only needed columns
SELECT id, email, name FROM users WHERE id = 1;
```

### 2. Implicit Type Conversion

```sql
-- BAD: Can't use index if id is integer
SELECT * FROM users WHERE id = '123';

-- GOOD: Match column type
SELECT * FROM users WHERE id = 123;
```

### 3. OR Conditions Without Indexes

```sql
-- BAD: May not use indexes efficiently
SELECT * FROM orders WHERE status = 'pending' OR status = 'processing';

-- GOOD: Rewrite same-column OR as IN
SELECT * FROM orders WHERE status IN ('pending', 'processing');
```

PostgreSQL generally rewrites OR conditions on the SAME column into `IN`/`ANY` and can use a plain B-tree index on that column directly. For OR conditions across DIFFERENT columns it combines separate index scans via a BitmapOr (or you can rewrite as `UNION ALL`). Verify the chosen path with EXPLAIN rather than assuming a partial index helps — a partial index only restricts which rows are indexed; it does not provide an access path for searching by those status values.

### 4. Correlated Subqueries

```sql
-- BAD: Executes subquery for each row
SELECT p.name,
    (SELECT COUNT(*) FROM order_items WHERE product_id = p.id) AS order_count
FROM products p;

-- GOOD: Use JOIN with aggregation
SELECT p.name, COUNT(oi.id) AS order_count
FROM products p
LEFT JOIN order_items oi ON oi.product_id = p.id
GROUP BY p.id, p.name;
```

### 5. Missing WHERE Clauses

```sql
-- BAD: Updates entire table
UPDATE products SET updated_at = NOW();

-- GOOD: Update only what changed
UPDATE products SET updated_at = NOW()
WHERE id IN (SELECT product_id FROM price_changes);
```

## Advanced Patterns

### CTEs (Common Table Expressions)

```sql
-- CTEs for readability and reusability
WITH recent_orders AS (
    SELECT customer_id, COUNT(*) AS order_count, SUM(total) AS total_spent
    FROM orders
    WHERE created_at > NOW() - INTERVAL '30 days'
    GROUP BY customer_id
),
high_value_customers AS (
    SELECT customer_id
    FROM recent_orders
    WHERE total_spent > 1000
)
SELECT c.name, c.email, ro.order_count, ro.total_spent
FROM customers c
INNER JOIN high_value_customers hvc ON c.id = hvc.customer_id
INNER JOIN recent_orders ro ON c.id = ro.customer_id;

-- Recursive CTEs for hierarchical data
WITH RECURSIVE category_tree AS (
    SELECT id, name, parent_id, 1 AS level
    FROM categories
    WHERE parent_id IS NULL
    UNION ALL
    SELECT c.id, c.name, c.parent_id, ct.level + 1
    FROM categories c
    INNER JOIN category_tree ct ON c.parent_id = ct.id
)
SELECT * FROM category_tree ORDER BY level, name;
```

### Window Functions

```sql
-- Ranking and row numbers
SELECT
    product_id,
    category_id,
    price,
    ROW_NUMBER() OVER (PARTITION BY category_id ORDER BY price DESC) AS price_rank,
    RANK() OVER (ORDER BY price DESC) AS overall_rank,
    DENSE_RANK() OVER (PARTITION BY category_id ORDER BY price DESC) AS dense_rank
FROM products;

-- Running totals and moving averages
SELECT
    date,
    revenue,
    SUM(revenue) OVER (ORDER BY date ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS running_total,
    AVG(revenue) OVER (ORDER BY date ROWS BETWEEN 6 PRECEDING AND CURRENT ROW) AS moving_avg_7day
FROM daily_sales
ORDER BY date;

-- Lead/Lag for time-series analysis
SELECT
    customer_id,
    order_date,
    total,
    LAG(order_date) OVER (PARTITION BY customer_id ORDER BY order_date) AS prev_order_date,
    LEAD(total) OVER (PARTITION BY customer_id ORDER BY order_date) AS next_order_total,
    total - LAG(total) OVER (PARTITION BY customer_id ORDER BY order_date) AS total_diff
FROM orders;
```

## Examples

### Example 1: Query Optimization with EXPLAIN

```sql
-- Original slow query
SELECT o.*, c.name, c.email
FROM orders o, customers c
WHERE o.customer_id = c.id
AND o.status = 'pending'
AND o.created_at > '2024-01-01'
ORDER BY o.created_at DESC;

-- Step 1: Analyze with EXPLAIN ANALYZE
-- SETTINGS (PG12) surfaces non-default GUCs so the plan reproduces across environments.
-- For a prepared statement's plan without executing, use EXPLAIN (GENERIC_PLAN) (PG16+).
EXPLAIN (ANALYZE, BUFFERS, SETTINGS, FORMAT TEXT)
SELECT o.*, c.name, c.email
FROM orders o, customers c
WHERE o.customer_id = c.id
AND o.status = 'pending'
AND o.created_at > '2024-01-01'
ORDER BY o.created_at DESC;

-- Output analysis:
-- Seq Scan on orders  (cost=0.00..15420.00 rows=50000)
--   Filter: (status = 'pending' AND created_at > '2024-01-01')
--   Rows Removed by Filter: 450000
-- Problem: Sequential scan on large table!

-- Step 2: Create composite index
CREATE INDEX idx_orders_status_created
ON orders(status, created_at DESC)
WHERE status IN ('pending', 'processing');
-- NOTE: partial-index predicates are matched at PLAN time by literal implication.
-- A parameterized query (WHERE status = $1) will NEVER use this partial index,
-- because the planner cannot prove $1 satisfies the predicate at plan time.
-- When the column must be bound, use a plain composite index (status, created_at).

-- Step 3: Rewrite with explicit JOIN
SELECT o.id, o.total, o.created_at, c.name, c.email
FROM orders o
INNER JOIN customers c ON o.customer_id = c.id
WHERE o.status = 'pending'
AND o.created_at > '2024-01-01'
ORDER BY o.created_at DESC
LIMIT 100;

-- After optimization:
-- Index Scan using idx_orders_status_created (cost=0.42..125.50 rows=100)
-- 99% reduction in query time!
```

### Example 2: N+1 Query Problem

```sql
-- Problem: N+1 queries
-- Application code:
-- orders = SELECT * FROM orders WHERE user_id = 1
-- for order in orders:
--     items = SELECT * FROM order_items WHERE order_id = order.id

-- Solution: Single query with JOIN
SELECT
    o.id AS order_id,
    o.total,
    o.created_at,
    oi.product_id,
    oi.quantity,
    oi.unit_price,
    p.name AS product_name
FROM orders o
LEFT JOIN order_items oi ON o.id = oi.order_id
LEFT JOIN products p ON oi.product_id = p.id
WHERE o.user_id = 1
ORDER BY o.created_at DESC, oi.id;

-- Alternative: Batch query
SELECT * FROM orders WHERE user_id = 1;
-- Get order IDs: [1, 2, 3, 4, 5]
SELECT * FROM order_items WHERE order_id IN (1, 2, 3, 4, 5);
```

### Example 3: Index Design Strategies

```sql
-- Single column index for equality checks
CREATE INDEX idx_users_email ON users(email);

-- Composite index for multiple conditions
-- Order columns: equality first, then range, then sort
CREATE INDEX idx_orders_user_status_date
ON orders(user_id, status, created_at DESC);
-- The trailing sort column eliminates a Sort node only when every PRECEDING column
-- is bound by an equality predicate AND the index ASC/DESC (and NULLS) direction
-- matches the ORDER BY exactly. If a preceding column uses IN(...) or a range, the
-- sort column degrades to a filter predicate and a Sort node reappears.
-- Confirm the Sort node is actually gone in EXPLAIN before relying on this.

-- Partial index for filtered queries
CREATE INDEX idx_orders_pending
ON orders(created_at DESC)
WHERE status = 'pending';

-- Covering index to avoid table lookups
CREATE INDEX idx_orders_summary
ON orders(user_id, status)
INCLUDE (total, created_at);

-- Expression index for computed conditions
CREATE INDEX idx_users_email_lower ON users(LOWER(email));

-- Check existing indexes
SELECT
    indexname,
    indexdef,
    pg_size_pretty(pg_relation_size(indexname::regclass)) AS size
FROM pg_indexes
WHERE tablename = 'orders';

-- Find unused indexes
SELECT
    schemaname, tablename, indexname,
    idx_scan, idx_tup_read, idx_tup_fetch
FROM pg_stat_user_indexes
WHERE idx_scan = 0
ORDER BY pg_relation_size(indexrelid) DESC;
```

### Example 4: Query Rewriting Patterns

```sql
-- Pattern 1: Replace subquery with JOIN
-- Before
SELECT * FROM orders
WHERE customer_id IN (SELECT id FROM customers WHERE region = 'US');

-- After
SELECT o.* FROM orders o
INNER JOIN customers c ON o.customer_id = c.id
WHERE c.region = 'US';

-- Pattern 2: Use EXISTS instead of IN for large subqueries
-- Before
SELECT * FROM products
WHERE id IN (SELECT product_id FROM order_items);

-- After
SELECT * FROM products p
WHERE EXISTS (
    SELECT 1 FROM order_items oi WHERE oi.product_id = p.id
);

-- Pattern 3: Avoid functions on indexed columns
-- Before (can't use index)
SELECT * FROM users WHERE YEAR(created_at) = 2024;

-- After (uses index)
SELECT * FROM users
WHERE created_at >= '2024-01-01' AND created_at < '2025-01-01';

-- Pattern 4: Optimize pagination
-- Before (slow for large offsets)
SELECT * FROM products ORDER BY id LIMIT 20 OFFSET 10000;

-- After (keyset pagination)
SELECT * FROM products
WHERE id > 10000
ORDER BY id
LIMIT 20;

-- Pattern 5: Batch operations
-- Before (row-by-row)
UPDATE products SET price = price * 1.1 WHERE id = 1;
UPDATE products SET price = price * 1.1 WHERE id = 2;
-- ... repeated 1000 times

-- After (single batch)
UPDATE products SET price = price * 1.1
WHERE id = ANY(ARRAY[1, 2, 3, ..., 1000]);

-- Or use CTE for complex batches
WITH price_updates AS (
    SELECT id, new_price FROM temp_price_updates
)
UPDATE products p
SET price = pu.new_price
FROM price_updates pu
WHERE p.id = pu.id;
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal guidance for thinking like a query-tuning expert. Defaults and version notes assume PostgreSQL.

### Reading EXPLAIN

**`Index Cond` is an access predicate; `Filter` is not.** On an Index Scan, `Index Cond` decides which index leaf entries are traversed; a `Filter` line is applied to every row those entries return, discarding non-matching rows afterward. A `Filter` on an Index Scan plus a high `Rows Removed by Filter` means the index is doing more work than the query needs — usually because a filtering column is not in the access predicate. Fix by reordering/extending the composite index so that column joins the `Index Cond`. (A `Filter` after a Seq Scan instead means no usable index at all.)

**Diagnose bad plans by estimated-vs-actual row divergence, not cost-vs-time.** Cost units (arbitrary) and actual time (ms) are not numerically comparable. The signal that matters is whether estimated `rows=N` tracks `actual rows=M`. A divergence of ~10× or more means the planner chose its architecture (nested loop vs hash join, index vs seq scan) on wrong cardinality — the usual root cause of a slow plan. Fixes in order: (1) `ANALYZE` to refresh stats; (2) raise per-column statistics target for skewed columns (`ALTER TABLE t ALTER COLUMN c SET STATISTICS 500`); (3) `CREATE STATISTICS` for correlated column groups. The `enable_*` GUCs (e.g. `SET enable_nestloop = off`) are diagnostic probes only — never a permanent fix, since they affect every query in the session.

**Looped-node times/rows are per-loop averages — multiply by `loops`.** For a node inside a loop (e.g. the inner side of a Nested Loop), `EXPLAIN ANALYZE` reports actual time and rows as averages PER execution, with `loops=N` the count. A node showing `actual time=0.003..0.005 rows=1 loops=10000` is ~30 ms total, not 0.003 ms. Related trap: under `LIMIT`, child nodes report only the rows delivered before the limit stopped them, not what they would have produced.

**Use the modern EXPLAIN options.** `SETTINGS` (PG12, non-default GUCs influencing the plan — essential for cross-environment reproduction); `WAL` (PG13, WAL generated by DML, needs `ANALYZE`); `GENERIC_PLAN` (PG16, the plan a prepared statement would use with `$1` placeholders, WITHOUT executing — cannot be combined with `ANALYZE`); `SERIALIZE` (PG17, cost of serializing output); `MEMORY` (PG18, planner memory). In PG18 `BUFFERS` is auto-included with `ANALYZE`. Recommended PG16+ form:

```sql
EXPLAIN (ANALYZE, BUFFERS, SETTINGS, FORMAT TEXT)
SELECT * FROM orders WHERE status = 'pending';
-- Inspect a prepared statement's generic plan without executing it (PG16+):
EXPLAIN (GENERIC_PLAN) SELECT * FROM orders WHERE customer_id = $1 AND status = $2;
```

### Index Design

**Equality columns must precede the range/sort column in a composite index.** In a B-tree, leading equality columns plus a single range column set the start/stop of the index scan (access predicates). Once a column is used for a range, every subsequent column can only filter row-by-row — narrowing nothing. So an index `(a, range_col, b)` with `WHERE a=? AND range_col>? AND b=?` scans the whole `range_col` span and merely filters on `b`. Rule: index for equality first, then for a single range, with the `ORDER BY` column last so it can also serve the sort.

```sql
-- GOOD: equality first, single range/sort column last → all are access predicates
CREATE INDEX idx_orders_user_status_date ON orders(user_id, status, created_at);
-- BAD: created_at (range) in the MIDDLE makes status a filter, not an access predicate
CREATE INDEX idx_bad ON orders(user_id, created_at, status);
```

**Sort direction (ASC/DESC, NULLS) must match `ORDER BY` exactly, or a Sort is forced.** A B-tree on `(x ASC, y ASC)` satisfies `ORDER BY x ASC, y ASC` (forward scan) or `x DESC, y DESC` (backward scan), but NOT mixed `x ASC, y DESC` — that needs an index declared `(x ASC, y DESC)`. NULLS placement is a second axis: default is `ASC NULLS LAST` / `DESC NULLS FIRST`, so `ORDER BY x ASC NULLS FIRST` won't match a default index. The mismatch is silent (correct results, but an explicit Sort node) and worst with `LIMIT`, which otherwise stops after N rows. Confirm by the presence/absence of a `Sort` node in EXPLAIN.

**Partial indexes need a literal predicate match — parameterized queries can't use them.** A partial index is used only if the planner can prove at PLAN time that the query's `WHERE` implies the index predicate (it recognizes only simple inequality implications like `x<1 ⇒ x<2`). A prepared statement `WHERE status = $1` will NEVER use a `WHERE status='pending'` partial index because the value is unknown at plan time — use a composite index `(status, created_at)` when the column must be bound. Many non-overlapping partial indexes as poor-man's partitioning is explicitly "a bad idea" per the docs; use real partitioning or a composite index.

**`INCLUDE` builds covering indexes; Index-Only Scans depend on the visibility map.** `INCLUDE (cols)` stores payload columns only in leaf pages (keeping the B-tree key compact, not affecting uniqueness). But an Index-Only Scan avoids the heap only for pages whose visibility-map bit is set; on high-UPDATE/DELETE tables the bit is often unset, so the scan silently degrades to heap fetches. Watch `Heap Fetches: N` in EXPLAIN; check churn via `n_dead_tup` in `pg_stat_user_tables` and `VACUUM` to restore VM coverage.

**HOT updates are lost when any indexed column changes — driving index bloat.** Heap-Only Tuple updates skip all indexes when only non-indexed columns change and the new tuple fits the same heap page. But updating even one INDEXED column (even to the same value) bypasses HOT and inserts an entry into EVERY index, leaving one dead entry per index per update for VACUUM to reclaim. Common trap: adding a frequently-updated column like `updated_at` to many indexes "for completeness" destroys HOT. Index only columns used in WHERE/JOIN, and set `fillfactor < 100` on hot tables so in-page HOT updates have room.

**BRIN indexes only help when values correlate with physical row order.** A BRIN stores only per-block-range summaries (min/max), making it 2–3 orders of magnitude smaller than a B-tree — ideal for append-only time-series/audit tables where timestamps or sequential IDs grow monotonically with insert order. It is useless for randomly-distributed columns (UUIDs, churned status): no block range can be eliminated, so the planner ignores it. Check `SELECT correlation FROM pg_stats WHERE tablename='t' AND attname='col'` (near ±1.0 is good) before creating. BRIN serves large range scans, not point lookups (still need a B-tree).

```sql
CREATE INDEX idx_events_ts_brin ON events USING BRIN (occurred_at);
```

**PG18 B-tree skip scan relaxes the leading-column rule for low-cardinality prefixes.** As of PG18 (2025-09-25), a composite B-tree on `(a, b)` can serve a query constraining only `b` if `a` has FEW distinct values — the planner issues one index search per distinct `a` value. It fires only when distinct leading values are few enough to skip most of the index; with many, it scans the whole index and the planner usually prefers a seq scan. On PG18 you may no longer need a redundant single-column index behind a low-cardinality leading column. It is automatic and cost-based — verify with EXPLAIN.

### Query Rewrites & Anti-Patterns

**`NOT IN` with a nullable subquery silently returns zero rows — use `NOT EXISTS`.** One NULL in the subquery makes `x NOT IN (..., NULL)` evaluate to UNKNOWN (never TRUE) for every row, so the `WHERE` filters out ALL rows with no error. This is a correctness bug, triggered whenever the subquery column lacks a `NOT NULL` constraint or an outer join feeds NULLs in. `NOT EXISTS` tests existence, is NULL-safe, and lets the planner use a hash anti-join; `NOT IN` with nullable input degrades to an expensive correlated subplan.

```sql
-- GOOD: NULL-safe, hash anti-join
SELECT * FROM products p
WHERE NOT EXISTS (SELECT 1 FROM order_items oi WHERE oi.product_id = p.id);
-- BAD: returns ZERO rows if order_items.product_id has ANY NULL
SELECT * FROM products WHERE id NOT IN (SELECT product_id FROM order_items);
```

**Keyset (seek) pagination eliminates cumulative OFFSET cost.** `OFFSET N` fetches and discards N rows, so latency grows linearly with page depth. Keyset pagination anchors a `WHERE` to the last-seen sort-key value, letting an index seek jump straight to position — constant time regardless of page. The sort key must be unique (add an `id` tiebreaker); use a row-value comparison so it maps to a composite index.

```sql
SELECT id, created_at, title FROM posts
WHERE (created_at, id) < ($last_created_at, $last_id)
ORDER BY created_at DESC, id DESC LIMIT 20;
```

**Prefer `timestamptz` over `timestamp`; use half-open ranges (`>=`, `<`) not `BETWEEN`.** `timestamp` (without zone) has no zone context, so arithmetic across DST or between servers is silently wrong; `timestamptz` stores an absolute UTC instant. Separately, `BETWEEN` is inclusive on BOTH ends: `created_at BETWEEN '2024-01-01' AND '2024-01-31'` includes only midnight Jan 31, excluding the rest of that day. The idiomatic range `>= lower AND < upper_exclusive` is unambiguous, microsecond-correct, and index-friendly.

### Planner Statistics

**Correlated columns need `CREATE STATISTICS` — per-column stats assume independence.** PostgreSQL multiplies individual column selectivities, badly underestimating result rows for correlated columns (zip→city, status→substatus), which pushes the planner into a nested loop sized for a tiny result — catastrophic on the real large result. Adding indexes does NOT fix a misestimate; `CREATE STATISTICS` (PG10+) does. Pick the kind: `dependencies` (functional deps on equality), `ndistinct` (GROUP BY combination counts), `mcv` (skewed multi-column value pairs). Always `ANALYZE` after and confirm with EXPLAIN.

```sql
CREATE STATISTICS order_stats (mcv) ON status, region FROM orders;
ANALYZE orders;
```

### Prepared Statements & Concurrency

**Prepared statements switch from custom to generic plans after 5 executions.** The first five executions use custom plans (planned with actual values); PostgreSQL then builds a generic plan (planned without knowing values, on average stats) and uses it if its cost is "not so much higher" than the custom average (`plan_cache_mode='auto'`). For skewed columns a generic plan optimized for one value can be disastrous for another. Inspect it directly with `EXPLAIN (GENERIC_PLAN)` (PG16+) — the often-repeated "read `$1` vs literals in EXPLAIN EXECUTE" trick is unreliable. Force per-execution plans with `SET plan_cache_mode = 'force_custom_plan'`.

**User-defined functions are `PARALLEL UNSAFE` by default, silently disabling parallel query.** Any `PARALLEL UNSAFE` object anywhere in a query (WHERE, SELECT, JOIN) disables parallelism for the whole query — no `Gather` node appears, no warning. Mark a function `PARALLEL SAFE` only if it has no side effects, does not write, and touches no connection-local state; functions reading temp tables or session state must be `PARALLEL RESTRICTED` (leader-only). Mislabeling a writing function as SAFE is silently incorrect.

```sql
CREATE FUNCTION score_record(val int) RETURNS int
  AS $$ SELECT val * 2; $$ LANGUAGE SQL PARALLEL SAFE IMMUTABLE;
```

**`READ COMMITTED` allows mid-statement inconsistency; `REPEATABLE READ` needs retry logic.** `READ COMMITTED` (default) takes a new snapshot per statement, so one `UPDATE`/`DELETE` can see a mix of pre- and post-commit data and silently skip rows whose qualifying columns changed mid-scan. `REPEATABLE READ` uses one snapshot per transaction but raises `could not serialize access due to concurrent update` errors the application MUST catch and retry from the beginning. Orthogonal trap: sequences (`nextval`) are non-transactional — values consumed are never returned on `ROLLBACK`, so gaps are permanent and expected.

### Maintenance

**Transaction ID wraparound silently approaches, then forces read-only — monitor `relfrozenxid` age.** With 32-bit XIDs (~4 billion circular), a table's `relfrozenxid` aging past `autovacuum_freeze_max_age` (default 200M) triggers a forced freeze. If autovacuum is disabled/starved/blocked, age climbs: warnings begin at 40M transactions from wraparound, and below 3M remaining the server refuses new XIDs — writes fail until VACUUM runs. Long-running/idle-in-transaction sessions, forgotten prepared transactions (`pg_prepared_xacts`), and replication slots all hold back the oldest XID. Alert well before 150M; in a crisis run plain `VACUUM` (`VACUUM FULL`/`FREEZE` consume more XIDs and worsen it).

```sql
SELECT datname, age(datfrozenxid) FROM pg_database ORDER BY 2 DESC;
```
