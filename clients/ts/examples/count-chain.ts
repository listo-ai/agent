/**
 * count-chain.ts — example: seed a count node, write an input, watch events.
 *
 * Usage:
 *   pnpm tsx examples/count-chain.ts
 *
 * Requires a running agent at http://localhost:4000.
 */
import { AgentClient } from "../src/index.js";

const BASE_URL = process.env["AGENT_URL"] ?? "http://localhost:4000";

async function main(): Promise<void> {
  const client = await AgentClient.connect({
    baseUrl: BASE_URL,
    // Skip capability check until the agent exposes /capabilities.
    skipCapabilityCheck: true,
  });

  console.log("Connected to agent at", BASE_URL);

  // 1. List top-level nodes.
  const nodes = await client.nodes.getNodes();
  console.log("Nodes:", nodes.map((n) => n.path));

  // 2. Subscribe to graph events and print the first 5.
  console.log("Subscribing to events (waiting for 5)…");
  const stream = client.events.subscribe({
    onOpen: () => console.log("SSE stream open"),
  });

  let count = 0;
  for await (const event of stream) {
    console.log("Event:", JSON.stringify(event));
    count += 1;
    if (count >= 5) {
      stream.close();
      break;
    }
  }

  console.log("Done.");
}

main().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
