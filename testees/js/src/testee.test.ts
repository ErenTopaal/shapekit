// runs the compiled testee as the conformance runner does, over pipes, against two pinned sdk
// releases: 17.4.0 (generated XdrValue classes with their own xdr-json) and 16.4.1 (js-xdr 4
// classes, rendered by the testee's sep-51 projection).

import { spawn } from "node:child_process";
import { once } from "node:events";
import { cpSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { after, describe, test } from "node:test";
import assert from "node:assert/strict";

const dist = dirname(fileURLToPath(import.meta.url));
const testeeRoot = dirname(dist);

type Reply = { id: string; status: string; result?: any; error?: { class: string; message: string }; reason?: string };

// a directory laid out like target/testees/js/<version>: the compiled testee next to a
// node_modules holding one sdk release
function layout(sdkDir: string): string {
  const dir = mkdtempSync(join(tmpdir(), "shapekit-testee-js-"));
  writeFileSync(join(dir, "package.json"), JSON.stringify({ private: true, type: "module" }));
  mkdirSync(join(dir, "node_modules", "@stellar"), { recursive: true });
  symlinkSync(realpathSync(sdkDir), join(dir, "node_modules", "@stellar", "stellar-sdk"), "dir");
  cpSync(dist, join(dir, "dist"), { recursive: true });
  return join(dir, "dist", "main.js");
}

async function exchange(program: string, lines: string[]): Promise<Reply[]> {
  const child = spawn(process.execPath, [program], { stdio: ["pipe", "pipe", "inherit"] });
  let out = "";
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk: string) => {
    out += chunk;
  });
  child.stdin.end(lines.map((l) => `${l}\n`).join(""));
  const [code] = await once(child, "close");
  assert.equal(code, 0);
  return out
    .split("\n")
    .filter((line) => line !== "")
    .map((line) => JSON.parse(line) as Reply);
}

async function ask(program: string, request: object): Promise<Reply> {
  const [reply] = await exchange(program, [JSON.stringify({ id: "t", ...request })]);
  assert.ok(reply !== undefined);
  return reply;
}

function ok(reply: Reply): any {
  assert.equal(reply.status, "ok", JSON.stringify(reply));
  return reply.result;
}

function errorClass(reply: Reply): string | undefined {
  assert.equal(reply.status, "error", JSON.stringify(reply));
  return reply.error?.class;
}

const ZERO_G = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const ASSET_ABCD = "AAAAAUFCQ0QAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

// the TransactionEnvelope example of sep-51 2.0.1, bytes and json as the sep gives them
const SEP51_ENVELOPE =
  "AAAAAgAAAADmmSZkwY3163TMouB2TY8MljqXw2IxVYTGyvDrR6YtAAAqmmQAABpuAAAAAQAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAQAAAAEAAAAAAAAAAQAAAAAAAAABAAAAAAAAAAAAAAABAAAABgAAAAHXkotywnA8z+r365/0701QSlWouXn8m0UOoshCtNHOYQAAABQAAAABAAI9fQAAAAAAAAD4AAAAAAAqmgAAAAABR6YtAAAAAEArDtxbqUI+CsdkRmV0lFhVt0wyB7fyrmmkM6Fr35wpPcK8WKcXeKTl4BQ+akE14MZtpaea9LMdhXopaW3pJA0E";
const SEP51_ENVELOPE_JSON = {
  tx: {
    tx: {
      source_account: "GDTJSJTEYGG7L23UZSROA5SNR4GJMOUXYNRDCVMEY3FPB22HUYWQBZIA",
      fee: 2792036,
      seq_num: "29059748724737",
      cond: "none",
      memo: "none",
      operations: [
        {
          source_account: null,
          body: {
            invoke_host_function: {
              host_function: {
                create_contract: { contract_id_preimage: { asset: "native" }, executable: "stellar_asset" },
              },
              auth: [],
            },
          },
        },
      ],
      ext: {
        v1: {
          ext: "v0",
          resources: {
            footprint: {
              read_only: [],
              read_write: [
                {
                  contract_data: {
                    contract: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                    key: "ledger_key_contract_instance",
                    durability: "persistent",
                  },
                },
              ],
            },
            instructions: 146813,
            disk_read_bytes: 0,
            write_bytes: 248,
          },
          resource_fee: "2791936",
        },
      },
    },
    signatures: [
      {
        hint: "47a62d00",
        signature:
          "2b0edc5ba9423e0ac764466574945855b74c3207b7f2ae69a433a16bdf9c293dc2bc58a71778a4e5e0143e6a4135e0c66da5a79af4b31d857a29696de9240d04",
      },
    ],
  },
};

// the testnet pool of py-stellar-base#1246, assets in core's order
const POOL_A = "AAAAAVVTRAAAAAAA4wX4hf/0IDWYv3p7spz23LBfnsxE8VpcVVfvZlwOAfc=";
const POOL_B = "AAAAAVVTRAAAAAAA96TA5bLRFwmw77hliDpn58MyAH9oo7Kz9x0ESsvSz4U=";
const POOL_ID = "c2a730127d006897b021d6fcb279da68b4f65f9b6a408c74eb2cdd85ea862d5e";

// js-stellar-sdk#1766 issuers: 32 bytes of 0x3a sort after 32 zero bytes
const ISSUER_3A = "AAAAAVVTRAAAAAAAOjo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo6Ojo=";
const ISSUER_00 = "AAAAAVVTRAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

// js-stellar-sdk#1789: {u32 1: u32 10, symbol "1": u32 20}
const COLLIDING_MAP = "AAAAEQAAAAEAAAACAAAAAwAAAAEAAAADAAAACgAAAA8AAAABMQAAAAAAAAMAAAAU";

interface Release {
  version: string;
  program: string;
  xdrJson: boolean;
  muxedContract: boolean;
}

const releases: Release[] = [
  { version: "17.4.0", program: join(dist, "main.js"), xdrJson: true, muxedContract: true },
  {
    version: "16.4.1",
    program: layout(join(testeeRoot, "node_modules", "stellar-sdk-16")),
    xdrJson: false,
    muxedContract: false,
  },
];
after(() => {
  for (const r of releases) {
    if (r.program !== join(dist, "main.js")) rmSync(dirname(dirname(r.program)), { recursive: true, force: true });
  }
});

for (const r of releases) {
  describe(`@stellar/stellar-sdk ${r.version}`, () => {
    test("hello reads the installed versions", async () => {
      const hello = ok(await ask(r.program, { op: "hello" }));
      assert.equal(hello.protocol, 2);
      assert.equal(hello.sdk, "@stellar/stellar-sdk");
      assert.equal(hello.sdk_version, r.version);
      assert.equal(hello.components["@stellar/js-xdr"], r.xdrJson ? "5.0.0" : "4.0.0");
      assert.equal(hello.decode_limits, "ignored");
      assert.equal(hello.ops.includes("encode_json"), r.xdrJson);
      assert.equal(hello.ops.includes("map_validate"), false);
    });

    test("decode_encode round-trips and refuses trailing bytes", async () => {
      const req = { op: "decode_encode", type: "Asset", depth_limit: 1000, len_limit: 44 };
      assert.deepEqual(ok(await ask(r.program, { ...req, xdr: ASSET_ABCD })), { xdr: ASSET_ABCD });
      const trailing = Buffer.concat([Buffer.from(ASSET_ABCD, "base64"), Buffer.alloc(4)]).toString("base64");
      assert.equal(errorClass(await ask(r.program, { ...req, xdr: trailing })), "rejected");
      assert.equal(errorClass(await ask(r.program, { ...req, xdr: "not base64!" })), "bad-request");
      assert.equal(errorClass(await ask(r.program, { ...req, xdr: "QUJD=Q==" })), "bad-request");
      assert.equal(errorClass(await ask(r.program, { ...req, xdr: "QUJDRA" })), "bad-request");
      // a ledger's worth of base64 (pubnet ledger 64783972 is 3.7 MB) is read, not refused
      const large = Buffer.concat([Buffer.from(ASSET_ABCD, "base64"), Buffer.alloc(4 << 20)]).toString("base64");
      assert.equal(errorClass(await ask(r.program, { ...req, xdr: large })), "rejected");
      const unknown = await ask(r.program, { ...req, type: "NoSuchType", xdr: ASSET_ABCD });
      assert.equal(unknown.status, "unsupported");
    });

    test("decode_json renders sep-51", async () => {
      const req = { op: "decode_json", depth_limit: 1000, len_limit: 1000 };
      assert.deepEqual(ok(await ask(r.program, { ...req, type: "Asset", xdr: ASSET_ABCD })), {
        json: { credit_alphanum4: { asset_code: "ABCD", issuer: ZERO_G } },
      });
      assert.deepEqual(ok(await ask(r.program, { ...req, type: "TransactionEnvelope", xdr: SEP51_ENVELOPE })), {
        json: SEP51_ENVELOPE_JSON,
      });
      assert.deepEqual(ok(await ask(r.program, { ...req, type: "ScVal", xdr: COLLIDING_MAP })), {
        json: { map: [{ key: { u32: 1 }, val: { u32: 10 } }, { key: { symbol: "1" }, val: { u32: 20 } }] },
      });
    });

    test("encode_json reads sep-51 where the sdk can", async () => {
      const reply = await ask(r.program, {
        op: "encode_json",
        type: "Asset",
        json: { credit_alphanum4: { asset_code: "ABCD", issuer: ZERO_G } },
      });
      if (r.xdrJson) assert.deepEqual(ok(reply), { xdr: ASSET_ABCD });
      else assert.equal(reply.status, "unsupported");
    });

    test("strkey codecs", async () => {
      assert.deepEqual(ok(await ask(r.program, { op: "strkey_decode", text: ZERO_G })), {
        kind: "account",
        payload: Buffer.alloc(32).toString("base64"),
      });
      assert.deepEqual(
        ok(await ask(r.program, { op: "strkey_encode", kind: "account", payload: Buffer.alloc(32).toString("base64") })),
        { text: ZERO_G },
      );
      assert.equal(errorClass(await ask(r.program, { op: "strkey_decode", text: `${ZERO_G}A` })), "rejected");
      const w = "WA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJUAAAAAAAAAAAAAWWC";
      const decoded = await ask(r.program, { op: "strkey_decode", text: w });
      if (r.muxedContract) assert.equal(ok(decoded).kind, "muxed_contract");
      else assert.equal(errorClass(decoded), "rejected");
    });

    test("strkey_decode text_base64: utf-8 decodes, other bytes are unsupported", async () => {
      const asBytes = Buffer.from(ZERO_G).toString("base64");
      assert.equal(ok(await ask(r.program, { op: "strkey_decode", text_base64: asBytes })).kind, "account");
      const stray = Buffer.concat([Buffer.from(ZERO_G), Buffer.from([0xff, 0xff])]).toString("base64");
      assert.equal((await ask(r.program, { op: "strkey_decode", text_base64: stray })).status, "unsupported");
      assert.equal(
        errorClass(await ask(r.program, { op: "strkey_decode", text: ZERO_G, text_base64: asBytes })),
        "bad-request",
      );
    });

    test("asset_order compares issuers by key bytes", async () => {
      assert.deepEqual(ok(await ask(r.program, { op: "asset_order", a: ISSUER_3A, b: ISSUER_00 })), { order: 1 });
      assert.deepEqual(ok(await ask(r.program, { op: "asset_order", a: ISSUER_00, b: ISSUER_00 })), { order: 0 });
    });

    test("pool_id takes the pair in the given order", async () => {
      assert.deepEqual(ok(await ask(r.program, { op: "pool_id", a: POOL_A, b: POOL_B, fee: 30 })), {
        pool_id: POOL_ID,
      });
      assert.equal(errorClass(await ask(r.program, { op: "pool_id", a: POOL_B, b: POOL_A, fee: 30 })), "rejected");
    });

    test("map_sort and map_validate", async () => {
      const entries = [
        { key: "AAAADwAAAAFiAAAA", val: "AAAAAwAAAAA=" },
        { key: "AAAADwAAAAJhYQAA", val: "AAAAAwAAAAE=" },
        { key: "AAAADwAAAAFhAAAA", val: "AAAAAwAAAAI=" },
      ];
      assert.deepEqual(ok(await ask(r.program, { op: "map_sort", entries })), {
        xdr: "AAAAEQAAAAEAAAADAAAADwAAAAFhAAAAAAAAAwAAAAIAAAAPAAAAAmFhAAAAAAADAAAAAQAAAA8AAAABYgAAAAAAAAMAAAAA",
      });
      assert.equal((await ask(r.program, { op: "map_validate", xdr: COLLIDING_MAP })).status, "unsupported");
    });

    test("id_parse reads what the sdk encodes", async () => {
      assert.deepEqual(ok(await ask(r.program, { op: "id_parse", kind: "offer", input: "16" })), { id: "16" });
      assert.deepEqual(ok(await ask(r.program, { op: "id_parse", kind: "offer", input: 16 })), { id: "16" });
      assert.deepEqual(
        ok(await ask(r.program, { op: "id_parse", kind: "muxed", input: "18446744073709551615" })),
        { id: "18446744073709551615" },
      );
      assert.equal((await ask(r.program, { op: "id_parse", kind: "muxed", input: 16 })).status, "unsupported");
      // the number reaches the sdk as the double a caller's literal would be
      const [unsafe] = await exchange(r.program, ['{"id":"u","op":"id_parse","kind":"offer","input":9007199254740993}']);
      assert.ok(unsafe !== undefined);
      if (r.version === "17.4.0") assert.equal(errorClass(unsafe), "rejected");
      else assert.deepEqual(ok(unsafe), { id: "9007199254740992" });
    });

    test("to_native describes the native value", async () => {
      const reply = await ask(r.program, { op: "to_native", xdr: COLLIDING_MAP });
      if (r.version === "17.4.0") {
        assert.equal(errorClass(reply), "rejected");
      } else {
        // 16.x keeps one of the two colliding entries (js-stellar-sdk#1789)
        assert.deepEqual(ok(reply), {
          native: { type: "map", entries: [{ key: { type: "string", value: "1" }, value: { type: "integer", value: "20" } }] },
        });
      }
      const vec = "AAAAEAAAAAEAAAACAAAAAAAAAAEAAAAOAAAAAmhpAAA=";
      assert.deepEqual(ok(await ask(r.program, { op: "to_native", xdr: vec })), {
        native: {
          type: "array",
          items: [
            { type: "bool", value: true },
            { type: "string", value: "hi" },
          ],
        },
      });
    });

    test("protocol errors", async () => {
      const replies = await exchange(r.program, ["not json", '{"op":"hello"}', '{"id":"x","op":"nope"}']);
      assert.deepEqual(
        replies.map((x) => [x.id, x.status, x.error?.class]),
        [
          ["", "error", "bad-request"],
          ["", "error", "bad-request"],
          ["x", "error", "bad-request"],
        ],
      );
    });
  });
}
