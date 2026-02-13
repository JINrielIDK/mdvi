# TODO

- [x] **P1** Cache display height in `max_scroll` to avoid rebuilding the full display doc on frequent navigation/redraw paths (`src/app.rs:195`).
  - Cache virtual row count keyed by width and invalidate on content/search/image layout changes.
- [x] **P2** Cache search-highlighted lines so redraws do not re-run regex highlighting across the whole document when query/match state is unchanged (`src/app.rs:300`).
- [x] **P2** Remove per-frame full-document clone before paragraph render by transferring ownership of existing rendered lines into `Text` (`src/app.rs:570`).
- [x] **P2** Move local image dimension probing off the UI thread during load/reload (defer to background or viewport-triggered requests) to reduce startup/input latency (`src/app.rs:860`).
