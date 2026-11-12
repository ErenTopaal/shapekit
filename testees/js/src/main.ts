// shapekit javascript testee. speaks newline-delimited json on stdin and stdout,
// one reply per request line. only `hello` exists until the full testee protocol lands.

import { createInterface } from "node:readline";

type Reply = Record<string, unknown>;

// version stays null until @stellar/stellar-sdk is an actual dependency,
// so the runner never sees a version this testee does not load
const hello: Reply = {
  sdk: "@stellar/stellar-sdk",
  version: null,
  ops: ["hello"],
};

function handle(line: string): Reply {
  let request: unknown;
  try {
    request = JSON.parse(line);
  } catch {
    return { error: "invalid json" };
  }
  if (typeof request === "object" && request !== null && "op" in request && request.op === "hello") {
    return hello;
  }
  return { error: "unsupported op" };
}

async function main(): Promise<void> {
  const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of lines) {
    if (line.trim() === "") continue;
    process.stdout.write(`${JSON.stringify(handle(line))}\n`);
  }
}

await main();
