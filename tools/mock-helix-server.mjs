#!/usr/bin/env node
/**
 * A stand-in HelixDB instance for development and tests.
 *
 * It accepts the flat `POST /v1/query` traversal format used by the app (and
 * retains the older nested `/v2/query` format for compatibility) over a small
 * in-memory sample graph. It implements the subset of steps this app emits, so
 * the visualizer can be exercised without installing HelixDB; it is NOT a
 * HelixDB implementation and makes no claim to match its semantics beyond the
 * steps listed in `applyStep`.
 *
 *   node tools/mock-helix-server.mjs [--port 6969] [--seed 42]
 *
 * Anything the app sends that is not implemented comes back as HTTP 400 with a
 * message naming the step, which is exactly the feedback needed while working
 * on the compiler.
 */
import { createServer } from "node:http";

// ---------------------------------------------------------------------------
// Sample graph
// ---------------------------------------------------------------------------

const FIRST_NAMES = [
  "Alice", "Bob", "Carla", "Dmitri", "Elena", "Farid", "Grace", "Hana",
  "Ivan", "Jules", "Kira", "Liam", "Maya", "Noor", "Omar", "Priya",
  "Quinn", "Rosa", "Sven", "Tara", "Uma", "Viktor", "Wren", "Xiu",
  "Yara", "Zane",
];
const TOPICS = ["graphs", "vectors", "rust", "storage", "retrieval", "agents", "indexing"];
const CITIES = ["Berlin", "Lagos", "Osaka", "Lima", "Toronto", "Lisbon"];

/** Small deterministic PRNG so a given seed always builds the same graph. */
function makeRandom(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function buildGraph(seed) {
  const random = makeRandom(seed);
  const nodes = new Map();
  const edges = new Map();
  let nextId = 1n;

  const addNode = (label, properties) => {
    const id = nextId++;
    nodes.set(id, { id, label, properties });
    return id;
  };
  const addEdge = (label, from, to, properties = {}) => {
    const id = nextId++;
    edges.set(id, { id, label, from, to, properties });
    return id;
  };

  const pick = (list) => list[Math.floor(random() * list.length)];

  const users = FIRST_NAMES.map((name, index) =>
    addNode("User", {
      name,
      handle: `@${name.toLowerCase()}`,
      age: 21 + Math.floor(random() * 32),
      city: pick(CITIES),
      verified: random() > 0.7,
      joined: 2018 + Math.floor(random() * 8),
      // One deliberately huge id-like value, to prove i64 survives the round trip.
      externalId: index === 0 ? 9223372036854775807n : BigInt(100000 + index),
    }),
  );

  const topics = TOPICS.map((title) =>
    addNode("Topic", { title, followers: Math.floor(random() * 900) }),
  );

  const posts = [];
  for (let index = 0; index < 40; index++) {
    posts.push(
      addNode("Post", {
        title: `Notes on ${pick(TOPICS)} #${index + 1}`,
        wordCount: 120 + Math.floor(random() * 1800),
        score: Math.round(random() * 100) / 10,
      }),
    );
  }

  const orgs = ["Helix Labs", "Northwind", "Acme Data"].map((name) =>
    addNode("Org", { name, employees: 12 + Math.floor(random() * 400) }),
  );

  for (const user of users) {
    const followCount = 1 + Math.floor(random() * 5);
    for (let i = 0; i < followCount; i++) {
      const other = pick(users);
      if (other !== user) addEdge("Follows", user, other, { since: 2019 + Math.floor(random() * 7) });
    }
    // A self-follow exercises the loop-drawing path in the renderer.
    if (random() > 0.93) addEdge("Follows", user, user, { since: 2024 });
    addEdge("WorksAt", user, pick(orgs), { role: pick(["eng", "research", "ops"]) });
    for (let i = 0; i < 1 + Math.floor(random() * 3); i++) {
      addEdge("Likes", user, pick(topics), { weight: Math.round(random() * 100) / 100 });
    }
  }

  for (const post of posts) {
    addEdge("AuthoredBy", post, pick(users), {});
    addEdge("About", post, pick(topics), {});
  }

  return { nodes, edges };
}

// ---------------------------------------------------------------------------
// JSON with i64 fidelity
// ---------------------------------------------------------------------------

// Entity ids are i64, so the mock has to survive values JavaScript cannot hold
// in a `number`. Both directions go through a sentinel string, built from
// characters JSON.stringify never escapes so it survives serialization intact.
const BIG_PREFIX = "@@bigint:";
const BIG_SUFFIX = "@@";

/** Parses JSON keeping integer literals too large for a double as bigint. */
function parseJsonBig(text) {
  const marked = text.replace(
    /([:[,]\s*)(-?\d+)(?=\s*[,}\]])/g,
    (match, prefix, digits) =>
      Number.isSafeInteger(Number(digits))
        ? match
        : `${prefix}"${BIG_PREFIX}${digits}${BIG_SUFFIX}"`,
  );
  return JSON.parse(marked, (_key, value) =>
    typeof value === "string" && value.startsWith(BIG_PREFIX) && value.endsWith(BIG_SUFFIX)
      ? BigInt(value.slice(BIG_PREFIX.length, -BIG_SUFFIX.length))
      : value,
  );
}

/** Serializes bigint as a bare JSON number, the way HelixDB does. */
function stringifyBig(value) {
  const text = JSON.stringify(value, (_key, inner) =>
    typeof inner === "bigint" ? `${BIG_PREFIX}${inner}${BIG_SUFFIX}` : inner,
  );
  // Unquote the sentinels so each value lands on the wire as a JSON number.
  return text.replace(
    new RegExp(`"${BIG_PREFIX}(-?\\d+)${BIG_SUFFIX}"`, "g"),
    (_match, digits) => digits,
  );
}

// ---------------------------------------------------------------------------
// AST interpretation
// ---------------------------------------------------------------------------

class UnsupportedStep extends Error {}

/** One entity flowing through a traversal, tagged with what it is. */
const asNode = (node) => ({ kind: "node", entity: node });
const asEdge = (edge) => ({ kind: "edge", entity: edge });

function resolveField(item, source, graph) {
  const { kind, entity } = item;
  if (source === "$id") return entity.id;
  if (source === "$label") return entity.label;
  if (kind === "edge") {
    if (source === "$from.$id") return entity.from;
    if (source === "$to.$id") return entity.to;
    if (source === "$from.$label") return graph.nodes.get(entity.from)?.label ?? null;
    if (source === "$to.$label") return graph.nodes.get(entity.to)?.label ?? null;
  }
  const value = entity.properties[source];
  return value === undefined ? null : value;
}

function evaluateOperand(operand, item, graph) {
  if (operand === null || operand === undefined) return null;
  if ("property" in operand) return resolveField(item, operand.property, graph);
  if ("constant" in operand) return readConstant(operand.constant);
  throw new UnsupportedStep(`operand ${JSON.stringify(operand)}`);
}

function readConstant(constant) {
  if (constant === null) return null;
  if (typeof constant !== "object") return constant;
  for (const key of ["string", "bool", "f64", "f32"]) {
    if (key in constant) return constant[key];
  }
  if ("i64" in constant) {
    const value = constant.i64;
    return typeof value === "bigint" ? value : BigInt(value);
  }
  if ("array" in constant) return constant.array.map(readConstant);
  if ("null" in constant) return null;
  throw new UnsupportedStep(`constant ${JSON.stringify(constant)}`);
}

/** Numeric-aware comparison that treats i64 and f64 as the same number line. */
function compareValues(left, right) {
  if (left === null || right === null || left === undefined || right === undefined) return NaN;
  if (typeof left === "bigint" || typeof right === "bigint") {
    const a = typeof left === "bigint" ? left : BigInt(Math.trunc(Number(left)));
    const b = typeof right === "bigint" ? right : BigInt(Math.trunc(Number(right)));
    return a < b ? -1 : a > b ? 1 : 0;
  }
  if (typeof left === "number" && typeof right === "number") return left - right;
  if (typeof left === "boolean" || typeof right === "boolean") {
    return Number(left) - Number(right);
  }
  return String(left).localeCompare(String(right));
}

const valuesEqual = (a, b) => compareValues(a, b) === 0;

function evaluatePredicate(predicate, item, graph) {
  const [key] = Object.keys(predicate);
  const body = predicate[key];

  switch (key) {
    case "and":
      return body.predicates.every((part) => evaluatePredicate(part, item, graph));
    case "or":
      return body.predicates.some((part) => evaluatePredicate(part, item, graph));
    case "not":
      return !evaluatePredicate(body.predicate ?? body, item, graph);

    case "eq":
    case "neq":
    case "gt":
    case "gte":
    case "lt":
    case "lte": {
      const left = evaluateOperand(body.left, item, graph);
      const right = evaluateOperand(body.right, item, graph);
      if (key === "eq") return valuesEqual(left, right);
      if (key === "neq") return !valuesEqual(left, right);
      const order = compareValues(left, right);
      if (Number.isNaN(order)) return false;
      if (key === "gt") return order > 0;
      if (key === "gte") return order >= 0;
      if (key === "lt") return order < 0;
      return order <= 0;
    }

    case "between": {
      const value = evaluateOperand(body.value ?? { property: body.property }, item, graph);
      const low = evaluateOperand(body.min ?? body.low, item, graph);
      const high = evaluateOperand(body.max ?? body.high, item, graph);
      return compareValues(value, low) >= 0 && compareValues(value, high) <= 0;
    }

    case "is_in": {
      const value = evaluateOperand(body.value, item, graph);
      const values = evaluateOperand(body.values, item, graph);
      return Array.isArray(values) && values.some((candidate) => valuesEqual(value, candidate));
    }

    case "starts_with":
    case "ends_with":
    case "contains": {
      const value = evaluateOperand(body.value, item, graph);
      const needle = evaluateOperand(
        body.prefix ?? body.suffix ?? body.substring ?? body.value2,
        item,
        graph,
      );
      if (typeof value !== "string" || typeof needle !== "string") return false;
      if (key === "starts_with") return value.startsWith(needle);
      if (key === "ends_with") return value.endsWith(needle);
      return value.includes(needle);
    }

    case "is_null":
    case "is_not_null":
    case "has_key": {
      const name = typeof body === "string" ? body : body.property;
      const raw =
        name.startsWith("$") || item.entity.properties[name] !== undefined
          ? resolveField(item, name, graph)
          : undefined;
      const present = raw !== undefined && raw !== null;
      if (key === "is_null") return !present;
      return present;
    }

    default:
      throw new UnsupportedStep(`predicate "${key}"`);
  }
}

/** Runs a traversal AST, returning either a stream of items or a terminal value. */
function evaluate(node, graph) {
  const [step] = Object.keys(node);
  const body = node[step];

  // Source steps.
  if (step === "nodes" || step === "edges") {
    const table = step === "nodes" ? graph.nodes : graph.edges;
    const wrap = step === "nodes" ? asNode : asEdge;
    const reference = body.reference;
    if (reference === "all") return { stream: [...table.values()].map(wrap) };
    if (reference && "ids" in reference) {
      const ids = reference.ids.map((id) => (typeof id === "bigint" ? id : BigInt(id)));
      return { stream: ids.map((id) => table.get(id)).filter(Boolean).map(wrap) };
    }
    throw new UnsupportedStep(`${step} reference ${JSON.stringify(reference)}`);
  }
  if (step === "nodes_where" || step === "edges_where") {
    const table = step === "nodes_where" ? graph.nodes : graph.edges;
    const wrap = step === "nodes_where" ? asNode : asEdge;
    const stream = [...table.values()]
      .map(wrap)
      .filter((item) => evaluatePredicate(body.predicate, item, graph));
    return { stream };
  }

  const input = evaluate(body.input, graph);
  return applyStep(step, body, input, graph);
}

function applyStep(step, body, input, graph) {
  if (input.stream === undefined) {
    throw new UnsupportedStep(`step "${step}" applied to a terminal value`);
  }
  const stream = input.stream;

  switch (step) {
    case "where":
      return { stream: stream.filter((item) => evaluatePredicate(body.predicate, item, graph)) };

    case "dedup": {
      const seen = new Set();
      return {
        stream: stream.filter((item) => {
          const key = `${item.kind}:${item.entity.id}`;
          if (seen.has(key)) return false;
          seen.add(key);
          return true;
        }),
      };
    }

    case "limit":
      return { stream: stream.slice(0, Number(readBound(body.count))) };
    case "skip":
      return { stream: stream.slice(Number(readBound(body.count))) };

    case "order_by": {
      const direction = (body.order ?? "asc") === "desc" ? -1 : 1;
      const property = body.property;
      return {
        stream: [...stream].sort((a, b) => {
          const order = compareValues(
            resolveField(a, property, graph),
            resolveField(b, property, graph),
          );
          return direction * (Number.isNaN(order) ? 0 : order);
        }),
      };
    }

    case "out":
    case "in":
    case "both": {
      const out = [];
      for (const item of stream) {
        for (const edge of graph.edges.values()) {
          if (body.label && edge.label !== body.label) continue;
          if ((step === "out" || step === "both") && edge.from === item.entity.id) {
            const node = graph.nodes.get(edge.to);
            if (node) out.push(asNode(node));
          }
          if ((step === "in" || step === "both") && edge.to === item.entity.id) {
            const node = graph.nodes.get(edge.from);
            if (node) out.push(asNode(node));
          }
        }
      }
      return { stream: out };
    }

    case "out_e":
    case "in_e":
    case "both_e": {
      const out = [];
      for (const item of stream) {
        for (const edge of graph.edges.values()) {
          if (body.label && edge.label !== body.label) continue;
          const outgoing = edge.from === item.entity.id;
          const incoming = edge.to === item.entity.id;
          const matches =
            (step === "out_e" && outgoing) ||
            (step === "in_e" && incoming) ||
            (step === "both_e" && (outgoing || incoming));
          if (matches) out.push(asEdge(edge));
        }
      }
      return { stream: out };
    }

    case "out_n":
    case "in_n":
    case "other_n": {
      const out = [];
      for (const item of stream) {
        if (item.kind !== "edge") continue;
        const ids = step === "out_n" ? [item.entity.from] : step === "in_n" ? [item.entity.to] : [item.entity.from, item.entity.to];
        for (const id of ids) {
          const node = graph.nodes.get(id);
          if (node) out.push(asNode(node));
        }
      }
      return { stream: out };
    }

    case "count":
      return { value: stream.length };

    case "group_count": {
      const counts = {};
      for (const item of stream) {
        const key = String(resolveField(item, body.property, graph) ?? "");
        counts[key] = (counts[key] ?? 0) + 1;
      }
      return { value: counts };
    }

    case "value_map": {
      const properties = body.properties ?? null;
      return {
        value: stream.map((item) => {
          if (properties === null) return { ...item.entity.properties };
          const row = {};
          for (const name of properties) row[name] = resolveField(item, name, graph);
          return row;
        }),
      };
    }

    case "project": {
      return {
        value: stream.map((item) => {
          const row = {};
          for (const projection of body.projections) {
            const { source, alias } = projection.property;
            row[alias] = resolveField(item, source, graph);
          }
          return row;
        }),
      };
    }

    default:
      throw new UnsupportedStep(`step "${step}"`);
  }
}

function readBound(bound) {
  if (bound === null || bound === undefined) return 0;
  if (typeof bound === "number" || typeof bound === "bigint") return bound;
  if ("literal" in bound) return bound.literal;
  throw new UnsupportedStep(`bound ${JSON.stringify(bound)}`);
}

/** Runs one read batch and returns the map of returned variables. */
function runBatch(request, graph) {
  const batch = request.query?.read ?? request.query?.write;
  if (!batch) throw new UnsupportedStep("request without a read or write batch");
  if (!request.query.read) throw new UnsupportedStep("write batches (this mock is read-only)");

  const variables = {};
  for (const entry of batch.entries) {
    if (!entry.query) throw new UnsupportedStep("for_each entries");
    const outcome = evaluate(entry.query.root, graph);
    variables[entry.query.name] = "value" in outcome ? outcome.value : outcome.stream.map((i) => i.entity);
  }

  const response = {};
  for (const name of batch.returns) response[name] = variables[name];
  return response;
}

function readFlatValue(value) {
  if (value === "Null" || value === null) return null;
  if (typeof value !== "object") return value;
  for (const key of ["String", "Bool", "F64", "F32", "I64"]) {
    if (key in value) return value[key];
  }
  for (const key of ["StringArray", "I64Array", "Array"]) {
    if (key in value) return value[key].map(readFlatValue);
  }
  throw new UnsupportedStep(`flat value ${JSON.stringify(value)}`);
}

function evaluateFlatPredicate(predicate, item, graph) {
  const [kind] = Object.keys(predicate);
  const body = predicate[kind];
  if (kind === "And") return body.every((part) => evaluateFlatPredicate(part, item, graph));
  if (kind === "Or") return body.some((part) => evaluateFlatPredicate(part, item, graph));
  if (kind === "Not") return !evaluateFlatPredicate(body, item, graph);
  if (["Eq", "Neq", "Gt", "Gte", "Lt", "Lte"].includes(kind)) {
    const [property, operand] = body;
    const left = resolveField(item, property, graph);
    const right = readFlatValue(operand);
    if (kind === "Eq") return valuesEqual(left, right);
    if (kind === "Neq") return !valuesEqual(left, right);
    const order = compareValues(left, right);
    if (Number.isNaN(order)) return false;
    if (kind === "Gt") return order > 0;
    if (kind === "Gte") return order >= 0;
    if (kind === "Lt") return order < 0;
    return order <= 0;
  }
  if (kind === "Between") {
    const [property, low, high] = body;
    const value = resolveField(item, property, graph);
    return compareValues(value, readFlatValue(low)) >= 0 && compareValues(value, readFlatValue(high)) <= 0;
  }
  if (kind === "IsIn") {
    const [property, values] = body;
    return readFlatValue(values).some((candidate) => valuesEqual(resolveField(item, property, graph), candidate));
  }
  if (["StartsWith", "EndsWith", "Contains"].includes(kind)) {
    const [property, operand] = body;
    const value = resolveField(item, property, graph);
    const needle = readFlatValue(operand);
    if (typeof value !== "string" || typeof needle !== "string") return false;
    if (kind === "StartsWith") return value.startsWith(needle);
    if (kind === "EndsWith") return value.endsWith(needle);
    return value.includes(needle);
  }
  if (["IsNull", "IsNotNull", "HasKey"].includes(kind)) {
    const value = resolveField(item, body, graph);
    const present = value !== undefined && value !== null;
    return kind === "IsNull" ? !present : present;
  }
  throw new UnsupportedStep(`flat predicate "${kind}"`);
}

function flatEntityRow(item) {
  if (item.kind === "edge") {
    return {
      ...item.entity.properties,
      $id: item.entity.id,
      $label: item.entity.label,
      $from: item.entity.from,
      $to: item.entity.to,
    };
  }
  return { ...item.entity.properties, $id: item.entity.id, $label: item.entity.label };
}

function flatSource(kind, body, graph) {
  const nodes = kind.startsWith("N");
  const table = nodes ? graph.nodes : graph.edges;
  const wrap = nodes ? asNode : asEdge;
  let stream;
  if (body === "All") stream = [...table.values()].map(wrap);
  else if (body && body.Ids) stream = body.Ids.map((id) => table.get(BigInt(id))).filter(Boolean).map(wrap);
  else stream = [...table.values()].map(wrap);
  if (kind.endsWith("Where")) stream = stream.filter((item) => evaluateFlatPredicate(body, item, graph));
  return { stream };
}

function applyFlatStep(step, input, graph) {
  if (step === "Dedup") return applyStep("dedup", {}, input, graph);
  if (step === "Count") return applyStep("count", {}, input, graph);
  if (step === "EdgeProperties") return { value: input.stream.map(flatEntityRow) };

  const [kind] = Object.keys(step);
  const body = step[kind];
  if (["N", "E", "NWhere", "EWhere"].includes(kind)) return flatSource(kind, body, graph);
  if (kind === "Where") return { stream: input.stream.filter((item) => evaluateFlatPredicate(body, item, graph)) };
  if (kind === "ValueMap") return { value: input.stream.map(flatEntityRow) };
  if (kind === "Limit") return applyStep("limit", { count: body }, input, graph);
  if (kind === "Skip") return applyStep("skip", { count: body }, input, graph);
  if (kind === "OrderBy") return applyStep("order_by", { property: body[0], order: body[1].toLowerCase() }, input, graph);
  if (kind === "GroupCount") return applyStep("group_count", { property: body }, input, graph);

  const traversal = {
    Out: "out", In: "in", Both: "both", OutE: "out_e", InE: "in_e", BothE: "both_e",
    OutN: "out_n", InN: "in_n", OtherN: "other_n",
  }[kind];
  if (traversal) return applyStep(traversal, { label: body }, input, graph);
  throw new UnsupportedStep(`flat step "${kind}"`);
}

function runFlatBatch(request, graph) {
  if (request.request_type !== "read") throw new UnsupportedStep("write requests (this mock is read-only)");
  const variables = {};
  for (const entry of request.query?.queries ?? []) {
    const query = entry.Query;
    if (!query) throw new UnsupportedStep("non-query flat entries");
    let outcome = { stream: [] };
    for (const step of query.steps) outcome = applyFlatStep(step, outcome, graph);
    variables[query.name] = "value" in outcome ? outcome.value : outcome.stream.map(flatEntityRow);
  }
  const response = {};
  for (const name of request.query?.returns ?? []) response[name] = variables[name];
  return response;
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

function parseArgs(argv) {
  const args = { port: 6969, seed: 42 };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--port") args.port = Number(argv[++i]);
    else if (argv[i] === "--seed") args.seed = Number(argv[++i]);
  }
  return args;
}

export function createMockServer({ seed = 42 } = {}) {
  const graph = buildGraph(seed);

  const server = createServer((request, response) => {
    if (request.method !== "POST" || (!request.url.endsWith("/v1/query") && !request.url.endsWith("/v2/query"))) {
      response.writeHead(404, { "content-type": "text/plain" });
      response.end("not found — this mock serves POST /v1/query and /v2/query");
      return;
    }

    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      try {
        const parsed = parseJsonBig(body);
        const result = parsed.request_type ? runFlatBatch(parsed, graph) : runBatch(parsed, graph);
        response.writeHead(200, { "content-type": "application/json" });
        response.end(stringifyBig(result));
      } catch (error) {
        const unsupported = error instanceof UnsupportedStep;
        response.writeHead(unsupported ? 400 : 500, { "content-type": "text/plain" });
        response.end(
          unsupported
            ? `mock server does not implement ${error.message}`
            : `mock server error: ${error.stack ?? error}`,
        );
      }
    });
  });

  return { server, graph };
}

// Only listen when run directly, so tests can import and drive the server.
if (process.argv[1] && import.meta.url.endsWith(process.argv[1].split("/").pop())) {
  const { port, seed } = parseArgs(process.argv.slice(2));
  const { server, graph } = createMockServer({ seed });
  server.listen(port, () => {
    console.log(
      `mock HelixDB listening on http://localhost:${port} ` +
        `(${graph.nodes.size} nodes, ${graph.edges.size} edges, seed ${seed})`,
    );
    console.log("this is a stand-in for development — not a HelixDB implementation");
  });
}
