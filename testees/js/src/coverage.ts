// coverage_snapshot: v8's precise block coverage of the sdk package, collected in this process
// through the inspector. it runs only when SHAPEKIT_COVERAGE=1 is set, because counting every
// block slows the testee down; without it hello does not list the op.

import { Session } from "node:inspector";
import { pathToFileURL } from "node:url";

interface Range {
  startOffset: number;
  endOffset: number;
  count: number;
}

interface ScriptCoverage {
  url: string;
  functions: { ranges: Range[] }[];
}

export class Coverage {
  private readonly session: Session;
  private readonly prefix: string;

  private constructor(session: Session, packageDir: string) {
    this.session = session;
    this.prefix = pathToFileURL(packageDir).href + "/";
  }

  // in-process inspector messages are answered before post returns, so the calls are
  // synchronous and the request loop stays one line in, one line out
  private post(method: string, params?: object): unknown {
    let result: unknown;
    let failure: Error | null = null;
    let answered = false;
    this.session.post(method, params ?? {}, (err: Error | null, r?: unknown) => {
      failure = err;
      result = r;
      answered = true;
    });
    if (!answered) throw new Error(`the inspector did not answer ${method} synchronously`);
    if (failure) throw failure;
    return result;
  }

  // started before the sdk is imported, so its module code is counted too
  static start(): Coverage | undefined {
    if (process.env.SHAPEKIT_COVERAGE !== "1") return undefined;
    const session = new Session();
    session.connect();
    const c = new Coverage(session, "");
    c.post("Profiler.enable");
    c.post("Profiler.startPreciseCoverage", { callCount: true, detailed: true });
    return c;
  }

  forPackage(packageDir: string): Coverage {
    return new Coverage(this.session, packageDir);
  }

  // v8's takePreciseCoverage resets the counters on every call, so what each call reports is
  // folded into `seen`: per file, every range v8 has listed with whether it has run since the
  // process started. v8 lists a nested block only while its count differs from its parent's,
  // so a block a call reports with a count of 0, that an earlier call left out under a parent
  // that ran, has run: it counts as run by the smallest range already seen that contains it
  private readonly seen = new Map<string, Map<string, { start: number; end: number; ran: boolean }>>();

  private ranBefore(file: Map<string, { start: number; end: number; ran: boolean }>, start: number, end: number): boolean {
    let best: { start: number; end: number; ran: boolean } | undefined;
    for (const r of file.values()) {
      if (r.start <= start && end <= r.end && (best === undefined || r.end - r.start < best.end - best.start)) {
        best = r;
      }
    }
    return best?.ran ?? false;
  }

  // the blocks of the package as <path inside the package>:<start offset>-<end offset>: those
  // that ran at least once since the process started, and those v8 has reported with a count of
  // 0 that never ran since. a block in neither list ran exactly as often as the smallest listed
  // block around it
  snapshot(): { ranges: string[]; uncovered: string[] } {
    const taken = this.post("Profiler.takePreciseCoverage") as { result: ScriptCoverage[] };
    for (const script of taken.result) {
      if (!script.url.startsWith(this.prefix)) continue;
      const path = script.url.slice(this.prefix.length);
      let file = this.seen.get(path);
      if (file === undefined) {
        file = new Map();
        this.seen.set(path, file);
      }
      // decided against what earlier calls left, before this call's answers go in
      const updates: [string, { start: number; end: number; ran: boolean }][] = [];
      for (const fn of script.functions) {
        for (const r of fn.ranges) {
          const key = `${r.startOffset}-${r.endOffset}`;
          const known = file.get(key);
          const ran = r.count > 0 || (known ? known.ran : this.ranBefore(file, r.startOffset, r.endOffset));
          updates.push([key, { start: r.startOffset, end: r.endOffset, ran }]);
        }
      }
      // a block known not to have run that this call leaves out ran as often as the smallest
      // block of this call around it
      const listed = new Map(updates.map(([k, v]) => [k, v]));
      const counts = script.functions.flatMap((fn) => fn.ranges);
      for (const [key, r] of file) {
        if (r.ran || listed.has(key)) continue;
        let best: Range | undefined;
        for (const c of counts) {
          if (
            c.startOffset <= r.start &&
            r.end <= c.endOffset &&
            (best === undefined || c.endOffset - c.startOffset < best.endOffset - best.startOffset)
          ) {
            best = c;
          }
        }
        if (best !== undefined && best.count > 0) r.ran = true;
      }
      for (const [key, value] of updates) file.set(key, value);
    }
    const ran: string[] = [];
    const never: string[] = [];
    for (const [path, file] of this.seen) {
      for (const [key, r] of file) (r.ran ? ran : never).push(`${path}:${key}`);
    }
    return { ranges: ran.sort(), uncovered: never.sort() };
  }
}
