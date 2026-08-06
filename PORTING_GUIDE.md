# HelixDB Explorer UI and connection porting guide

This document records the complete Explorer-style UI, connection, transport,
and graph-loading change set made to Helix Visualizer relative to commit
`21e55ef2c2278dc8d247147b8876d7b1f0f6f117`. It is intended for applying the
same work to another copy of the project.

The work has three connected parts:

1. Match the overall desktop UI and connection workflow of
   `helixdb-explorer` while retaining this project's HelixSQL, tables,
   inspector, schema browser, history, and custom graph renderer.
2. Speak the API used by the current `helixdb/enterprise-dev:latest` image and
   HelixDB Explorer: `POST /v1/query` with the flat, tagged dynamic-query wire
   format.
3. Make the top-level Graph workspace load the current graph automatically,
   instead of requiring the user to run a `GRAPH` query first.

No npm or Cargo dependencies were added.

## Resulting behavior

- Each app launch opens with an Explorer-style splash screen that animates a
  valid HelixSQL graph query before revealing a Continue action.
- The app opens in the Query workspace with a desktop-style top toolbar.
- Connection, Query, and Graph are primary toolbar destinations.
- Query and Graph are separate top-level workspaces. Results and Wire Format
  remain tabs inside Query.
- The connection editor is a modal with Local and Cloud modes.
- Local connections use separate host and **host port** fields. For Docker
  `6969:8080`, the value entered in the app is `6969`, not the container's
  internal port `8080`.
- A candidate connection is tested before it is saved.
- After connecting while Graph is open, the app automatically runs
  `GRAPH LIMIT 300` and renders the returned snapshot.
- Opening Graph later also loads the snapshot if the app is connected and no
  graph has been loaded yet.
- Graph has loading, retry, refresh, disconnected, and empty states.
- Running a manual `GRAPH` query still replaces the snapshot and opens Graph.
- Existing query tables, schema discovery, history, node/edge inspection,
  graph controls, graph caveats, theme support, and API-key handling remain.

## File inventory

| File | Porting action | What changed |
|---|---|---|
| `src/App.tsx` | Copy the whole file when possible | New Query/Graph workspace shell, Explorer-compatible transport selection, connection lifecycle, graph auto-load, refresh/retry states, lazy graph bundle, and inspector placement. |
| `src/ui/SplashScreen.tsx` | Add this new file | React splash lifecycle, valid tokenized HelixSQL preview, progress/ready states, keyboard handling, and exit transition. |
| `src/ui/SplashScreen.css` | Add this new file | Explorer-inspired splash composition, light/dark token colors, editor window, progress treatment, responsive sizing, and reduced-motion behavior. |
| `src/ui/ConnectionBar.tsx` | Copy the whole file when possible | Replaced the connection chip/dropdown with the Explorer-style toolbar and Local/Cloud modal. Added test, connect/reconnect, disconnect, validation, saved-key behavior, and programmatic modal opening. |
| `src/styles.css` | Copy the whole file | Added the full Explorer-style visual layer, tokens, toolbar, modal, graph workspace, responsive layouts, and reduced-motion handling. The new rules intentionally override portions of the earlier stylesheet. |
| `src/hql/legacy.ts` | Add this new file | Compiles parsed HelixSQL into the flat `/v1/query` dynamic-query format used by Explorer and current enterprise-dev images. |
| `src/results.ts` | Copy or merge the response-reader changes | Accepts Explorer `properties` envelopes and `$id`/`$label`/`$from`/`$to` fields while preserving the older response shapes. |
| `src/client.ts` | One-line functional edit | Browser proxy path changed from `/helix/v2/query` to `/helix/v1/query`. |
| `src-tauri/src/lib.rs` | Focused endpoint edit, then `cargo fmt` | Desktop endpoint changed from `/v2/query` to `/v1/query`; endpoint tests and comments were updated. Other visible diff is Rust formatting. |
| `src/hql/compiler.ts` | Comment-only edit | Clarifies that the SDK compiler remains the validation/mock representation; it is no longer the production transport JSON. |
| `test/legacy.test.ts` | Add this new file | Covers the `/v1` envelope and flat traversal steps. |
| `test/results.test.ts` | Merge the added cases | Covers Explorer envelopes, virtual identity precedence, graph rows, and label aggregation. |
| `README.md` | Merge documentation edits | Corrects the endpoint and explains the split between SDK validation output and Explorer-compatible production output. |

## Recommended porting order

Apply the changes in this order so each intermediate state is understandable:

1. Change the backend and browser proxy from `/v2/query` to `/v1/query`.
2. Add `src/hql/legacy.ts`.
3. Add the Explorer response normalization in `src/results.ts`.
4. Replace or merge `src/App.tsx` so application requests use the legacy
   transport JSON while retaining the SDK-produced `ResultShape`.
5. Replace `src/ui/ConnectionBar.tsx` and `src/styles.css` together. Their DOM
   class names and styles are tightly coupled.
6. Add `SplashScreen.tsx` and `SplashScreen.css`, then render the splash from
   `App.tsx` before the application shell.
7. Add the tests and documentation.
8. Run every verification command at the end of this guide.

If the other copy is still based on the commit named above, copying the whole
files marked that way is safer than manually reproducing individual hunks. If
the other copy has diverged, use the implementation details below to merge by
responsibility.

## 1. Transport and endpoint changes

### Desktop/Tauri transport

In `src-tauri/src/lib.rs`, change:

```rust
const QUERY_PATH_SEGMENTS: [&str; 2] = ["v1", "query"];
```

Update every endpoint expectation and comment from `/v2/query` to `/v1/query`.
Keep the existing endpoint behavior:

- A bare origin becomes `{origin}/v1/query`.
- A trailing slash does not create a doubled slash.
- A gateway path prefix is preserved.
- A scheme-less host is treated as HTTP.
- A URL already ending in `/v1/query` is not appended twice.

The Rust command surface did not change:

- `run_query` sends an already serialized request using the saved connection.
- `test_connection` sends the same probe using an unsaved candidate.
- `get_connection` never returns the API-key value, only `hasApiKey`.
- `set_connection` retains the API-key tri-state behavior: `undefined` keeps
  the saved key, an empty string clears it, and a non-empty string replaces it.
- `writerOnly` still adds `x-helix-require-writer: true`.
- A saved API key still produces `Authorization: Bearer <key>`.

### Browser transport

In `src/client.ts`, set:

```ts
const BROWSER_FALLBACK_URL = "/helix/v1/query";
```

The Vite proxy continues to remove `/helix` and forward to `HELIX_URL`, so the
actual request becomes `/v1/query` on the target.

Important browser-mode behavior: editing the host or port in the modal does
not retarget an already-running Vite proxy. Start Vite with the intended target:

```bash
HELIX_URL=http://127.0.0.1:6969 npm run dev
```

The desktop build does use the host/port entered in the modal directly.

## 2. Explorer-compatible wire compiler

Add `src/hql/legacy.ts`. The existing `compile(statement)` function remains in
place because it supplies two useful things:

- validation and summary information;
- the `ResultShape` that tells `readResult` how to decode each named return.

Production requests are now assembled as follows:

```ts
function compileForStatement(statement: Statement): ExecutableQuery {
  const compiled = compile(statement);
  return {
    ...compiled,
    transportJson: compileLegacy(statement, compiled.shape),
  };
}
```

`ExecutableQuery` is `CompiledQuery & { transportJson: string }`. The transport
calls `runQuery(compiled.transportJson)` rather than
`runQuery(compiled.request.toJsonString())`.

### `/v1/query` envelope

`compileLegacy` emits this top-level shape:

```json
{
  "request_type": "read",
  "query_name": "helix_visualizer_<statement kind>",
  "query": {
    "queries": [],
    "returns": []
  },
  "parameters": {}
}
```

Every named query uses:

```json
{
  "Query": {
    "name": "rows",
    "steps": [],
    "condition": null
  }
}
```

All requests remain read-only.

### Traversal mapping

The compatibility compiler maps the existing HelixSQL AST to flat tagged
steps:

| HelixSQL concept | `/v1` step |
|---|---|
| all nodes | `{ "N": "All" }` |
| nodes by id | `{ "N": { "Ids": [...] } }` |
| filtered nodes/edges | `NWhere` / `EWhere` |
| adjacent nodes | `Out`, `In`, `Both` |
| adjacent edges | `OutE`, `InE`, `BothE` |
| edge endpoints | `OutN`, `InN`, `OtherN` |
| distinct | `"Dedup"` |
| order/paging | `OrderBy`, `Skip`, `Limit` |
| node properties and identity | `{ "ValueMap": null }` |
| edge properties and identity | `"EdgeProperties"` |
| scalar count | `"Count"` |
| grouped count | `{ "GroupCount": "<field>" }` |

Conditions map to `And`, `Or`, `Not`, `Eq`, `Neq`, `Gt`, `Gte`, `Lt`, `Lte`,
`Between`, `IsIn`, `StartsWith`, `EndsWith`, `Contains`, `IsNull`, `IsNotNull`,
and `HasKey`. Literals are tagged as `String`, `I64`, `F64`, `Bool`, `Null`,
or the appropriate array type. Entity ids are converted to `number` only when
safe and remain `bigint` otherwise.

### Statement-specific behavior

- `SELECT`: applies the selection, paging, and a terminal of `ValueMap`,
  `EdgeProperties`, `Count`, or `GroupCount`.
- `GRAPH`: returns named `nodes` and `edges` queries. Edges fan out from the
  selected nodes with `BothE`, then deduplicate and apply the edge limit.
- `SHOW LABELS`: samples node labels with `GroupCount`. Edge labels are fetched
  by walking `N All -> OutE -> Dedup -> Limit -> EdgeProperties`, because the
  current API does not support `E All`/edge `GroupCount` in this path.
- `SHOW STATS`: returns node/edge counts plus label samples.
- `DESCRIBE NODE`: returns the entity, identity, incident edges, neighbours,
  and degree.
- `DESCRIBE EDGE`: returns the entity, identity, source node, and target node.

## 3. Response normalization

The `/v1` endpoint frequently returns entity rows inside:

```json
{ "properties": [/* rows */] }
```

Update `src/results.ts` so `asRows` unwraps that form in addition to the prior
bare arrays, one-row objects, and named response wrappers.

Normalize Explorer virtual fields as follows:

| Server field | UI field |
|---|---|
| `$id` | `id` |
| `$label` | `label` |
| `$from` | `source` |
| `$to` | `target` |

The graph decoder accepts both the existing compiler aliases and the Explorer
names. It also accepts `from`/`source` and `to`/`target` fallbacks for edge
endpoints.

Identity must win when stored properties collide with reserved names. For
example, `{ "$id": 7, "id": 999 }` is displayed and inspected as entity id
`7`, not `999`. Strip `$id`, `$label`, `$from`, and `$to` from the property bag
before presenting it to the inspector.

For edge-label sampling, the v1 response may contain one edge row per sampled
edge without a `count` field. `readGroupCount` now tallies repeated `$label`
values locally, then sorts by descending count and label.

All prior response shapes remain supported.

## 4. Application shell and state

`src/App.tsx` now separates global navigation from query-output tabs:

```ts
export type AppView = "query" | "graph";
type OutputTab = "results" | "wire";
```

The old Graph result tab was removed. The top toolbar controls `AppView`; Query
keeps Results and Wire Format beneath the editor.

### Query workspace

The Query view retains:

- schema and example sidebar;
- HelixSQL editor and keyboard execution;
- results and wire-format tabs;
- query history;
- inspector;
- automatic navigation to Results for non-graph results;
- automatic navigation to Graph for graph results.

Queries are blocked with a clear message until `status.kind === "connected"`.
Compilation still happens while typing so syntax errors appear before a request
is sent.

### Graph workspace

The Graph view occupies its own top-level layout and keeps the inspector on the
right. Its toolbar shows node/edge totals plus Refresh and Edit Graph Query.

The graph canvas is dynamically imported:

```ts
const GraphCanvas = lazy(() =>
  import("./graph/GraphCanvas").then((module) => ({
    default: module.GraphCanvas,
  })),
);
```

This keeps the canvas/layout code out of the initial Query bundle. Render it
inside `Suspense` with a lightweight fallback.

### Automatic graph loading

The automatic snapshot uses the existing safe starter query:

```ts
const INITIAL_QUERY = "GRAPH LIMIT 300";
```

Track:

- `graphLoading` for progress;
- `graphError` for the retry surface;
- `graphLoadAttempted` to prevent effect loops and duplicate requests;
- `graphLoadId` to ignore a response from an old connection or request.

The effect should load only when all of these are true:

- the active view is Graph;
- the status is connected;
- no graph is already present;
- no graph request is running;
- the current connection/view cycle has not already attempted a load.

`loadCurrentGraph` compiles `INITIAL_QUERY`, executes it, requires a graph
result, stores the graph and run metadata, and updates the connection latency.
Failures populate the Graph error state; transport failures also mark the
connection failed.

Refresh explicitly calls the same loader. A successful manual `GRAPH` query
sets the graph, clears graph errors, marks the automatic load as satisfied, and
opens Graph.

When a new connection is saved or the user disconnects:

- increment `graphLoadId` to invalidate in-flight work;
- clear the old graph, graph error, and loading state;
- reset `graphLoadAttempted`;
- clear schema/selection/run state on disconnect.

Disconnect is a session action. It does not delete the saved connection
configuration or saved API key, so the modal can be used to reconnect.

### Startup splash

`src/ui/SplashScreen.tsx` mirrors the structure of HelixDB Explorer's splash
without using its HQL write-query example. The preview is a valid query in this
app's read-only HelixSQL grammar:

```sql
GRAPH NODES:User
WHERE active = true
TRAVERSE OUT Follows
WITH name, email
ORDER BY name ASC
LIMIT 150 EDGE LIMIT 600
```

The preview distinguishes HelixSQL keywords, entities, labels, properties,
directions, operators, literals, numbers, and punctuation with separate token
classes. `SPLASH_QUERY` is assembled from the displayed tokens, and its test
passes that exact string through the real parser so the example cannot drift
away from the language.

`App` initializes `showSplash` to true and returns the splash before rendering
the application shell. Existing startup effects still run because the early
return occurs after all hooks. After a two-second minimum animation, Continue
receives keyboard focus; clicking it or pressing Enter runs the exit transition
and opens Query. Reduced-motion users get the complete query and ready state
without the extended animation.

Splash styling is isolated in `src/ui/SplashScreen.css`; it uses the app's
existing theme variables and does not add an image dependency. The logo is an
inline SVG, and the heavy graph canvas remains lazy-loaded.

## 5. Connection UI and lifecycle

`src/ui/ConnectionBar.tsx` now owns two surfaces:

1. A fixed desktop-style toolbar with Connection, Query, Graph, connection
   summary, and theme controls.
2. A modal `ConnectionDialog`.

The connection status union is:

```ts
type ConnectionStatus =
  | { kind: "disconnected" }
  | { kind: "checking" }
  | { kind: "connected"; durationMs: number }
  | { kind: "failed"; message: string };
```

### Local mode

- Defaults to host `127.0.0.1` and port `6969`.
- Treats `localhost` as `127.0.0.1` when constructing the URL.
- Displays Docker guidance explaining that `6969:8080` means host port 6969.
- Validates that host is present and port contains digits.

### Cloud mode

- Accepts a full instance URL and API key.
- Adds `https://` when no scheme is present.
- Removes trailing slashes.
- Requires a key only for a new cloud connection when no saved key exists.
- Leaving the key blank preserves an existing saved key.
- The checkbox sends an empty string to remove an existing key.

### Shared behavior

- Advanced Options contains request timeout and writer-node preference.
- Test Connection probes without saving.
- Connect first tests, then persists the candidate, updates status, and loads
  schema. If Graph is active, its effect then loads the graph.
- Reconnect uses the same verified-save path.
- Disconnect clears the live session state and closes the modal.
- Escape and backdrop-click close the modal only when no request is running.
- Inline success/error feedback is shown in the modal.
- An integer `openRequest` prop lets Graph's Connect Now button open the modal
  without moving modal ownership out of `ConnectionBar`.

Startup intentionally reads saved settings only. It does not silently make a
network request or mark the session connected. The user still chooses Connect.

## 6. Styling changes

The Explorer-style rules are in `src/styles.css`. Copy the complete stylesheet
unless you deliberately want to merge a different theme.

The changes include:

- expanded dark/light tokens for window, elevated surfaces, toolbar, warning,
  and overlay colors;
- a two-row desktop header with a draggable title region;
- icon-first navigation buttons and connection indicators;
- independent Query and Graph grid layouts;
- a full-height graph stage and right-side inspector;
- centered empty/loading/error states;
- a blurred modal backdrop and elevated connection dialog;
- Local/Cloud mode cards, form rows, feedback banners, and action footer;
- responsive behavior at 960px and 720px;
- reduced-motion overrides;
- light/dark theme parity.

At widths below 960px, the Query inspector is hidden and sidebars narrow. At
widths below 720px, Query's schema sidebar and Graph's inspector are hidden,
the main workspace becomes one column, and modal fields stack.

The existing canvas renderer, force layout, palette, tables, editor, inspector,
and sidebar components were not rewritten. The new shell styles them in place.

## 7. Tests added or updated

`test/legacy.test.ts` verifies:

- `/v1` read envelope and named returns;
- flat node selection/count steps;
- label filters, comparisons, ordering, skip, and limit;
- graph edge fan-out from the selected node set;
- edge-label sampling without unsupported `E All`;
- numeric id addressing for DESCRIBE.

`test/results.test.ts` adds coverage for:

- `{ properties: [...] }` row envelopes;
- Explorer virtual-field normalization;
- virtual identity winning over stored `id`/`label` properties;
- group counts from `$label` rows with and without explicit counts;
- graph nodes and edges using `$id`, `$label`, `$from`, and `$to`.

The Rust endpoint tests now expect `/v1/query` for bare origins, trailing
slashes, path prefixes, scheme-less hosts, and already-complete endpoint URLs.

`test/splash.test.ts` parses the exact tokenized splash query and verifies its
node label, traversal, projected properties, node limit, and edge limit.

## 8. Verification checklist

Run:

```bash
npm install
npm run typecheck
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

Then verify against a real instance. With Docker publishing `6969:8080`:

1. Start the HelixDB container and confirm host port `6969` is published.
2. Start the app and confirm the splash reveals all six HelixSQL lines.
3. Confirm Continue appears, supports Enter, and transitions into Query.
4. Open Graph before connecting; confirm Connect Now is shown.
5. Connect Local to `127.0.0.1`, host port `6969`.
6. Confirm schema labels load.
7. Confirm Graph renders automatically without visiting Query.
8. Click Refresh and confirm the canvas remains populated.
9. Run `GRAPH LIMIT 300` manually and confirm it opens Graph.
10. Select a node and edge and confirm the Inspector still works.
11. Run representative SELECT, GROUP BY, SHOW, and DESCRIBE queries.

The implementation was verified on the source project with:

- 142 passing Vitest tests;
- a passing TypeScript check and production Vite build;
- 8 passing Rust tests;
- a real `helixdb/enterprise-dev:latest` container at
  `127.0.0.1:6969`, where the automatic snapshot rendered 21 nodes and
  17 edges and Refresh returned the same graph.

## 9. Important limits and assumptions

- The automatic snapshot is deliberately bounded at 300 nodes. The normal
  graph edge limit remains 2,000. Use a manual Graph query for a different
  scope or limit.
- The current enterprise-dev/Explorer contract is `/v1/query` plus the flat
  tagged query format. If the other environment exposes a different API
  version, make the endpoint and compiler selectable rather than mixing wire
  formats in one request.
- The bundled mock server exercises the official SDK traversal representation,
  while the production app sends the Explorer-compatible representation. The
  compiler tests cover both paths separately.
- Browser preview always targets the `HELIX_URL` captured when Vite starts.
- Saved API keys remain stored by the Rust backend as before; the frontend only
  knows whether a key exists.
- Graph auto-load does not auto-connect on application launch.
- The splash appears once per application/page launch, matching Explorer; it
  is not suppressed permanently in local storage.

## 10. Quick transplant checklist

- [ ] Add `src/hql/legacy.ts`.
- [ ] Route production compilation through `compileLegacy` while retaining the
      SDK `ResultShape`.
- [ ] Change desktop and browser paths to `/v1/query`.
- [ ] Normalize Explorer response envelopes and virtual identity fields.
- [ ] Replace `App.tsx`, `ConnectionBar.tsx`, and `styles.css` as a coordinated
      UI change.
- [ ] Preserve the other copy's unrelated features while merging.
- [ ] Add both compatibility test groups.
- [ ] Update user-facing endpoint and Docker-port documentation.
- [ ] Run frontend tests, typecheck, build, and Rust tests.
- [ ] Exercise Connect Now -> Connect -> automatic Graph render against a real
      instance.
