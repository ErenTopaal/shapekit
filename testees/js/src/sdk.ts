// loads @stellar/stellar-sdk from wherever node resolves it next to the compiled testee, and
// reads the versions hello reports from the installed packages rather than from constants.

import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const SDK_PACKAGE = "@stellar/stellar-sdk";
const XDR_PACKAGE = "@stellar/js-xdr";

// the sdk is typed loosely on purpose: one compiled source runs against 16.x and 17.x, whose
// apis differ, so every use goes through a feature check instead of one version's typings
export type Loose = any;

export interface LoadedSdk {
  module: Loose;
  version: string;
  // the installed package's directory
  dir: string;
  components: { [name: string]: string };
}

interface PackageJson {
  name?: string;
  version?: string;
}

function readPackage(path: string): PackageJson {
  return JSON.parse(readFileSync(path, "utf8")) as PackageJson;
}

// the package.json of the package that contains `file`
function owningPackage(file: string, name: string): { dir: string; pkg: PackageJson } {
  let dir = dirname(file);
  for (;;) {
    const candidate = join(dir, "package.json");
    if (existsSync(candidate)) {
      const pkg = readPackage(candidate);
      if (pkg.name === name) {
        return { dir, pkg };
      }
    }
    const parent = dirname(dir);
    if (parent === dir) {
      throw new Error(`no package.json named ${name} above ${file}`);
    }
    dir = parent;
  }
}

// node's lookup for a dependency of the package in `fromDir`
function dependencyVersion(fromDir: string, name: string): string {
  let dir = fromDir;
  for (;;) {
    const candidate = join(dir, "node_modules", name, "package.json");
    if (existsSync(candidate)) {
      const { version } = readPackage(candidate);
      if (typeof version !== "string") {
        throw new Error(`${candidate} has no version`);
      }
      return version;
    }
    const parent = dirname(dir);
    if (parent === dir) {
      throw new Error(`${name} is not installed for the package in ${fromDir}`);
    }
    dir = parent;
  }
}

export async function loadSdk(): Promise<LoadedSdk> {
  const entry = fileURLToPath(import.meta.resolve(SDK_PACKAGE));
  const { dir, pkg } = owningPackage(entry, SDK_PACKAGE);
  if (typeof pkg.version !== "string") {
    throw new Error(`${SDK_PACKAGE} package.json has no version`);
  }
  const module: Loose = await import(SDK_PACKAGE);
  return {
    module,
    version: pkg.version,
    dir,
    components: { [XDR_PACKAGE]: dependencyVersion(dir, XDR_PACKAGE) },
  };
}
