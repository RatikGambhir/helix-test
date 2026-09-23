# Rust application-core porting guide

This document records the current Helix Visualizer architecture and the changes
needed to port it to another copy of the project. The important boundary is:

> React sends user intent and renders view models. Rust owns HQL, networking,
> response interpretation, schema discovery, and connection persistence.

There is deliberately no browser implementation of the query pipeline.

## Resulting behavior

- The Explorer-style splash, Query, Schema, and Graph workspaces remain React.
- The editor sends HQL source to Rust for live validation and wire preview.
- Running a query sends source—not an AST or serialized traversal—over IPC.
- Rust lexes, parses, validates, compiles, posts to `/v1/query`, checks the HTTP
  response, decodes its shape, and returns a typed result view model.
- Rust discovers labels, samples properties, infers field metadata, and builds
  the starter queries shown in the schema UI.
- Rust owns URL normalization, timeouts, bearer credentials, writer routing,
  and persisted connection settings.
- Entity ids and integers outside JavaScript's safe range cross IPC as strings,
  so the webview cannot round an `i64`.
- The frontend owns only view state, request lifecycle state, selection, theme,
  layout, and rendering.

## File inventory

| File | Responsibility |
|---|---|
| `src-tauri/src/hql.rs` | HQL lexer, parser, AST, semantic validation, v1 wire compiler, compiler tests |
| `src-tauri/src/results.rs` | JSON parsing, response-envelope handling, result normalization, i64-safe IPC models |
| `src-tauri/src/schema.rs` | Schema field inference and safe starter-query generation |
| `src-tauri/src/lib.rs` | Tauri commands, HTTP transport, connection persistence, execution and schema orchestration |
| `src/client.ts` | Thin typed wrappers around Tauri `invoke` plus structured error conversion |
| `src/results.ts` | TypeScript result contracts and display-only string formatting |
| `src/schema.ts` | TypeScript schema contracts and display-only filtering/formatting |
| `src/App.tsx` | UI state and calls to the Rust commands |
| `tools/mock-helix-server.mjs` | Development stand-in for the flat `/v1/query` protocol |

The old `src/hql/` TypeScript implementation and frontend response reader are
removed. Do not restore them as fallbacks: two implementations will drift.

## IPC contract

### `compile_query`

Input:

```json
{ "source": "SELECT * FROM NODES LIMIT 25" }
```

Output:

```json
{
  "transportJson": "{ ... }",
  "summary": "all properties of nodes"
}
```

HQL failures are serialized with `kind`, `message`, `span`, and `hint`, allowing
the React editor to select the offending token without parsing anything.

### `run_query`

Input is the same HQL `source`. Output contains:

- `compiled`: the backend-produced wire preview and summary;
- `durationMs`: HTTP duration;
- `result`: a tagged rows/count/groupCount/graph/labels/stats/describe model.

The command recompiles source even if the editor already validated it. Live
validation is a user-experience feature; backend validation remains authoritative.

### `test_connection`

Input is a candidate connection only. Rust constructs and executes the fixed,
schema-independent probe internally. The frontend cannot supply arbitrary probe
JSON.

### `load_schema`

Takes no query input. Rust performs label discovery, property sampling, field
inference, and safe label quoting, then returns the complete schema model.
Per-label sampling failures are attached to that label instead of discarding the
rest of the schema.

### Connection commands

- `get_connection` returns key presence, never the key.
- `set_connection` validates the URL before saving.
- An omitted API key reuses the stored key only for the same effective endpoint.
- An empty key clears it; a non-empty key replaces it.
- Changing hosts or gateway prefixes cannot forward an old credential.

## Rust HQL pipeline

The Rust module supports these read-only statement families:

- `SELECT`, including projections, `COUNT(*)`, `GROUP BY`, conditions,
  traversal, ordering, distinct, skip, and limit;
- `QUERY` (`GRAPH` remains a compatibility alias), including node/edge caps,
  `VIA`, requested properties, and traversal;
- `SHOW LABELS`, node/edge label variants, and `SHOW STATS`;
- `DESCRIBE NODE` and `DESCRIBE EDGE`.

Compilation always emits a read request in the Explorer-compatible flat format:

```json
{
  "request_type": "read",
  "query_name": "helix_visualizer_select",
  "query": {
    "queries": [
      { "Query": { "name": "rows", "steps": [], "condition": null } }
    ],
    "returns": ["rows"]
  },
  "parameters": {}
}
```

Traversal compatibility checks and unsupported `LIKE`/`NULL` forms fail before
HTTP. Integer literals are parsed directly as signed 64-bit integers and never
pass through a JavaScript number.

## Response and schema rules

The decoder accepts named results, `data`/`result`/`results` wrappers, bare
single-variable arrays, and Explorer `properties` envelopes. It:

- normalizes `$id`, `$label`, `$from`, and `$to` for display;
- prevents stored `id`/`label` properties from shadowing entity identity;
- deduplicates graph entities and excludes dangling edges;
- records truncation and dangling-edge counts;
- supports scalar, object, map, and sampled-row count shapes;
- separates entity properties, identity, relationships, and endpoints;
- converts out-of-range integer properties to exact decimal strings before IPC.

Schema labels include backend-generated `browseQuery` and `graphQuery` strings,
so React never needs to quote identifiers or assemble HQL.

## Connection and security behavior

- Base origins, gateway prefixes, and already-complete `/v1/query` URLs work.
- Scheme-less values default to HTTP; only HTTP and HTTPS are accepted.
- Timeouts are clamped to 1–600 seconds.
- API keys are added only as bearer headers and never returned through IPC.
- The config file is narrowed to mode `0600` on Unix.
- The Tauri transport is not subject to browser CORS restrictions.
- `npm run dev` is for rendering/styling only; use `npm run app` for functional
  query and connection development.

## Verification

Run all of the following after porting:

```bash
npm test
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

For a runtime smoke test:

```bash
npm run mock
npm run app
```

Connect to `http://127.0.0.1:6969`, then verify:

1. `SELECT * FROM NODES LIMIT 5` returns a table.
2. `QUERY LIMIT 25 EDGE LIMIT 90` renders a graph.
3. Schema refresh returns labels and sampled fields.
4. Selecting a node and edge loads the Inspector.
5. The Wire format tab matches the JSON compiled by Rust.

## Porting checklist

- [ ] Copy the three Rust domain modules and register them from `lib.rs`.
- [ ] Replace serialized-query commands with source-in/result-out commands.
- [ ] Preserve the connection credential and endpoint protections.
- [ ] Replace `src/client.ts` with typed IPC wrappers.
- [ ] Remove the TypeScript HQL/compiler/result-reader implementation.
- [ ] Consume schema-provided starter queries instead of assembling HQL in UI.
- [ ] Move language/decoder/schema tests to Rust.
- [ ] Remove the frontend Helix SDK dependency.
- [ ] Update the mock server to accept `/v1/query`.
- [ ] Run every verification command above.
