// shapekit javascript testee: answers testee protocol requests (docs/testee-protocol.md) with
// @stellar/stellar-sdk, one json object per line on stdin and stdout.

import { createInterface } from "node:readline";

import { Coverage } from "./coverage.js";
import { Ops } from "./ops.js";
import { handle } from "./request.js";
import { loadSdk } from "./sdk.js";

async function main(): Promise<void> {
  const coverage = Coverage.start();
  const ops = new Ops(await loadSdk(), coverage);
  const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
  for await (const line of lines) {
    if (line === "") continue;
    process.stdout.write(`${JSON.stringify(handle(ops, line))}\n`);
  }
}

await main();
