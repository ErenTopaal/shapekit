// prints, as one json array, every mutant stryker's instrumenter generates in the files named
// on the command line (paths relative to the working directory), with the default mutators
// and no ignorers: the population `shapekit conformance seed-check --sdk js` samples from.
import { readFileSync } from "node:fs";
import { Instrumenter } from "@stryker-mutator/instrumenter";

const quiet = () => false;
const toStderr = (...a) => console.error(...a);
const logger = {
  isTraceEnabled: quiet,
  isDebugEnabled: quiet,
  isInfoEnabled: quiet,
  isWarnEnabled: () => true,
  isErrorEnabled: () => true,
  isFatalEnabled: () => true,
  trace() {},
  debug() {},
  info() {},
  warn: toStderr,
  error: toStderr,
  fatal: toStderr,
};

const files = process.argv.slice(2).map((name) => ({
  name,
  content: readFileSync(name, "utf8"),
  mutate: true,
}));
const result = await new Instrumenter(logger).instrument(files, {
  plugins: null,
  excludedMutations: [],
  ignorers: [],
  noHeader: false,
});
const mutants = result.mutants.map(({ fileName, location, mutatorName, replacement }) => ({
  fileName,
  location,
  mutatorName,
  replacement,
}));
process.stdout.write(JSON.stringify(mutants) + "\n");
