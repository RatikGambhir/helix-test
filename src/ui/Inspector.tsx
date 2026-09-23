import { ArrowDownLeft, ArrowUpRight, MousePointerClick, Network } from "lucide-react";

import { Button } from "@/components/ui/button";
import { stringify, type QueryResult } from "../results";
import { EmptyState, ErrorNotice } from "./feedback";

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
  if (loading) {
    return <EmptyState className="is-compact" loading title="Loading entity" />;
  }
  if (error) return <div className="inspector"><ErrorNotice message={error} /></div>;
  if (!detail) {
    return (
      <EmptyState className="is-compact" icon={MousePointerClick} title="Nothing selected">
        Select a node or edge in the graph, click an id in the results, or run{" "}
        <code>DESCRIBE NODE &lt;id&gt;</code>.
      </EmptyState>
    );
  }

  const properties = Object.entries(detail.properties);

  return (
    <div className="inspector">
      <header className="inspector-head">
        <span className="eyebrow">{detail.entity === "nodes" ? "Node" : "Edge"}</span>
        <h3>{detail.label ?? "(no label)"}</h3>
        {detail.id ? <code className="inspector-id">id {detail.id}</code> : null}
        {detail.entity === "nodes" && detail.id ? (
          <Button variant="outline" size="sm" onClick={() => onFocusInGraph(detail.id!)}>
            <Network />
            Show neighbourhood
          </Button>
        ) : null}
      </header>

      <section className="inspector-section">
        <h4 className="eyebrow">
          Properties <span className="eyebrow-count">{properties.length}</span>
        </h4>
        {properties.length === 0 ? (
          <p className="inspector-note">No stored properties.</p>
        ) : (
          <dl className="property-list">
            {properties.map(([key, value]) => (
              <div key={key}>
                <dt title={key}>{key}</dt>
                <dd>{stringify(value)}</dd>
              </div>
            ))}
          </dl>
        )}
      </section>

      {detail.entity === "edges" && detail.endpoints ? (
        <section className="inspector-section">
          <h4 className="eyebrow">Endpoints</h4>
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
                  <Button variant="link" onClick={() => onInspect("node", node.id)}>
                    {node.label ?? "(no label)"} · {node.id}
                  </Button>
                ) : (
                  <span className="inspector-note">unavailable</span>
                )}
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {detail.entity === "nodes" ? (
        <>
          <section className="inspector-section">
            <h4 className="eyebrow">
              Relationships
              {detail.degree !== null ? <span className="eyebrow-count">{detail.degree}</span> : null}
            </h4>
            {detail.edges.length === 0 ? (
              <p className="inspector-note">Nothing connected.</p>
            ) : (
              <ul className="relation-list">
                {detail.edges.map((edge) => {
                  const outgoing = edge.source === detail.id;
                  const other = outgoing ? edge.target : edge.source;
                  const Direction = outgoing ? ArrowUpRight : ArrowDownLeft;
                  return (
                    <li key={edge.id}>
                      <Direction
                        className="relation-direction"
                        data-direction={outgoing ? "out" : "in"}
                        aria-label={outgoing ? "outgoing" : "incoming"}
                        role="img"
                      />
                      <Button variant="link" className="relation-edge" onClick={() => onInspect("edge", edge.id)}>
                        {edge.label ?? "(no label)"}
                      </Button>
                      <Button variant="link" className="relation-node" onClick={() => onInspect("node", other)}>
                        {other}
                      </Button>
                    </li>
                  );
                })}
              </ul>
            )}
            {detail.degree !== null && detail.edges.length < detail.degree ? (
              <p className="inspector-note">
                Showing {detail.edges.length} of {detail.degree}. Raise the cap with{" "}
                <code>DESCRIBE NODE {detail.id} LIMIT {detail.degree}</code>.
              </p>
            ) : null}
          </section>

          <section className="inspector-section">
            <h4 className="eyebrow">
              Neighbours <span className="eyebrow-count">{detail.neighbours.length}</span>
            </h4>
            {detail.neighbours.length === 0 ? (
              <p className="inspector-note">No neighbours.</p>
            ) : (
              <ul className="relation-list">
                {detail.neighbours.map((node) => (
                  <li key={node.id}>
                    <Button variant="link" onClick={() => onInspect("node", node.id)}>
                      {node.label ?? "(no label)"} · {node.id}
                    </Button>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      ) : null}
    </div>
  );
}
