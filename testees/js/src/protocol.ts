// the testee protocol's message shapes (docs/testee-protocol.md) and the errors ops raise to
// pick a response status.

export const PROTOCOL_VERSION = 2;

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

export type ErrorClass = "rejected" | "panic" | "bad-request" | "internal";

export type Response =
  | { id: string; status: "ok"; result: { [key: string]: Json } }
  | { id: string; status: "error"; error: { class: ErrorClass; message: string } }
  | { id: string; status: "unsupported"; reason: string };

// the sdk version under test has no api for this op or this input
export class Unsupported extends Error {}

// the request does not follow the protocol
export class BadRequest extends Error {}

// an exception thrown by an sdk call, kept apart from the testee's own failures so that only
// the sdk's errors count as the sdk refusing an input
export class SdkError extends Error {
  readonly original: unknown;

  constructor(original: unknown) {
    super(describe(original));
    this.original = original;
  }

  // v8 throws a RangeError when the stack runs out; that is a crash, not a rejection
  get isPanic(): boolean {
    return (
      this.original instanceof RangeError &&
      /maximum call stack size exceeded/i.test(this.original.message)
    );
  }
}

export function describe(err: unknown): string {
  if (err instanceof Error) {
    return `${err.name}: ${err.message}`;
  }
  return String(err);
}

// runs one sdk call and tags anything it throws as the sdk's
export function sdk<T>(call: () => T): T {
  try {
    return call();
  } catch (err) {
    if (err instanceof SdkError || err instanceof Unsupported) {
      throw err;
    }
    throw new SdkError(err);
  }
}

export function respond(id: string, run: () => { [key: string]: Json }): Response {
  try {
    return { id, status: "ok", result: run() };
  } catch (err) {
    if (err instanceof Unsupported) {
      return { id, status: "unsupported", reason: err.message };
    }
    if (err instanceof BadRequest) {
      return { id, status: "error", error: { class: "bad-request", message: err.message } };
    }
    if (err instanceof SdkError) {
      const errorClass = err.isPanic ? "panic" : "rejected";
      return { id, status: "error", error: { class: errorClass, message: err.message } };
    }
    return { id, status: "error", error: { class: "internal", message: describe(err) } };
  }
}
