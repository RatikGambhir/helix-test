import { describe, expect, it } from "vitest";

import { parse } from "../src/hql/parser";
import { SPLASH_QUERY } from "../src/ui/SplashScreen";

describe("splash HelixSQL preview", () => {
  it("is a valid graph query in the app's language", () => {
    expect(parse(SPLASH_QUERY)).toMatchObject({
      kind: "graph",
      selection: {
        source: { entity: "nodes", label: "User" },
        limit: 150,
        hops: [{ direction: "out", label: "Follows" }],
      },
      withProperties: ["name", "email"],
      maxEdges: 600,
    });
  });
});
