// turns one request line into a response.

import type { Ops } from "./ops.js";
import type { Request } from "./ops.js";
import type { Response } from "./protocol.js";
import { BadRequest, respond } from "./protocol.js";

interface Parsed {
  id: string;
  request: Request | BadRequest;
}

type Reviver = (this: unknown, key: string, value: unknown, context?: { source?: string }) => unknown;

export function parseLine(line: string): Parsed {
  // json.parse hands the reviver each number's source text; only the top-level `input` keeps
  // it, so an id above 2^53 reaches id_parse as the digits the runner wrote
  const sources: { holder: unknown; source: string }[] = [];
  const reviver: Reviver = function (key, value, context) {
    if (key === "input" && typeof value === "number" && context?.source !== undefined) {
      sources.push({ holder: this, source: context.source });
    }
    return value;
  };
  let value: unknown;
  try {
    value = JSON.parse(line, reviver as (key: string, value: unknown) => unknown);
  } catch (err) {
    return { id: "", request: new BadRequest(`request is not json: ${String(err)}`) };
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return { id: "", request: new BadRequest("request is not a json object") };
  }
  const members = value as { [key: string]: unknown };
  const id = members["id"];
  if (typeof id !== "string") {
    return { id: "", request: new BadRequest("request has no string id") };
  }
  const op = members["op"];
  if (typeof op !== "string") {
    return { id, request: new BadRequest("request has no string op") };
  }
  const inputSource = sources.find((s) => s.holder === value)?.source;
  return { id, request: { op, members, inputSource } };
}

export function handle(ops: Ops, line: string): Response {
  const { id, request } = parseLine(line);
  return respond(id, () => {
    if (request instanceof BadRequest) {
      throw request;
    }
    return ops.run(request);
  });
}
