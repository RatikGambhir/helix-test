import { stringify, type QueryResult } from "../results";

interface Props {
  /** The most recent DESCRIBE result, if one has been run. */
  detail: Extract<QueryResult, { kind: "describe" }> | null;
  loading: boolean;
  error: string | null;
  onInspect: (kind: "node" | "edge", id: string) => void;
  onFocusInGraph: (id: string) => void;
}

/**
 * Shows one entity and everything attached to it — the "relationship info"
 * side of the app, as opposed to the whole-graph picture.
 */
export function Inspector({ detail, loading, error, onInspect, onFocusInGraph }: Props) {
  if (loading) return <p className="hint-text">Loading…</p>;
  if (error) return <p className="panel-error">{error}</p>;
  if (!detail) {
    return (
      <p className="hint-text">
        Select a node or edge in the graph, click an id in the results, or run{" "}
        <code>DESCRIBE NODE &lt;id&gt;</code>.
      </p>
    );
  }

  const properties = Object.entries(detail.properties);

  return (
    <div className="inspector">
      <header>
        <span className="entity-kind">{detail.entity === "nodes" ? "Node" : "Edge"}</span>
        <h3>{detail.label ?? "(no label)"}</h3>
        {detail.id && <code className="entity-id">{detail.id}</code>}
        {detail.entity === "nodes" && detail.id && (
          <button type="button" onClick={() => onFocusInGraph(detail.id!)}>
            Show neighbourhood
          </button>
        )}
      </header>

      <section>
        <h4>Properties</h4>
        {properties.length === 0 ? (
          <p className="hint-text">No stored properties.</p>
        ) : (
          <dl className="property-list">
            {properties.map(([key, value]) => (
              <div key={key}>
                <dt>{key}</dt>
                <dd>{stringify(value)}</dd>
              </div>
            ))}
          </dl>
        )}
      </section>

      {detail.entity === "edges" && detail.endpoints && (
        <section>
          <h4>Endpoints</h4>
          <ul className="relation-list">
            {(
              [
                ["source", detail.endpoints.source],
                ["target", detail.endpoints.target],
              ] as const
            ).map(([role, node]) => (
              <li key={role}>
                <span className="relation-role">{role}</span>
                {node ? (
                  <button type="button" className="cell-link" onClick={() => onInspect("node", node.id)}>
                    {node.label ?? "(no label)"} · {node.id}
                  </button>
                ) : (
                  <span className="hint-text">unavailable</span>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      {detail.entity === "nodes" && (
        <>
          <section>
            <h4>
              Relationships
              {detail.degree !== null && <span className="count-badge">{detail.degree}</span>}
            </h4>
            {detail.edges.length === 0 ? (
              <p className="hint-text">Nothing connected.</p>
            ) : (
              <ul className="relation-list">
                {detail.edges.map((edge) => {
                  const outgoing = edge.source === detail.id;
                  const other = outgoing ? edge.target : edge.source;
                  return (
                    <li key={edge.id}>
                      <span className="relation-direction" aria-label={outgoing ? "outgoing" : "incoming"}>
                        {outgoing ? "→" : "←"}
                      </span>
                      <button type="button" className="cell-link" onClick={() => onInspect("edge", edge.id)}>
                        {edge.label ?? "(no label)"}
                      </button>
                      <button type="button" className="cell-link muted" onClick={() => onInspect("node", other)}>
                        {other}
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
            {detail.degree !== null && detail.edges.length < detail.degree && (
              <p className="hint-text">
                Showing {detail.edges.length} of {detail.degree}. Raise the cap with{" "}
                <code>DESCRIBE NODE {detail.id} LIMIT {detail.degree}</code>.
              </p>
            )}
          </section>

          <section>
            <h4>
              Neighbours
              <span className="count-badge">{detail.neighbours.length}</span>
            </h4>
            {detail.neighbours.length === 0 ? (
              <p className="hint-text">No neighbours.</p>
            ) : (
              <ul className="relation-list">
                {detail.neighbours.map((node) => (
                  <li key={node.id}>
                    <button type="button" className="cell-link" onClick={() => onInspect("node", node.id)}>
                      {node.label ?? "(no label)"} · {node.id}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      )}
    </div>
  );
}
