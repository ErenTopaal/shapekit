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

  // every block of the package that ran at least once since the process started, as
  // <path inside the package>:<start offset>-<end offset>
  snapshot(): string[] {
    const taken = this.post("Profiler.takePreciseCoverage") as { result: ScriptCoverage[] };
    const out = new Set<string>();
    for (const script of taken.result) {
      if (!script.url.startsWith(this.prefix)) continue;
      const path = script.url.slice(this.prefix.length);
      for (const fn of script.functions) {
        for (const r of fn.ranges) {
          if (r.count > 0) out.add(`${path}:${r.startOffset}-${r.endOffset}`);
        }
      }
    }
    return [...out].sort();
  }
}
