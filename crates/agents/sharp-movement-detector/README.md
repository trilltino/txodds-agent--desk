# crates/agents/sharp-movement-detector/

**Role:** `risk` — detects and scores sharp-money movements.

## Strategy

Monitors betting-exchange volume patterns and odds velocity to identify sharp-money entries. Provides risk-adjusted confidence ratings that the `match-intelligence` agent uses as a tool call input.

## Output

Returns a `SharpSignal` struct:

```rust
pub struct SharpSignal {
    pub fixture_id: u32,
    pub outcome: String,        // "home" | "draw" | "away"
    pub direction: String,      // "FOR" | "AGAINST"
    pub magnitude: f64,         // 0–1, strength of the movement
    pub confidence: f64,        // 0–1
    pub ts: String,
}
```

## Running tests

```bash
cargo test -p sharp-movement-detector
```
