---
name: mermaid-diagrams
description: Generate diagrams using Mermaid syntax. Use when creating flowcharts, sequence diagrams, state machines, architecture diagrams, ER diagrams, or any visual representation of processes, relationships, or systems.
---

# Mermaid Diagrams

## When to Use

Use Mermaid diagrams instead of ASCII art or text-based diagrams when visualizing:

- Workflows and processes (flowchart)
- API call sequences (sequenceDiagram)
- State machines (stateDiagram-v2)
- System architecture (graph TD/LR)
- Data relationships (erDiagram)
- Class hierarchies (classDiagram)
- Timelines and Gantt charts
- Git branching (gitGraph)

## Format

Always use fenced code blocks with `mermaid` language identifier:

```mermaid
graph TD
    A[Start] --> B{Decision}
    B -->|Yes| C[Action]
    B -->|No| D[End]
```

## Diagram Types

### Flowchart (graph)
```mermaid
graph TD
    A[Rectangle] --> B(Rounded)
    B --> C{Diamond}
    C -->|One| D[Result 1]
    C -->|Two| E[Result 2]
```

### Sequence Diagram
```mermaid
sequenceDiagram
    participant A as Client
    participant B as Server
    A->>B: Request
    B-->>A: Response
```

### State Diagram
```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Processing: start
    Processing --> Done: complete
    Done --> [*]
```

### Entity Relationship
```mermaid
erDiagram
    USER ||--o{ ORDER : places
    ORDER ||--|{ LINE-ITEM : contains
```

## Best Practices

1. **Use clear, descriptive labels** - Node text should be self-explanatory
2. **Keep diagrams focused** - Split complex flows into multiple diagrams
3. **Use subgraphs** to group related nodes:
   ```mermaid
   graph TD
       subgraph Frontend
           A[React] --> B[State]
       end
       subgraph Backend
           C[API] --> D[DB]
       end
       B --> C
   ```
4. **Choose appropriate direction**:
   - `TD` (top-down) for hierarchies and vertical flows
   - `LR` (left-right) for sequences and horizontal processes
5. **Use styling sparingly** - Let the default theme handle most styling
