// the lossless description of a native javascript value that to_native returns
// (docs/testee-protocol.md, to_native).

import type { Json } from "./protocol.js";

export function describeNative(value: unknown): Json {
  if (value === null || value === undefined) {
    return { type: "null" };
  }
  if (typeof value === "boolean") {
    return { type: "bool", value };
  }
  if (typeof value === "bigint") {
    return { type: "integer", value: value.toString() };
  }
  if (typeof value === "number") {
    if (Number.isInteger(value)) {
      return { type: "integer", value: BigInt(value).toString() };
    }
    return { type: "object", class: "Number", text: String(value) };
  }
  if (typeof value === "string") {
    return { type: "string", value };
  }
  if (value instanceof Uint8Array) {
    return { type: "bytes", value: Buffer.from(value).toString("base64") };
  }
  if (Array.isArray(value)) {
    return { type: "array", items: value.map(describeNative) };
  }
  if (value instanceof Map) {
    return {
      type: "map",
      entries: [...value].map(([k, v]) => ({ key: describeNative(k), value: describeNative(v) })),
    };
  }
  if (typeof value === "object") {
    const proto: unknown = Object.getPrototypeOf(value);
    if (proto === Object.prototype || proto === null) {
      // a plain object's keys are always strings, which is exactly what a caller iterates
      return {
        type: "map",
        entries: Object.entries(value).map(([k, v]) => ({ key: describeNative(k), value: describeNative(v) })),
      };
    }
  }
  return { type: "object", class: className(value), text: String(value) };
}

function className(value: unknown): string {
  if (typeof value === "object" && value !== null) {
    const ctor: unknown = (value as { constructor?: unknown }).constructor;
    if (typeof ctor === "function" && ctor.name !== "") {
      return ctor.name;
    }
  }
  return typeof value;
}
