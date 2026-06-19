---
name: loom-database-design
description: Database schema and data model design for relational, NoSQL, time-series, and warehouse systems. Use for ERDs, normalization/denormalization, indexing, migrations, star/snowflake schemas, event sourcing, and OLTP/OLAP performance tuning.
allowed-tools:
  - Read
  - Grep
  - Glob
  - Edit
  - Write
  - Bash
triggers:
  - schema
  - table
  - column
  - migration
  - ERD
  - normalize
  - denormalize
  - index
  - foreign key
  - primary key
  - constraint
  - relationship
  - SQL
  - DDL
  - data model
  - database design
  - data warehouse
  - star schema
  - snowflake schema
  - time-series
  - event sourcing
  - dimension table
  - fact table
  - ETL
  - data pipeline
  - OLAP
  - OLTP
---

# Database Design

## Overview

This skill focuses on designing efficient, scalable, and maintainable database schemas and data models. It covers:

- **OLTP Systems**: Relational databases (PostgreSQL, MySQL) with normalization and transactional integrity
- **OLAP Systems**: Data warehouses with star/snowflake schemas for analytics
- **NoSQL**: Document stores (MongoDB), key-value (Redis), wide-column (Cassandra)
- **Time-Series**: Specialized databases for metrics and events (TimescaleDB, InfluxDB)
- **Event Sourcing**: Append-only event stores for audit and temporal queries
- **Data Pipelines**: Schema design considerations for ETL/ELT workflows

This skill incorporates data modeling expertise for both operational and analytical workloads.

## Instructions

### 1. Understand Data Requirements

- Identify entities and their attributes
- Map relationships between entities (one-to-one, one-to-many, many-to-many)
- Determine data access patterns (read vs write heavy, query patterns)
- Estimate data volumes, growth rate, and retention requirements
- Distinguish OLTP (transactional) vs OLAP (analytical) needs

### 2. Design Schema

**For OLTP (Transactional Systems)**:

- Normalize to 3NF to eliminate redundancy
- Define primary keys (surrogate vs natural)
- Establish foreign key relationships with appropriate cascade rules
- Choose appropriate data types for storage efficiency
- Plan for NULL handling and default values
- Add CHECK constraints for data integrity

**For OLAP (Data Warehouses)**:

- Design star schema (central fact table with dimension tables)
- Or snowflake schema (normalized dimensions) if cardinality is high
- Create slowly changing dimensions (SCD Type 1, 2, or 3)
- Denormalize for query performance
- Add surrogate keys for dimension tables
- Design fact tables with foreign keys to dimensions and measure columns

**For Time-Series**:

- Use timestamp as primary key component
- Partition by time ranges (day, week, month)
- Design for append-only writes
- Consider downsampling and aggregation tables
- Use appropriate retention policies

**For Event Sourcing**:

- Store events as immutable append-only records
- Include event type, aggregate ID, timestamp, payload
- Design projections for read models
- Plan for event versioning and schema evolution

### 3. Optimize for Performance

- Design indexes for query patterns (WHERE, JOIN, ORDER BY, GROUP BY)
- **Index every foreign key column on the child (referencing) side** — PostgreSQL auto-indexes the parent primary key but NOT the child FK column, so unindexed FKs force a sequential scan of the child table on every parent `DELETE`/`UPDATE` (referential-integrity check) and on `ON DELETE CASCADE` (see Expert Practices)
- Consider covering indexes to avoid table lookups (use `INCLUDE` for payload columns)
- Use partial indexes for filtered queries — but the planner only uses one when it can prove at PLAN time the query `WHERE` implies the index predicate; parameterized/prepared statements (`$1`) defeat them (see Expert Practices)
- Plan denormalization for read-heavy workloads — model it as *derived data* refreshed from a single system of record, not duplicated app-side writes (see Expert Practices)
- Design partitioning strategy for large tables — use declarative `PARTITION BY RANGE/LIST/HASH` (PG10+), never inheritance + trigger routing
- Add materialized views for expensive aggregations
- Design for concurrent access: the default `READ COMMITTED` gives a fresh snapshot per *statement* (not per transaction) and allows lost updates, vanishing `DELETE`s, and write skew. For cross-row invariants or financial logic use `SERIALIZABLE` and retry on `SQLSTATE 40001` (see Expert Practices)

### 4. Plan Migrations

- Create reversible migrations with UP and DOWN scripts
- Handle data transformations safely (backfill, defaults)
- Plan for zero-downtime deployments (expand/contract pattern)
- Version control all schema changes
- Test migrations on production-like data volumes
- Document breaking changes and migration dependencies

**Lock-aware DDL (large tables):** most `ALTER TABLE` forms take `ACCESS EXCLUSIVE`, and the lock *queues* — a slow open transaction makes the DDL wait, and all subsequent traffic queues behind it. Always:

- Set a low `lock_timeout` (e.g. `200ms`) before any `ALTER TABLE` so it fails fast instead of stalling all traffic, and retry with exponential backoff + jitter.
- Add FK/CHECK/NOT NULL with the two-phase **`NOT VALID` + `VALIDATE CONSTRAINT`** pattern: `ADD CONSTRAINT ... NOT VALID` enforces on new writes with a brief lock and no scan; `VALIDATE CONSTRAINT` then scans existing rows under `SHARE UPDATE EXCLUSIVE`, which does NOT block concurrent DML.
- Add unique constraints via `CREATE UNIQUE INDEX CONCURRENTLY` then `ADD CONSTRAINT ... USING INDEX`.
- Know what rewrites: a *constant* `DEFAULT` on `ADD COLUMN` is metadata-only on PG11+; a *volatile* default (`gen_random_uuid()`, `now()`) and almost any column type change rewrite the whole table — use expand/contract for type changes.

(See Expert Practices for the full mechanism and example SQL.)

### 5. Consider ETL/Data Pipeline Impact

- Design schemas that support efficient bulk loading
- Add staging tables for incremental updates
- Include audit columns (created_at, updated_at, loaded_at)
- Plan for change data capture (CDC) if needed
- Design idempotent upsert operations
- Consider schema evolution and backward compatibility

## Best Practices

1. **Choose Appropriate Types**: Use correct data types for storage efficiency (`VARCHAR`/`TEXT` over `char(n)`, `DECIMAL` over `FLOAT` for money). For auto-incremented surrogate PKs default to `BIGINT GENERATED ALWAYS AS IDENTITY` — `INT4` caps at ~2.1B and fails *hard* on insert at the limit, and `SERIAL` is a legacy pseudo-type with sequence-ownership/permission/`pg_dump` pitfalls. Use `TIMESTAMPTZ` (never bare `TIMESTAMP`) for real-world instants. (See Expert Practices.)
2. **Index Wisely**: Index columns used in WHERE, JOIN, ORDER BY, GROUP BY, but avoid over-indexing — every index is a write tax (a separate B-tree updated on every write). Audit `pg_stat_user_indexes` for `idx_scan = 0` and drop unused indexes.
3. **Normalize First**: Start normalized (3NF) for OLTP, denormalize strategically for OLAP or read-heavy workloads. Frame it as system-of-record (normalized, one home per fact) vs *derived* read models (materialized views, projections) refreshed from it.
4. **Use Constraints**: Enforce data integrity at database level (PRIMARY KEY, FOREIGN KEY, UNIQUE, CHECK, NOT NULL). Avoid `NOT IN (subquery)` when the subquery column is nullable — a single NULL silently returns zero rows; use `NOT EXISTS`. (See Expert Practices.)
5. **Plan for Scale**: Consider sharding, partitioning, and replication early for high-volume tables
6. **Document Schemas**: Maintain ERD, data dictionary, and relationship diagrams
7. **Test Migrations**: Always test on production-like data volumes and monitor performance
8. **Audit Everything**: Add created_at, updated_at, created_by for accountability
9. **Version Events**: For event sourcing, include schema version in event payload
10. **Optimize for Cardinality**: High-cardinality columns benefit from indexes, low-cardinality may not
11. **Separate Reads from Writes**: For high-scale systems, consider CQRS pattern with separate read/write models
12. **Design for Idempotency**: Ensure ETL operations can safely retry without duplicates

## Examples

### Example 1: E-Commerce Schema (PostgreSQL)

> **PK choice:** `gen_random_uuid()` (UUIDv4) is random and fragments the B-tree (page splits, low fill) on high-volume tables. It is fine for low-volume tables; for write-heavy OLTP prefer `uuidv7()` (PG18+) or `pg_uuidv7`'s `uuid_generate_v7()` (PG14-17) — time-ordered, append-friendly, but embeds an approximate creation timestamp — or `BIGINT GENERATED ALWAYS AS IDENTITY` when an opaque/distributed ID is not required. (See Expert Practices.)

```sql
-- Users table with proper constraints
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,

    CONSTRAINT email_format CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$')
);

-- Products with proper indexing
CREATE TABLE products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sku VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    price DECIMAL(10, 2) NOT NULL CHECK (price >= 0),
    stock_quantity INTEGER NOT NULL DEFAULT 0 CHECK (stock_quantity >= 0),
    category_id UUID REFERENCES categories(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX idx_products_category ON products(category_id);
CREATE INDEX idx_products_price ON products(price);
CREATE INDEX idx_products_name_search ON products USING gin(to_tsvector('english', name));

-- Orders with proper relationships
CREATE TABLE orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    total_amount DECIMAL(10, 2) NOT NULL,
    shipping_address JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_status CHECK (status IN ('pending', 'paid', 'shipped', 'delivered', 'cancelled'))
);

CREATE INDEX idx_orders_user ON orders(user_id);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_created ON orders(created_at DESC);

-- Order items junction table
CREATE TABLE order_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_price DECIMAL(10, 2) NOT NULL,

    UNIQUE(order_id, product_id)
);

-- FK indexes: UNIQUE(order_id, product_id) leads with order_id so it covers
-- the FK check on orders deletes, but NOTHING leads with product_id — deleting
-- a product would seq-scan order_items. Index every child FK column explicitly.
CREATE INDEX idx_order_items_product_id ON order_items(product_id);
-- For a pure junction table, PRIMARY KEY (order_id, product_id) (dropping the
-- surrogate UUID) removes the redundant unique index entirely.
```

### Example 2: Migration Script

```sql
-- Migration: Add customer loyalty program
-- Version: 20240115_001

-- Fail fast instead of queuing behind a long transaction and stalling all
-- traffic to users; run the whole migration inside a backoff+jitter retry loop.
SET lock_timeout = '200ms';

BEGIN;

-- Add loyalty tier to users.
-- Constant DEFAULTs are metadata-only on PG11+ (no table rewrite). A volatile
-- default like now()/gen_random_uuid() WOULD rewrite the whole table.
ALTER TABLE users
ADD COLUMN loyalty_tier VARCHAR(20) DEFAULT 'bronze',
ADD COLUMN loyalty_points INTEGER DEFAULT 0;

-- Add the CHECK in two phases so it never holds ACCESS EXCLUSIVE for a full scan.
-- Phase 1: enforce on new writes immediately, brief lock, no scan.
ALTER TABLE users
ADD CONSTRAINT valid_loyalty_tier
CHECK (loyalty_tier IN ('bronze', 'silver', 'gold', 'platinum')) NOT VALID;

COMMIT;

-- Phase 2: validate existing rows under SHARE UPDATE EXCLUSIVE (concurrent DML
-- is NOT blocked). Run outside the transaction above.
ALTER TABLE users VALIDATE CONSTRAINT valid_loyalty_tier;

BEGIN;

-- Create points history table
CREATE TABLE loyalty_points_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    points_change INTEGER NOT NULL,
    reason VARCHAR(100) NOT NULL,
    reference_type VARCHAR(50),
    reference_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_loyalty_history_user ON loyalty_points_history(user_id);
CREATE INDEX idx_loyalty_history_created ON loyalty_points_history(created_at DESC);

COMMIT;

-- Rollback script (save separately)
-- BEGIN;
-- DROP TABLE IF EXISTS loyalty_points_history;
-- ALTER TABLE users DROP COLUMN IF EXISTS loyalty_points;
-- ALTER TABLE users DROP COLUMN IF EXISTS loyalty_tier;
-- COMMIT;
```

### Example 3: MongoDB Document Design

```javascript
// User document with embedded addresses
{
  _id: ObjectId("..."),
  email: "user@example.com",
  profile: {
    name: "John Doe",
    avatar_url: "https://..."
  },
  addresses: [
    {
      type: "shipping",
      street: "123 Main St",
      city: "Boston",
      state: "MA",
      zip: "02101",
      is_default: true
    }
  ],
  preferences: {
    newsletter: true,
    notifications: {
      email: true,
      push: false
    }
  },
  created_at: ISODate("2024-01-15T10:00:00Z")
}

// Indexes
db.users.createIndex({ email: 1 }, { unique: true });
db.users.createIndex({ "addresses.zip": 1 });
db.users.createIndex({ created_at: -1 });
```

### Example 4: Data Warehouse Star Schema (PostgreSQL)

```sql
-- Dimension: Date (conformed dimension)
CREATE TABLE dim_date (
    date_key INTEGER PRIMARY KEY,  -- YYYYMMDD format
    full_date DATE NOT NULL,
    day_of_week INTEGER,
    day_name VARCHAR(10),
    month INTEGER,
    month_name VARCHAR(10),
    quarter INTEGER,
    year INTEGER,
    is_weekend BOOLEAN,
    is_holiday BOOLEAN
);

-- Dimension: Product
CREATE TABLE dim_product (
    product_key BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,  -- Surrogate key (SQL-standard, replaces legacy SERIAL)
    product_id VARCHAR(50) NOT NULL,  -- Natural key from source
    product_name VARCHAR(255) NOT NULL,
    category VARCHAR(100),
    subcategory VARCHAR(100),
    brand VARCHAR(100),
    unit_cost DECIMAL(10, 2),

    -- SCD Type 2 columns for tracking changes
    effective_date DATE NOT NULL,
    expiration_date DATE,
    is_current BOOLEAN DEFAULT TRUE,

    UNIQUE(product_id, effective_date)
);

CREATE INDEX idx_dim_product_current ON dim_product(product_id) WHERE is_current = TRUE;

-- Dimension: Customer
CREATE TABLE dim_customer (
    customer_key BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    customer_id VARCHAR(50) NOT NULL,
    customer_name VARCHAR(255),
    customer_segment VARCHAR(50),
    region VARCHAR(100),
    country VARCHAR(100),

    -- SCD Type 1 (overwrite) for most attributes
    -- Use SCD Type 2 if you need to track segment changes
    effective_date DATE NOT NULL,
    expiration_date DATE,
    is_current BOOLEAN DEFAULT TRUE
);

-- Fact: Sales (central fact table)
CREATE TABLE fact_sales (
    sale_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    date_key INTEGER NOT NULL REFERENCES dim_date(date_key),
    product_key INTEGER NOT NULL REFERENCES dim_product(product_key),
    customer_key INTEGER NOT NULL REFERENCES dim_customer(customer_key),

    -- Measures (additive facts)
    quantity INTEGER NOT NULL,
    unit_price DECIMAL(10, 2) NOT NULL,
    discount_amount DECIMAL(10, 2) DEFAULT 0,
    tax_amount DECIMAL(10, 2) NOT NULL,
    total_amount DECIMAL(10, 2) NOT NULL,
    cost_amount DECIMAL(10, 2) NOT NULL,

    -- Degenerate dimensions (attributes with no separate dimension table)
    order_number VARCHAR(50),
    transaction_time TIMESTAMP NOT NULL
);

-- Indexes for typical analytical queries
CREATE INDEX idx_fact_sales_date ON fact_sales(date_key);
CREATE INDEX idx_fact_sales_product ON fact_sales(product_key);
CREATE INDEX idx_fact_sales_customer ON fact_sales(customer_key);
CREATE INDEX idx_fact_sales_composite ON fact_sales(date_key, product_key, customer_key);

-- Materialized view for pre-aggregated monthly sales
CREATE MATERIALIZED VIEW mv_monthly_sales AS
SELECT
    d.year,
    d.month,
    p.category,
    c.region,
    SUM(f.quantity) AS total_quantity,
    SUM(f.total_amount) AS total_revenue,
    SUM(f.cost_amount) AS total_cost,
    SUM(f.total_amount - f.cost_amount) AS total_profit
FROM fact_sales f
JOIN dim_date d ON f.date_key = d.date_key
JOIN dim_product p ON f.product_key = p.product_key
JOIN dim_customer c ON f.customer_key = c.customer_key
GROUP BY d.year, d.month, p.category, c.region;

CREATE INDEX idx_mv_monthly_sales ON mv_monthly_sales(year, month, category);
```

### Example 5: Time-Series Database (TimescaleDB)

```sql
-- Create hypertable for metrics
CREATE TABLE metrics (
    time TIMESTAMPTZ NOT NULL,
    device_id VARCHAR(50) NOT NULL,
    metric_name VARCHAR(100) NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    tags JSONB,

    PRIMARY KEY (time, device_id, metric_name)
);

-- Convert to hypertable with 1-day chunks
SELECT create_hypertable('metrics', 'time', chunk_time_interval => INTERVAL '1 day');

-- Create indexes for common query patterns
CREATE INDEX idx_metrics_device_time ON metrics(device_id, time DESC);
CREATE INDEX idx_metrics_name_time ON metrics(metric_name, time DESC);
CREATE INDEX idx_metrics_tags ON metrics USING gin(tags);

-- Compression policy (compress chunks older than 7 days).
-- Note: TimescaleDB 2.18+ introduces the "hypercore" storage engine as the
-- recommended default for new hypertables, unifying hot rowstore and cold
-- columnstore and automating conversion. The manual policy below is for older
-- versions or fine-grained control. (Moving target — verify against your version.)
ALTER TABLE metrics SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'device_id, metric_name'
);

SELECT add_compression_policy('metrics', INTERVAL '7 days');

-- Retention policy (drop chunks older than 90 days)
SELECT add_retention_policy('metrics', INTERVAL '90 days');

-- Continuous aggregate for hourly rollups
CREATE MATERIALIZED VIEW metrics_hourly
WITH (timescaledb.continuous) AS
SELECT
    time_bucket('1 hour', time) AS hour,
    device_id,
    metric_name,
    AVG(value) AS avg_value,
    MAX(value) AS max_value,
    MIN(value) AS min_value,
    COUNT(*) AS count
FROM metrics
GROUP BY hour, device_id, metric_name;

-- Refresh policy for continuous aggregate
SELECT add_continuous_aggregate_policy('metrics_hourly',
    start_offset => INTERVAL '3 hours',
    end_offset => INTERVAL '1 hour',
    schedule_interval => INTERVAL '1 hour');
```

### Example 6: Event Sourcing Pattern (PostgreSQL)

```sql
-- Event store (append-only)
CREATE TABLE events (
    event_id BIGSERIAL PRIMARY KEY,
    aggregate_id UUID NOT NULL,
    aggregate_type VARCHAR(100) NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    event_version INTEGER NOT NULL,
    payload JSONB NOT NULL,
    metadata JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- For optimistic concurrency control
    sequence_number INTEGER NOT NULL,

    CONSTRAINT unique_sequence UNIQUE(aggregate_id, sequence_number)
);

-- Indexes for event replay
CREATE INDEX idx_events_aggregate ON events(aggregate_id, sequence_number);
CREATE INDEX idx_events_type_time ON events(event_type, occurred_at);
CREATE INDEX idx_events_occurred ON events(occurred_at DESC);

-- Snapshots for performance (optional, reduces replay cost)
CREATE TABLE snapshots (
    snapshot_id BIGSERIAL PRIMARY KEY,
    aggregate_id UUID NOT NULL,
    aggregate_type VARCHAR(100) NOT NULL,
    sequence_number INTEGER NOT NULL,
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT unique_snapshot UNIQUE(aggregate_id, sequence_number)
);

CREATE INDEX idx_snapshots_aggregate ON snapshots(aggregate_id, sequence_number DESC);

-- Projection (read model) - materialized view of event stream
CREATE TABLE account_balances (
    account_id UUID PRIMARY KEY,
    current_balance DECIMAL(15, 2) NOT NULL,
    last_event_sequence INTEGER NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

-- Example: Event handlers update projections
-- (In application code, not database triggers for better testability)
-- When AccountCredited event occurs:
--   UPDATE account_balances SET current_balance = current_balance + amount

-- Query to rebuild projection from events
CREATE OR REPLACE FUNCTION rebuild_account_balance(p_account_id UUID)
RETURNS DECIMAL AS $$
DECLARE
    v_balance DECIMAL(15, 2) := 0;
BEGIN
    SELECT COALESCE(SUM(
        CASE
            WHEN event_type = 'AccountCredited' THEN (payload->>'amount')::DECIMAL
            WHEN event_type = 'AccountDebited' THEN -(payload->>'amount')::DECIMAL
            ELSE 0
        END
    ), 0)
    INTO v_balance
    FROM events
    WHERE aggregate_id = p_account_id
    AND aggregate_type = 'Account'
    ORDER BY sequence_number;

    RETURN v_balance;
END;
$$ LANGUAGE plpgsql;
```

### Example 7: ETL Staging Pattern (PostgreSQL)

```sql
-- Staging table for incremental loads
CREATE TABLE staging_orders (
    order_id VARCHAR(50) PRIMARY KEY,
    customer_id VARCHAR(50),
    order_date TIMESTAMPTZ,
    total_amount DECIMAL(10, 2),
    status VARCHAR(20),

    -- ETL metadata
    source_system VARCHAR(50),
    extracted_at TIMESTAMPTZ NOT NULL,
    loaded_at TIMESTAMPTZ DEFAULT NOW(),
    batch_id VARCHAR(100),

    -- For change detection
    source_hash VARCHAR(64),
    is_processed BOOLEAN DEFAULT FALSE
);

-- Production table
CREATE TABLE orders (
    order_id VARCHAR(50) PRIMARY KEY,
    customer_id VARCHAR(50),
    order_date TIMESTAMPTZ,
    total_amount DECIMAL(10, 2),
    status VARCHAR(20),

    -- Audit columns
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source_system VARCHAR(50),

    -- For CDC
    source_hash VARCHAR(64),
    version INTEGER DEFAULT 1
);

-- Merge staging into production (idempotent upsert).
-- Use a single MERGE (PG15+): a two-pass INSERT-then-UPDATE has a race window
-- where a concurrent session can insert the same order_id between the statements.
-- Each target row must match at most one source row, so de-duplicate staging first.
CREATE OR REPLACE FUNCTION merge_orders()
RETURNS VOID AS $$
BEGIN
    MERGE INTO orders o
    USING (SELECT * FROM staging_orders WHERE NOT is_processed) s
       ON o.order_id = s.order_id
    WHEN MATCHED AND o.source_hash <> s.source_hash THEN
        UPDATE SET
            customer_id  = s.customer_id,
            order_date   = s.order_date,
            total_amount = s.total_amount,
            status       = s.status,
            updated_at   = NOW(),
            source_hash  = s.source_hash,
            version      = o.version + 1
    WHEN NOT MATCHED THEN
        INSERT (order_id, customer_id, order_date, total_amount, status, source_system, source_hash)
        VALUES (s.order_id, s.customer_id, s.order_date, s.total_amount, s.status, s.source_system, s.source_hash);

    -- Mark staging records as processed
    UPDATE staging_orders SET is_processed = TRUE WHERE NOT is_processed;
END;
$$ LANGUAGE plpgsql;
-- For a simple upsert against ONE unique constraint, prefer INSERT ... ON CONFLICT
-- DO UPDATE: it has purpose-built INSERT-race handling that classic MERGE lacks.
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal, mechanism-level guidance. Most examples are PostgreSQL; the principles generalize. Verify version-specific features against your installed version.

### Keys, Types & Modeling

**Default `BIGINT GENERATED ALWAYS AS IDENTITY` for surrogate keys.** `INT4` (`SERIAL`/`serial`) caps at 2,147,483,647 and fails *hard* on `INSERT` at the limit — high-volume tables hit this in months, and the `BIGINT` sequence behind a `SERIAL` never overflows, so watching `max(id)` understates the risk; migrating `INT`→`BIGINT` forces a full-table rewrite under `ACCESS EXCLUSIVE`. `SERIAL` is also a non-standard pseudo-type with awkward sequence ownership/permissions/`pg_dump`. `IDENTITY` (SQL:2003, PG10+) binds the sequence to the column and blocks accidental manual overrides. Use `BY DEFAULT` only when importing explicit values.

```sql
CREATE TABLE events (id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name TEXT NOT NULL);
SELECT max(id)::float / 2147483647 AS pct_used FROM high_volume_table;  -- monitor legacy INT keys
```

**Prefer UUIDv7 over UUIDv4 when you need UUID keys.** UUIDv4 (`gen_random_uuid()`) is fully random, so every insert lands at an arbitrary B-tree leaf — frequent page splits, ~50-69% page fill vs ~90%+, larger indexes, lower insert throughput as the table grows. UUIDv7 (RFC 9562) puts a 48-bit millisecond timestamp in the high bits, so values append to the right of the index like a `BIGINT` while staying globally unique — the tradeoff is an embedded approximate creation time. PG18 ships built-in `uuidv7()`; PG14-17 use the `pg_uuidv7` extension. `BIGINT IDENTITY` is still fastest and smallest for pure sequential inserts — reach for UUIDv7 specifically when distributed/opaque/client-generated IDs are required.

```sql
CREATE TABLE orders (id UUID PRIMARY KEY DEFAULT uuidv7());  -- PG18+
```

**Always use `TIMESTAMPTZ` for real-world instants; never bare `TIMESTAMP`.** `TIMESTAMP` stores a wall-clock picture with no UTC anchor — any offset in the input literal is *silently discarded* (`'2024-01-15 12:00:00+05:30'::timestamp` stores `12:00:00`), so DST arithmetic and cross-client comparisons silently produce wrong answers. `TIMESTAMPTZ` converts to UTC on write and to the session zone on read; both are 8 bytes, so there is no storage reason to prefer `TIMESTAMP`. Related traps: `timestamp(N)` *rounds* fractional seconds — use `date_trunc('second', ...)` to truncate; and store IANA names (`'America/New_York'`), never fixed-offset abbreviations like `'EST'` that ignore DST. Also flag `money` (locale-dependent rounding, no fractional cents) and `char(n)` (space-padding, no perf benefit over `varchar`/`text`) as anti-patterns.

**Avoid EAV; reach for typed columns + a `JSONB` escape hatch.** Entity-Attribute-Value (`entity_id, attr_name TEXT, value TEXT`) looks schemaless but every value is `TEXT` (no numeric/date/enum enforcement), reading all attributes of an entity needs one self-join per attribute (defeating indexes and planner statistics), and a typo in `attr_name` silently creates a new "attribute" no constraint can catch. For dynamic attributes on an otherwise normalized table use a `JSONB` column (GIN indexing, containment operators, type-aware access) while fixed columns keep their relational guarantees. Reserve EAV for the rare case where attribute names themselves are unstructured and user-controlled.

```sql
CREATE TABLE products (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    price NUMERIC(10,2) NOT NULL CHECK (price >= 0),
    extra_attributes JSONB                       -- typed escape hatch
);
CREATE INDEX idx_products_attrs ON products USING GIN (extra_attributes);
```

**Normalize the system of record; derive denormalized read models.** *Designing Data-Intensive Applications* (Kleppmann) distinguishes the system of record (source of truth, each fact written once, normalized) from derived data (caches, indexes, materialized views, read models computed *from* it). This dissolves the false "normalize or denormalize" dilemma: normalize so each fact has one home, then DERIVE hot read models via materialized views, event-driven projections, or continuous aggregates — and let the derivation pipeline keep them consistent, not duplicated app-side writes. Storing a pre-computed total on `orders` *and* recomputing it in a view creates two sources of truth that drift.

### Indexing

**Index every foreign key column on the child side — PostgreSQL never does it automatically.** It auto-indexes the parent primary key but NOT the referencing child column. Two consequences: (1) joins from child to parent fall back to a sequential scan of the child; (2) every parent `DELETE`/`UPDATE` triggers a referential-integrity check that scans the child for referencing rows — once per affected parent row, so deleting N parents is O(N) seq scans (the same cliff hits `ON DELETE CASCADE`). Documented cases show 50k parent deletes dropping from ~30 minutes to ~100ms after adding the child index. (The RI check uses row-level `FOR KEY SHARE` locks on referenced rows, *not* a table-level lock on the child.) Exception: very small child tables where a seq scan beats an index scan.

```sql
CREATE INDEX idx_order_items_order_id   ON order_items(order_id);
CREATE INDEX idx_order_items_product_id ON order_items(product_id);
-- Find unindexed FKs / never-scanned indexes:
SELECT schemaname, relname AS table, indexrelname AS index, idx_scan
FROM pg_stat_user_indexes WHERE idx_scan = 0
ORDER BY pg_relation_size(indexrelid) DESC;
```

**Every index is a write tax; don't over-index write-heavy OLTP tables.** Each index is a separate B-tree updated on every `INSERT`/`UPDATE`/`DELETE`. Under MVCC an `UPDATE` writes a new row version and needs new index entries in every index covering the changed columns (HOT updates avoid this only when no indexed column changes and the page has room). Five indexes mean ~5x index write I/O, more WAL, faster bloat, and more autovacuum pressure — a loop where bloat slows scans and tempts yet more indexes. Before adding an index, ask whether a query can be restructured or an existing index extended; the planner usually uses only one index per scan anyway.

**Use `INCLUDE` for covering indexes; index-only scans depend on the visibility map.** An index-only scan answers a query entirely from the index when every referenced column (SELECT/WHERE/ORDER BY/GROUP BY) is present. `INCLUDE` (PG11+) stores payload columns only in leaf pages — they don't widen the tree, don't affect ordering, and don't participate in uniqueness, so `UNIQUE INDEX (user_id) INCLUDE (email)` enforces uniqueness on `user_id` alone. Critical dependency: index-only scans require the heap page's visibility-map bit to be set; on write-heavy tables with stale visibility maps the planner falls back to heap fetches. Verify the plan shows `Index Only Scan`, not `Index Scan`.

```sql
CREATE INDEX idx_users_lookup ON users(user_id) INCLUDE (email, status);
```

**Partial-index predicates match by simple implication at plan time — parameterized queries never use them.** A partial index indexes only rows satisfying a predicate (smaller, faster, cheaper to maintain). But the planner uses it only when it can prove *at plan time* that the query `WHERE` implies the index predicate — there is no theorem prover; it recognizes only simple inequality implications (`x < 1` ⇒ `x < 2`), otherwise the query predicate must *exactly* match part of the index predicate. Because matching is at plan time, a prepared `x < $1` can never be proven to imply `x < 2`, so partial indexes intended for hot subsets are *silently unused* under most ORMs — verify with `EXPLAIN (ANALYZE)` on the actual parameterized statement. Anti-pattern: many per-value partial indexes (one per status) instead of one composite `(status, ...)`. Useful idiom: partial `UNIQUE` for soft-delete uniqueness.

```sql
-- Usable only with a literal predicate:
CREATE INDEX idx_orders_pending ON orders(user_id, created_at) WHERE status = 'pending';
-- Soft-delete uniqueness: unique only among non-deleted rows
CREATE UNIQUE INDEX idx_users_email_active ON users(email) WHERE deleted_at IS NULL;
-- For parameterized ($1) access use a regular composite index instead.
```

**Mark functions `IMMUTABLE` only if truly immutable; config-dependent ones must be `STABLE`.** Expression indexes require `IMMUTABLE`, which promises identical output for identical input forever — no DB access, no config/session dependency. Mislabeling a config-dependent function (depends on `TimeZone`, `lc_collate`, `search_path`, `current_setting()`, or calls `now()`) lets the planner fold it to a constant at plan time and bake a *stale value* into cached/prepared plans, silently returning wrong results. A `timestamptz::date` cast is the classic offender (depends on `TimeZone`). Use `STABLE` for functions constant within one statement but session-dependent — they may appear in `WHERE` but cannot define an index expression.

```sql
CREATE FUNCTION lower_email(text) RETURNS text LANGUAGE sql IMMUTABLE STRICT AS $$ SELECT lower($1) $$;
CREATE INDEX idx_users_lower_email ON users(lower_email(email));
```

**GIN on JSONB: pick `jsonb_path_ops` for containment, disable `fastupdate` for steady latency.** (1) `jsonb_ops` (default) supports `@>`, `?`, `?|`, `?&`, etc.; `jsonb_path_ops` supports only `@>`/`@?`/`@@` but is smaller and faster — use it when queries are containment-only. Either way, expression access like `payload->>'type' = 'login'` is NOT GIN-indexable and silently seq-scans. (2) `fastupdate` (on by default) buffers entries in a pending list; inserts are fast until the list exceeds `gin_pending_list_limit`, then the next insert pays a synchronous cleanup — a P99/P999 latency spike invisible in averages, costly after bulk loads. Set `fastupdate = off` for write-heavy/bulk workloads.

```sql
CREATE INDEX idx_events_payload ON events USING GIN (payload jsonb_path_ops);
CREATE INDEX idx_docs_tags ON documents USING GIN (tags) WITH (fastupdate = off);
```

**BRIN is tiny on physically-correlated columns, useless on random data.** A Block Range INdex stores only a min/max summary per range of consecutive heap pages, so the whole index for a 100M-row table can be kilobytes; the planner skips ranges whose summary can't match. Hard requirement: the column must be well-correlated with physical row order. For append-only tables with a monotonic timestamp/ID (logs, events, metrics) summaries are tight and disjoint — excellent for range scans. For randomly ordered columns (e.g. `last_name` on OLTP) ranges overlap and BRIN degenerates to a full scan. It is not a substitute for B-tree point lookups — combine them (BRIN on time, B-tree on the lookup key).

```sql
CREATE INDEX idx_events_brin ON events USING BRIN (occurred_at);  -- append-only
```

**For pgvector in production prefer HNSW over IVFFlat.** IVFFlat must be built on already-populated data (it trains cluster lists) and gives a weaker recall/latency tradeoff. HNSW builds an incremental multi-layer graph, can be created on an empty table, and delivers better recall at a given latency — the recommended default for production/real-time-insert workloads. IVFFlat is fine for batch-rebuild pipelines where build speed beats incrementality. Tune `m`/`ef_construction` at build time and `hnsw.ef_search` at query time.

```sql
CREATE INDEX ON embeddings USING hnsw (embedding vector_cosine_ops) WITH (m = 16, ef_construction = 64);
SET hnsw.ef_search = 100;  -- higher = better recall, more latency
```

### Querying & Concurrency

**`NOT IN` with a nullable subquery silently returns zero rows — use `NOT EXISTS`.** SQL three-valued logic breaks `NOT IN`: `x NOT IN (a, b, NULL)` expands to `x <> a AND x <> b AND x <> NULL`; the last term is `UNKNOWN`, `UNKNOWN AND anything` is `UNKNOWN`, and `WHERE` discards it — so one NULL in the subquery empties the entire result set, with no error or warning. This is standard SQL, listed on the PostgreSQL wiki "Don't Do This" (and `NOT IN` can't be optimized into an anti-join). `NOT EXISTS` checks row existence rather than a comparison, handles NULLs correctly, and optimizes into an anti-join.

```sql
SELECT id FROM products p
WHERE NOT EXISTS (SELECT 1 FROM order_items oi WHERE oi.product_id = p.id);
```

**`READ COMMITTED` (the default) allows lost updates, vanishing `DELETE`s, and write skew.** Each *statement* (not transaction) gets a fresh snapshot. An `UPDATE`/`DELETE` finds rows committed as of statement start, but if a concurrent transaction already modified a target row, the `WHERE` is *re-evaluated against the new row version after locking* — so given `hits ∈ {9,10}`, one session's `UPDATE website SET hits = hits + 1` racing another's `DELETE FROM website WHERE hits = 10` can delete zero rows (the `10` became `11` before the `DELETE` locked it). Cross-row invariants (write skew) are not prevented by `READ COMMITTED` or `REPEATABLE READ`; only `SERIALIZABLE` detects them, aborting one tx with `SQLSTATE 40001` — so the app must retry the whole transaction. (A single `balance = balance + 100` statement is atomic and safe; the danger is read-then-write across statements and multi-row invariants.)

```sql
BEGIN ISOLATION LEVEL SERIALIZABLE;
SELECT sum(value) FROM accounts WHERE group_id = 1;
INSERT INTO accounts (group_id, value) VALUES (1, 50);
COMMIT;  -- may raise 40001; the app must retry the whole tx
```

**Use `MERGE` (PG15+) for multi-action sync; keep `ON CONFLICT` for single-constraint upserts.** `MERGE` does `WHEN MATCHED` (UPDATE/DELETE) and `WHEN NOT MATCHED` (INSERT) in one statement — the right tool for syncing a staging set (insert+update+delete in one pass), eliminating the race window of a hand-rolled two-pass function. Important: `INSERT ... ON CONFLICT DO UPDATE` is still better for a simple upsert against a single unique constraint because it has purpose-built INSERT-race handling; classic `MERGE` does NOT detect a concurrent INSERT and under `READ COMMITTED` can still raise a unique violation on its INSERT branch. Cardinality rule: each target row must match at most one source row, or `MERGE` errors; use stable source ordering to reduce deadlocks.

### Migrations & DDL (lock-aware)

**`ALTER TABLE` locks stack — a fast DDL can freeze a table behind one long query.** Most `ALTER TABLE` forms take `ACCESS EXCLUSIVE`; the trap is the lock *queue* — a long query holding even `ACCESS SHARE` makes the DDL wait, and every subsequent SELECT/INSERT/UPDATE/DELETE then queues behind the waiting DDL. So a 3-second DDL can stall all traffic for as long as the slow query runs, then unblock a thundering herd. Mitigate with a low `lock_timeout` (fail fast) plus retry with exponential backoff + jitter. Rewrite rules: a *constant* `DEFAULT` on `ADD COLUMN` is metadata-only on PG11+, but a *volatile* default (`gen_random_uuid()`, `now()`) and almost any column type change rewrite the whole table.

```sql
SET lock_timeout = '200ms';
ALTER TABLE orders ADD COLUMN tier VARCHAR(20) DEFAULT 'standard';  -- constant default: metadata-only on PG11+
```

**Add FK/CHECK/NOT NULL to large tables with `NOT VALID` + `VALIDATE CONSTRAINT`.** A plain `ADD CONSTRAINT` scans the whole table to validate existing rows (CHECK/NOT NULL hold `ACCESS EXCLUSIVE` for the scan). The two-phase pattern: (1) `ADD CONSTRAINT ... NOT VALID` enforces on new writes immediately with a brief lock and no scan; (2) `VALIDATE CONSTRAINT` scans only pre-existing rows under `SHARE UPDATE EXCLUSIVE`, which does NOT block concurrent DML. `NOT VALID` works for FK/CHECK on PG9.6+ and for NOT NULL on PG18+. A validated CHECK proving non-null lets a subsequent `SET NOT NULL` skip its scan. For UNIQUE, build with `CREATE UNIQUE INDEX CONCURRENTLY` then `ADD CONSTRAINT ... USING INDEX`.

```sql
ALTER TABLE orders ADD CONSTRAINT chk_amount_positive CHECK (total_amount > 0) NOT VALID;
ALTER TABLE orders VALIDATE CONSTRAINT chk_amount_positive;  -- concurrent DML allowed
```

**`CREATE INDEX CONCURRENTLY` can leave an INVALID index that still costs writes — and `IF NOT EXISTS` hides it.** On failure (lock timeout, unique violation, deadlock, cancellation) it leaves a `pg_index` row with `indisvalid = false`. The planner ignores an invalid index for reads (no benefit) but it still receives updates on every write (full write cost). `IF NOT EXISTS` makes a retry see the existing invalid name, skip creation, and return success while the dead index lingers. Detect via `pg_index WHERE NOT indisvalid`; recover with `DROP INDEX CONCURRENTLY` then rebuild, or `REINDEX INDEX CONCURRENTLY`.

```sql
SELECT indexrelid::regclass AS index FROM pg_index WHERE NOT indisvalid;
DROP INDEX CONCURRENTLY IF EXISTS idx_orders_status;
CREATE INDEX CONCURRENTLY idx_orders_status ON orders(status);
```

### Partitioning, Temporal & Generated Columns

**Use declarative partitioning (PG10+), not inheritance + trigger routing.** Pre-10 partitioning meant table inheritance plus hand-written INSERT triggers — slow, error-prone, and the parent silently accumulates unrouted rows if a trigger is missing. Declarative `PARTITION BY RANGE/LIST/HASH` automates routing, models bounds as real constraints (enabling plan-time partition pruning), and supports partition-wise joins/aggregates and `ATTACH`/`DETACH`. New projects should never use inheritance-based partitioning.

```sql
CREATE TABLE measurements (measured_at TIMESTAMPTZ NOT NULL, device_id UUID NOT NULL, value DOUBLE PRECISION NOT NULL)
    PARTITION BY RANGE (measured_at);
CREATE TABLE measurements_2025 PARTITION OF measurements FOR VALUES FROM ('2025-01-01') TO ('2026-01-01');
```

**PG18 generated columns are VIRTUAL by default — only STORED can be indexed.** Through PG17 generated columns were always STORED (computed on write, persisted). PG18 makes VIRTUAL the default (computed at read time, no storage). VIRTUAL suits write-heavy workloads (no write cost) but cannot be indexed; STORED suits read-heavy/expensive expressions because it can be indexed. Neither may reference other generated columns, subqueries, or non-immutable functions. On PG18, *omitting* `STORED` yields a VIRTUAL column, so any index on it errors — add `STORED` when you intend to index.

```sql
CREATE TABLE documents (
    body TEXT NOT NULL,
    tsv TSVECTOR GENERATED ALWAYS AS (to_tsvector('english', body)) STORED  -- STORED required to index
);
CREATE INDEX ON documents USING GIN (tsv);
```

**PG18 adds native temporal (non-overlapping) constraints — `WITHOUT OVERLAPS` / `PERIOD`.** PG18 adds temporal constraints over range types: `PRIMARY KEY`/`UNIQUE` gain `WITHOUT OVERLAPS`, and `FOREIGN KEY` uses `PERIOD` (not `WITHOUT OVERLAPS`). This is the canonical pattern for SCD Type 2 dimensions, price/rate history, and reservations, replacing the `btree_gist` + `EXCLUDE USING GIST (... WITH &&)` idiom (still required pre-18).

```sql
CREATE TABLE product_prices (
    product_id INT NOT NULL REFERENCES products(id),
    price_cents BIGINT NOT NULL,
    valid_period daterange NOT NULL,
    PRIMARY KEY (product_id, valid_period WITHOUT OVERLAPS)  -- PG18+
);
```

### Operational Gotchas (design with these in mind)

**Monitor transaction-ID wraparound — unfrozen XIDs eventually hard-stop all writes.** PostgreSQL uses a 32-bit XID with modulo-2^32 comparison; within ~3M XIDs of wraparound it refuses to assign new XIDs, halting all writes and DDL (only VACUUM and read-only queries run). VACUUM prevents this by freezing old rows; the common blockers are long-running transactions, abandoned prepared transactions, and stale replication slots holding `xmin`. Autovacuum handles freezing by default, but on high-churn systems monitor `age(datfrozenxid)` and alert with lead time.

```sql
SELECT datname, age(datfrozenxid) AS xid_age, 2000000000 - age(datfrozenxid) AS xids_remaining
FROM pg_database ORDER BY age(datfrozenxid) DESC;
```

**Bound WAL retention with `max_slot_wal_keep_size` when using logical replication / CDC.** A logical replication slot retains WAL until the subscriber confirms receipt. With the default `-1` (unlimited), a stalled or disconnected CDC consumer (Debezium, pglogical) accumulates WAL until the disk fills and the primary halts all writes. PG13+ lets you cap it (e.g. `50GB`); past the cap PostgreSQL invalidates the slot and reclaims WAL rather than filling the disk — losing that stream but keeping the primary alive. Anyone embedding CDC in a pipeline must set this and monitor `pg_replication_slots` for `active = false` and `wal_status` of `'extended'`/`'lost'`.

```sql
-- postgresql.conf
max_slot_wal_keep_size = 50GB
```

**MongoDB unbounded embedded arrays degrade well before the 16MB document limit.** The performance cliff comes far earlier than 16MB: a growing embedded array (comments, events, log lines) rewrites the entire document on each push, so write cost scales with document size, and a multikey index stores one entry per element (10k elements = 10k index entries to maintain). The result is progressively slower writes with no error. Beyond roughly a few hundred elements on frequently-updated documents, switch to the Subset pattern (embed the most recent N, store the rest in a sibling collection fetched via `$lookup`) or full referencing — trading some read latency for bounded write cost.
