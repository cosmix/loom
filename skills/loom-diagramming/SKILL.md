---
name: loom-diagramming
description: Create technical diagrams using Mermaid syntax for architecture, sequences, ERDs, flowcharts, and state machines. Use for visualizing system design, data flows, C4 models, and process diagrams in documentation.
triggers:
  - diagram
  - diagrams
  - mermaid
  - plantuml
  - draw.io
  - excalidraw
  - flowchart
  - sequence diagram
  - class diagram
  - architecture diagram
  - ERD
  - entity relationship
  - entity-relationship
  - C4
  - C4 model
  - system context
  - container diagram
  - component diagram
  - state diagram
  - state machine
  - visualize
  - draw
  - chart
  - flow
  - data flow
  - API flow
  - system design
  - architecture visualization
  - UML
---

# Diagramming

## Overview

Create maintainable technical diagrams in Mermaid (renders in GitHub, GitLab, and most doc tools). Diagrams live in version control next to code — they must stay in sync or they mislead. This skill covers type selection, syntax, and the parsing footguns that waste the most time.

## Choose the diagram type

| Type            | Use when                                        | Not when                                 |
| --------------- | ----------------------------------------------- | ---------------------------------------- |
| Sequence        | Interactions **over time** across participants  | Showing static structure                 |
| Flowchart       | Decision logic, process/pipeline steps          | Timing between services (use sequence)   |
| State           | An entity's lifecycle + transitions/guards      | Data flow or call order                  |
| ERD             | Data model, tables, cardinality                 | Runtime behavior                         |
| Class           | OO structure, interfaces, inheritance           | Deployment or infra                      |
| C4 (Context)    | System boundary + external actors/systems       | Internal code detail                     |
| C4 (Container)  | Deployable units + their tech + data stores     | Class-level detail                       |

Rule of thumb: **structure → flowchart/C4/class/ERD; behavior over time → sequence; lifecycle → state.** For architecture, prefer C4's layered zoom (Context → Container → Component) over one sprawling diagram.

## Gotchas (Mermaid parsing footguns)

These cause silent render failures or garbled output far more often than logic errors.

- **`end` is reserved in flowcharts.** A lowercase node id `end` breaks the parser. Use `End`, `END`, or quote it: `id["end"]`. Same care with `subgraph`/`click`/`class` as bare ids.
- **Quote labels with special characters.** Parentheses `()`, `{}`, `[]`, `#`, `:`, `;`, and quotes inside a label break parsing — wrap the text: `A["fetch(url)"]`, `B["step: parse"]`. For literal special chars inside a quoted label, use HTML entities: `#quot;` won't work — use `&quot;`, `&amp;`, `&#35;` (for `#`).
- **Line breaks:** `<br/>` inside a label, not `\n`: `A["Line one<br/>Line two"]`.
- **Edge labels** with special chars must be quoted: `A -->|"retry (max 3)"| B`.
- **Leading `o`/`x` on an edge become arrowheads.** `A---oB` renders a circle end, `A---xB` a cross. Add a space (`A --- oB` is still risky) or rename the node so an edge doesn't touch a bare `o`/`x`.
- **Subgraph direction:** set `direction LR` *inside* the subgraph; note that edges crossing subgraph boundaries can override a subgraph's internal direction.
- **Comments** are `%%` on their own line. **Semicolons** are optional line terminators.
- **C4 diagrams** (`C4Context`/`C4Container`) have limited, sometimes experimental layout control and lag other Mermaid features — verify they render in your target tool before committing to them; a plain `flowchart` with subgraphs is a robust fallback.
- **Don't build the giant diagram.** Past ~15-20 nodes it's unreadable and un-reviewable. Split by concern or zoom level. One diagram, one idea.

## Keep diagrams in sync

Diagrams are code artifacts: update the diagram in the same PR as the code it depicts, review it in the diff, and prefer a diagram that's easy to regenerate over a pixel-perfect one that rots. A wrong diagram is worse than none.

## Syntax quick reference

- **Direction:** `flowchart TB` (top-bottom), `LR` (left-right). Sequence diagrams auto-layout.
- **Node shapes:** `[Rect]` process · `(Rounded)` start/end · `{Diamond}` decision · `[(DB)]` store · `((Circle))` connector.
- **Edges:** `-->` solid · `-.->` dotted/optional · `==>` emphasis · `->>` (sequence) sync message · `-->>` async/return.
- **Sequence keywords:** `participant`, `autonumber`, `alt/else/end`, `loop/end`, `par/and/end`, `Note over A,B`.

## Examples

### C4 Context (Level 1)

```mermaid
C4Context
    title System Context — E-Commerce Platform
    Person(customer, "Customer", "Browses and purchases")
    System(ecommerce, "E-Commerce Platform", "Core system")
    System_Ext(payment, "Payment Gateway", "Processes payments")
    Rel(customer, ecommerce, "Uses")
    Rel(ecommerce, payment, "Charges", "HTTPS")
```

### C4 Container (Level 2)

```mermaid
C4Container
    title Containers — E-Commerce Platform
    Person(customer, "Customer")
    Container_Boundary(ec, "E-Commerce Platform") {
        Container(web, "Web App", "React", "Customer UI")
        Container(api, "API Gateway", "Node.js", "REST API")
        ContainerDb(db, "Database", "PostgreSQL", "Stores data")
        ContainerQueue(queue, "Queue", "RabbitMQ", "Async events")
    }
    Rel(customer, web, "Uses", "HTTPS")
    Rel(web, api, "Calls", "JSON/HTTPS")
    Rel(api, db, "Reads/writes")
    Rel(api, queue, "Publishes")
```

### Sequence — request flow with conditionals

```mermaid
sequenceDiagram
    autonumber
    participant C as Client
    participant S as Service
    participant D as Database
    C->>S: POST /api/users
    S->>S: Validate input
    alt Validation fails
        S-->>C: 400 Bad Request
    else Valid
        S->>D: INSERT user
        alt Constraint violation
            D-->>S: Duplicate key
            S-->>C: 409 Conflict
        else Success
            D-->>S: Created
            S-->>C: 201 Created
        end
    end
```

### Sequence — parallel work

```mermaid
sequenceDiagram
    participant O as Order Service
    participant I as Inventory
    participant P as Payment
    par Check inventory and authorize payment
        O->>I: Check stock
        I-->>O: Available
    and
        O->>P: Authorize
        P-->>O: Authorized
    end
    O->>P: Capture payment
```

### Flowchart — decision logic

```mermaid
flowchart TD
    A[User Login] --> B{Valid credentials?}
    B -->|No| D[Show error]
    D --> A
    B -->|Yes| C{2FA enabled?}
    C -->|No| G[Create session]
    C -->|Yes| E[Send 2FA code]
    E --> F{Code valid?}
    F -->|Yes| G
    F -->|No| H{Attempts < 3?}
    H -->|Yes| E
    H -->|No| I[Lock account]
    G --> J[Dashboard]
```

### ERD

Cardinality: `||` exactly one · `o|` zero-or-one · `}|` one-or-more · `}o` zero-or-more (left char = min, right char = max, read toward the entity).

```mermaid
erDiagram
    USER ||--o{ ORDER : places
    ORDER ||--|{ ORDER_ITEM : contains
    ORDER_ITEM }|--|| PRODUCT : references
    USER {
        uuid id PK
        string email UK
        timestamp created_at
    }
    ORDER {
        uuid id PK
        uuid user_id FK
        string status
    }
```

### State machine

```mermaid
stateDiagram-v2
    [*] --> Draft: create
    Draft --> Pending: submit
    Pending --> Confirmed: payment received
    Pending --> Cancelled: payment failed / timeout
    Confirmed --> Shipped: fulfill
    Shipped --> Delivered: confirmed
    Delivered --> [*]
    note right of Pending
        Auto-cancels after 24h
    end note
```

### Class diagram

```mermaid
classDiagram
    class Repository~T~ {
        <<interface>>
        +findById(id) T
        +save(entity: T) T
    }
    class UserRepository {
        -db: Database
        +findByEmail(email) User
    }
    Repository~T~ <|.. UserRepository
    UserRepository --> User
```

## Verify before done

- [ ] Diagram renders in the target tool (GitHub/GitLab/docs), not just a local previewer — especially for C4.
- [ ] Labels with `()`, `#`, `:`, `,`, or quotes are wrapped in `"…"`; no bare `end` node id in a flowchart.
- [ ] Diagram type matches intent (behavior→sequence, structure→flowchart/C4, lifecycle→state).
- [ ] ≤~20 nodes; split otherwise. One diagram, one concept.
- [ ] Matches current code; updated in the same PR as the change it depicts.
