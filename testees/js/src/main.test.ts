import { spawn } from "node:child_process";
import { once } from "node:events";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import assert from "node:assert/strict";

// runs the compiled program as the conformance runner will, over real pipes
const program = fileURLToPath(new URL("./main.js", import.meta.url));

async function exchange(input: string): Promise<unknown[]> {
  const child = spawn(process.execPath, [program], { stdio: ["pipe", "pipe", "inherit"] });
  let out = "";
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk: string) => {
    out += chunk;
  });
  child.stdin.end(input);
  const [code] = await once(child, "close");
  assert.equal(code, 0);
  return out
    .split("\n")
    .filter((line) => line !== "")
    .map((line) => JSON.parse(line));
}

test("hello reports the sdk and supported ops", async () => {
  const replies = await exchange('{"op":"hello"}\n');
  assert.deepEqual(replies, [{ sdk: "@stellar/stellar-sdk", version: null, ops: ["hello"] }]);
});

test("unknown ops are rejected", async () => {
  const replies = await exchange('{"op":"decode"}\n');
  assert.deepEqual(replies, [{ error: "unsupported op" }]);
});

test("one reply per line, in order", async () => {
  const replies = await exchange('{"op":"hello"}\n{"op":"encode"}\nnot json\n{"op":"hello"}\n');
  assert.deepEqual(replies, [
    { sdk: "@stellar/stellar-sdk", version: null, ops: ["hello"] },
    { error: "unsupported op" },
    { error: "invalid json" },
    { sdk: "@stellar/stellar-sdk", version: null, ops: ["hello"] },
  ]);
});
