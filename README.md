# Helix Visualizer

A Tauri desktop app for exploring a [HelixDB](https://github.com/HelixDB/helix-db)
instance: type SQL-like queries to pull back nodes, edges and their
relationships, and draw the graph structure as a whole.

See [PORTING_GUIDE.md](PORTING_GUIDE.md) for the complete Explorer-style UI,
connection, `/v1/query`, and automatic graph-loading change log.

![the graph view](docs/graph-view.png)

## What it does

- **Welcome splash** — an Explorer-style startup animation previews a valid,
  read-only HelixSQL graph query before entering the workspace.
- **HelixSQL** — a small, read-only SQL dialect (`SELECT … FROM NODES … WHERE …
  TRAVERSE OUT …`) that compiles to HelixDB's JSON traversal AST. Full grammar
  below.
- **Graph view** — connecting from the Graph workspace automatically loads a
  bounded snapshot; a manual `QUERY` statement can replace it. The force-directed
  canvas supports pan, zoom, drag, neighbourhood focus on hover, and per-label
  colouring.
- **Inspector** — click a node or edge to see its properties, its incident edges
  with direction, its neighbours and its degree.
- **Schema sidebar** — node and edge labels discovered from the instance, each
  one a click away from a query.
- **Wire format tab** — the exact Explorer-compatible JSON sent to
  `POST /v1/query`, so nothing about the translation is hidden.
- **Design system** — warm graphite chrome in light and dark themes, so the
  label colours on the canvas and in charts carry all the meaning. One signal
  accent marks the primary action, the active workspace, focus and selection.
  Tokens live at the top of `src/styles.css`; Radix-backed primitives in
  `src/components/ui` use them through Tailwind.
- **Typography** — IBM Plex Sans for the interface and IBM Plex Mono for the
  HelixSQL editor, ids, numbers and wire-format surfaces, both bundled.
- **Responsive panes** — on narrower windows the Library and Inspector become
  drawers, toggled from the top bar and dismissed with `Esc`.

Queries are read-only by construction: the compiler only ever emits a `read`
batch, and there is no syntax for writes. Use a HelixDB SDK for those.

## Requirements

- Node.js 20+
- Rust 1.77+
- A running HelixDB instance (`helix start dev` listens on `localhost:6969`),
  or the bundled mock server below.
- Linux only: the usual Tauri v2 system dependencies —
  `libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev`, `libjavascriptcoregtk-4.1-dev`,
  `librsvg2-dev`, `build-essential`, `libssl-dev`, `pkg-config`.

## Running it

```bash
npm install
npm run app          # tauri dev — builds the Rust backend and opens the window
```

Set the instance from Connection in the top toolbar. It is saved to the app
config directory and prefilled on the next launch. For a Docker mapping such as
`6969:8080`, enter the host-side port `6969`.

The desktop app can connect directly to all of these:

- a local instance such as `http://127.0.0.1:6969`;
- a HelixDB server elsewhere on your network such as `http://192.168.1.20:6969`;
- an HTTPS cloud deployment such as `https://helix.example.com`;
- a deployment behind a gateway path such as `https://example.com/helix`.

Choose **Remote / Cloud** for a full URL. API keys are optional and are sent as
`Authorization: Bearer <key>` only when supplied. Remote URLs default to HTTPS
when the scheme is omitted. The desktop Rust transport is not subject to browser
CORS rules. A saved key is reused only while reconnecting to the same effective
endpoint, so changing the host or gateway path cannot forward an old credential.

To build a distributable:

```bash
npm run app:build
```

The repo ships PNG icons only. For `.ico`/`.icns` (Windows and macOS bundles),
run `npm run tauri icon src-tauri/icons/icon.png` once.

### Without a HelixDB instance

`tools/mock-helix-server.mjs` exercises the Explorer-compatible `/v1/query`
format over a small in-memory sample graph (users, posts, topics, orgs). It
interprets the subset of traversal steps this app emits — it is a development
stand-in, **not** a HelixDB implementation.

```bash
npm run mock                       # listens on :6969
npm run app                        # point the app at http://localhost:6969
```

### Frontend-only development

`npm run dev` can still render the UI shell for styling work, but queries and
connection actions require the Rust process. Use `npm run app` for functional
development. This is intentional: there is one parser, compiler, transport,
and response decoder, all in Rust, instead of a separate browser implementation.

```bash
npm run dev                        # http://localhost:14237
```

## HelixSQL

HelixDB has no SQL dialect of its own; queries are built as a JSON traversal
AST. HelixSQL is a thin, Rust-implemented language over that AST. The backend
lexes, parses, validates, and compiles source directly to the flat
Explorer-compatible format accepted by the current enterprise-dev `/v1/query`
endpoint.

Keywords are case-insensitive; labels and property names are not.

### SELECT

```
SELECT [DISTINCT] ( * | COUNT(*) | <column> [, …] )
FROM ( NODES | EDGES ) [ :<Label> ]
[ WHERE <condition> ]
[ TRAVERSE <direction> [<EdgeLabel>] [WHERE <condition>] ]…
[ GROUP BY <column> ]
[ ORDER BY <column> [ASC|DESC] [, …] ]
[ SKIP <n> ] [ LIMIT <n> ]
```

Without a `LIMIT`, 500 rows are fetched.

```sql
SELECT * FROM NODES:User LIMIT 25
SELECT id, name, age FROM NODES:User WHERE age BETWEEN 25 AND 40 ORDER BY age DESC
SELECT COUNT(*) FROM NODES GROUP BY label
SELECT id, label, source, target FROM EDGES:Follows LIMIT 50
```

**Virtual columns.** `id` and `label` work on both nodes and edges; `source` and
`target` are an edge's endpoints. `score` and `distance` are available where
HelixDB populates them. A reserved field can also be written out in full
(`$id`, `$label`, `$from.$id`, `$to.$id`). To read a *property* actually named
`id`, quote it: `SELECT "id" FROM NODES`.

`WHERE label = 'User'` and `FROM NODES:User` compile to the same thing — a label
match on the source step, so HelixDB can use its label index.

**Conditions.** `= != <> > >= < <=`, `AND`, `OR`, `NOT`, parentheses, `IN (…)`,
`BETWEEN … AND …`, `IS [NOT] NULL`, `HAS(<property>)`, and `LIKE` with `%` at
either end (`'ali%'`, `'%son'`, `'%li%'`). `_` is not supported.

**Traversal directions.**

| Direction | Starts from | Lands on |
|---|---|---|
| `OUT` / `IN` / `BOTH` | node | neighbouring nodes |
| `OUT EDGES` / `IN EDGES` / `BOTH EDGES` | node | the edges themselves |
| `SOURCE` / `TARGET` / `OTHER` | edge | the edge's endpoints |

```sql
-- who does Alice follow, and who do they follow?
SELECT DISTINCT id, name
FROM NODES:User WHERE name = 'Alice'
TRAVERSE OUT Follows
TRAVERSE OUT Follows

-- read a property stored on the relationship itself
SELECT label, source, target, since
FROM NODES:User
TRAVERSE OUT EDGES Follows WHERE since > 2022
```

Using a hop from the wrong side (`FROM EDGES … TRAVERSE OUT`) is rejected before
anything is sent, with the offending clause pointed at.

### QUERY

Returns nodes plus the edges among them, and draws them.

```
QUERY [ [FROM] NODES[:<Label>] ]
[ WHERE <condition> ] [ TRAVERSE … ]…
[ VIA <EdgeLabel> ] [ WITH <property> [, …] ]
[ ORDER BY … ] [ SKIP <n> ] [ LIMIT <n> ] [ EDGE LIMIT <n> ]
```

`LIMIT` caps nodes (default 400), `EDGE LIMIT` caps edges (default 2000), `VIA`
restricts which edge label is drawn, and `WITH` fetches node properties so the
canvas can label nodes by name rather than by type.

Edges are always fetched by walking out from the nodes that were selected, so
they belong to the part of the graph on screen. On a graph larger than `LIMIT`
some of them will still lead to nodes that were not fetched; those cannot be
drawn, and the view reports how many rather than dropping them silently.

```sql
QUERY LIMIT 300
QUERY NODES:User VIA Follows WITH name LIMIT 150
QUERY NODES WHERE id = 42 TRAVERSE BOTH LIMIT 200
```

`GRAPH` is still accepted as a compatibility alias for saved queries, but new
queries and app-generated examples use `QUERY`.

Only edges whose *both* endpoints are in the fetched node set can be drawn. Any
that leave the selection are counted and reported under the canvas rather than
silently dropped.

### SHOW and DESCRIBE

```sql
SHOW LABELS [SAMPLE <n>]     -- also: SHOW NODE LABELS, SHOW EDGE LABELS
SHOW STATS                   -- node/edge totals plus label breakdowns
DESCRIBE NODE <id> [LIMIT <n>]
DESCRIBE EDGE <id>
```

HelixDB has no catalog to query, so labels are derived by counting `$label` over
a bounded sample (5000 entities by default). Rare labels can be missed; the
sample size is stated wherever labels are shown.

### Keyboard and mouse

| | |
|---|---|
| `Ctrl`/`Cmd` + `Enter` | run the query |
| double-click a node | load its neighbourhood into the editor |
| drag a node | move it to a clearer position |
| scroll | zoom about the cursor |

## How it fits together

```
src/              React UI, typed IPC client, and presentation-only helpers
src/graph/        force layout (Barnes–Hut) + React Flow renderer + colour assignment
src/ui/           editor, results table, inspector, sidebar, connection bar
src-tauri/src/hql.rs      lexer → parser → AST → validator → wire compiler
src-tauri/src/results.rs  Helix response decoder → UI-facing view models
src-tauri/src/schema.rs   schema query generation and property inference
src-tauri/src/lib.rs      commands, connection state, HTTP transport, orchestration
tools/            mock HelixDB server, icon generator
```

The webview never parses HQL or talks to HelixDB directly. It hands source text
to the Rust command `run_query`; Rust compiles it, posts it to `{url}/v1/query`,
and returns a decoded view model. That keeps the app clear of webview CORS rules, lets an `https`-origin webview reach a
plain-HTTP local instance, and keeps the API key in the backend's config file
rather than in webview storage. That file is written to the platform config
directory as `connection.json`, narrowed to `0600` on Unix; the key is stored in
plain text, so it is protected by file permissions rather than by a keychain.

Entity ids are `i64`, which JavaScript cannot hold exactly in a `number`. Rust
parses them exactly, exposes entity ids as strings, and converts any other
out-of-range integer to a string before IPC, so no value silently rounds in the
webview.

## Tests

```bash
npm test                    # presentation, graph-layout, and UI helper tests
npm run typecheck
cargo test --manifest-path src-tauri/Cargo.toml
```

Rust tests cover HQL tokenization, parsing, semantic validation, v1 compilation,
response envelopes, graph normalization, i64 preservation, and schema
inference. Frontend tests cover the remaining presentation behavior.

## Known limits

- Read-only. There is no `INSERT`/`UPDATE`/`DELETE` syntax.
- `VIA` takes a single edge label, not a list.
- `LIKE` supports `%` only at the start and/or end of the pattern.
- `SELECT *` runs the selection twice inside one batch — once for the stored
  properties and once for `$id`/`$label`, since a traversal has only one
  terminal — and merges the two positionally. If they ever come back with
  different lengths the properties are shown without ids rather than
  mislabelled. Name the columns explicitly to avoid the second pass.
- Sorting a results table sorts the rows already fetched; `ORDER BY` in the query
  is what sorts the whole result set.
- The graph view colours up to 8 labels; beyond that the rarest fold into a
  neutral "Other" bucket rather than reusing a hue.
