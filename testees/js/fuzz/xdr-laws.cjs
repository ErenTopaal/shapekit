// the coverage-guided target for @stellar/stellar-sdk, run by `shapekit fuzz native --target
// js` through jazzer.js. it checks the encoding laws of the law library: a value the sdk
// decodes must encode back to exactly the input bytes, and the input with a word more or a byte
// less must not decode. the first input byte picks the type from the configured list; the rest
// is the xdr. classes are named as in testees/rust/fuzz/fuzz_targets/xdr_laws.rs.
//
// SHAPEKIT_FUZZ_CONFIG names a json file with the types, the sdk install directory to load
// the package from and a file of classes not to report again.
"use strict";

const { readFileSync } = require("node:fs");
const { join } = require("node:path");

const configPath = process.env.SHAPEKIT_FUZZ_CONFIG;
if (!configPath) throw new Error("SHAPEKIT_FUZZ_CONFIG is not set");
const config = JSON.parse(readFileSync(configPath, "utf8"));
const sdk = require(join(config.sdk, "node_modules", "@stellar", "stellar-sdk"));
const skip = new Set(
  (config.skip ? readFileSync(config.skip, "utf8") : "")
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l !== ""),
);
// 17.x generates XdrValue classes with fromXdr and toXdr; 16.x has js-xdr's fromXDR and toXDR
const types = config.types.map((t) => {
  const type = sdk.xdr[t.name];
  if (!type) throw new Error(`the sdk has no xdr type ${t.name}`);
  const values = typeof type.fromXdr === "function";
  return {
    name: t.name,
    decode: (b) => (values ? type.fromXdr(b) : type.fromXDR(Buffer.from(b))),
    encode: (v) => Buffer.from(values ? v.toXdr() : v.toXDR()),
  };
});

class Violation extends Error {}

function crashed(err) {
  return err instanceof RangeError && /Maximum call stack size exceeded/.test(String(err.message));
}

// the value, or undefined when the sdk refuses the bytes; a crash is not a refusal
function decode(ty, bytes) {
  try {
    return ty.decode(bytes);
  } catch (err) {
    if (crashed(err)) throw err;
    return undefined;
  }
}

function report(cls, detail) {
  if (!skip.has(cls)) throw new Violation(`shapekit-class ${cls}: ${detail}`);
}

function wordKind(w) {
  if (w.length === 4 && w[0] === 0 && w[1] === 0 && w[2] === 0 && (w[3] === 0 || w[3] === 1)) {
    return String(w[3]);
  }
  return "x";
}

function firstWordChange(input, again) {
  const n = Math.max(input.length, again.length);
  for (let i = 0; i * 4 < n; i++) {
    const a = input.subarray(i * 4, i * 4 + 4);
    const b = again.subarray(i * 4, i * 4 + 4);
    if (Buffer.compare(a, b) !== 0) return `${wordKind(a)}->${wordKind(b)}`;
  }
  return "x->x";
}

module.exports.fuzz = function (data) {
  if (data.length === 0) return;
  const ty = types[data[0] % types.length];
  const input = Buffer.from(data.subarray(1));
  let value;
  try {
    value = decode(ty, input);
    if (value === undefined) return;
    let again;
    try {
      again = ty.encode(value);
    } catch (err) {
      if (crashed(err)) throw err;
      report(`${ty.name}:roundtrip:encode-error`, String(err));
      return;
    }
    if (Buffer.compare(again, input) !== 0) {
      report(
        `${ty.name}:roundtrip:${firstWordChange(input, again)}`,
        `re-encoding gives ${again.length} bytes differing from the ${input.length} input bytes`,
      );
    }
    if (decode(ty, Buffer.concat([input, Buffer.alloc(4)])) !== undefined) {
      report(`${ty.name}:noncanonical:longer`, "the input plus a trailing zero word decodes");
    }
    if (input.length > 0 && decode(ty, input.subarray(0, input.length - 1)) !== undefined) {
      report(`${ty.name}:noncanonical:shorter`, "the input without its last byte decodes");
    }
  } catch (err) {
    if (err instanceof Violation) throw err;
    report(`${ty.name}:panic`, String(err));
  }
};
