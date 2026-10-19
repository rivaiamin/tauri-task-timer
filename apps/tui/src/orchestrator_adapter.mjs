import { readFileSync } from "node:fs";
import { createPaseoClient } from "@getpaseo/client";

function fail(message) {
  process.stderr.write(`${JSON.stringify({ error: message })}\n`);
  process.exitCode = 1;
}

function validCursor(cursor) {
  return cursor === null || (
    cursor &&
    typeof cursor.epoch === "string" &&
    Number.isSafeInteger(cursor.seq) &&
    cursor.seq >= 0
  );
}

async function main() {
  const nodeMajor = Number(process.versions.node.split(".")[0]);
  if (!Number.isSafeInteger(nodeMajor) || nodeMajor < 22) {
    throw new Error("Paseo history requires Node.js 22 or newer");
  }
  let input;
  try {
    input = JSON.parse(readFileSync(0, "utf8"));
  } catch {
    throw new Error("invalid adapter request JSON");
  }
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    throw new Error("adapter request must be an object");
  }
  if (input.action !== "history") {
    throw new Error("unsupported adapter action");
  }
  if (typeof input.agentId !== "string" || !input.agentId.trim()) {
    throw new Error("agentId is required");
  }
  let daemonUrl;
  try {
    daemonUrl = new URL(input.url);
  } catch {
    throw new Error("invalid local Paseo WebSocket URL");
  }
  if (
    daemonUrl.protocol !== "ws:" ||
    !["127.0.0.1", "::1", "[::1]", "localhost"].includes(daemonUrl.hostname) ||
    daemonUrl.pathname !== "/ws"
  ) {
    throw new Error("Paseo URL must be a loopback /ws endpoint");
  }
  const direction = input.direction ?? "tail";
  if (!["tail", "before", "after"].includes(direction)) {
    throw new Error("invalid history direction");
  }
  if (direction !== "tail" && !validCursor(input.cursor)) {
    throw new Error("valid history cursor required");
  }
  if (direction !== "tail" && input.cursor == null) {
    throw new Error("history cursor required");
  }
  const limit = input.limit ?? 50;
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 100) {
    throw new Error("history page size must be between 1 and 100");
  }

  const client = createPaseoClient({
    url: input.url,
    password: process.env.PASEO_PASSWORD || undefined,
    connectTimeoutMs: 5000,
    reconnect: { enabled: false },
  });
  try {
    await client.connect();
    const page = await client.agents.ref(input.agentId).timeline.refetch({
      direction,
      cursor: input.cursor ?? undefined,
      limit,
      projection: "projected",
    });
    if (
      page.agentId !== input.agentId ||
      page.direction !== direction ||
      typeof page.epoch !== "string" ||
      !Array.isArray(page.entries) ||
      typeof page.hasOlder !== "boolean" ||
      typeof page.hasNewer !== "boolean" ||
      typeof page.reset !== "boolean" ||
      typeof page.staleCursor !== "boolean" ||
      typeof page.gap !== "boolean" ||
      !page.window ||
      !Number.isSafeInteger(page.window.minSeq) ||
      !Number.isSafeInteger(page.window.maxSeq) ||
      !Number.isSafeInteger(page.window.nextSeq)
    ) {
      throw new Error("Paseo returned an invalid history page");
    }
    const entries = page.entries.map((entry) => {
      if (
        !entry ||
        !entry.item ||
        typeof entry.item.type !== "string" ||
        typeof entry.timestamp !== "string" ||
        !Number.isSafeInteger(entry.seqStart) ||
        !Number.isSafeInteger(entry.seqEnd) ||
        !Array.isArray(entry.sourceSeqRanges)
      ) {
        throw new Error("Paseo returned an invalid history entry");
      }
      return {
        item: entry.item,
        timestamp: entry.timestamp,
        seqStart: entry.seqStart,
        seqEnd: entry.seqEnd,
        sourceSeqRanges: entry.sourceSeqRanges,
      };
    });
    process.stdout.write(JSON.stringify({
      agentId: page.agentId,
      epoch: page.epoch,
      direction: page.direction,
      startCursor: page.startCursor,
      endCursor: page.endCursor,
      hasOlder: page.hasOlder,
      hasNewer: page.hasNewer,
      reset: page.reset,
      staleCursor: page.staleCursor,
      gap: page.gap,
      window: page.window,
      entries,
    }));
  } finally {
    await client.close();
  }
}

main().catch((error) => fail(error instanceof Error ? error.message : String(error)));
