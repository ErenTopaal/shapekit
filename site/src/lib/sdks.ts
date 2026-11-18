// the display name of each sdk key of conformance/versions.toml, in the readme's order.
// soroban-sdk and stellar-cli are named as they are, the others as the language their sdk is for
const names: Record<string, string> = {
  js: "JavaScript",
  go: "Go",
  rust: "Rust",
  python: "Python",
  java: "Java",
  dart: "Dart",
  php: "PHP",
  dotnet: ".NET",
  kmp: "Kotlin Multiplatform",
  soroban: "soroban-sdk",
  "stellar-cli": "stellar-cli",
}

// not language sdks: their own name stands alone, without "SDK" after it
const named = new Set(["soroban", "stellar-cli"])

const order = Object.keys(names)
const rank = (k: string) => (order.includes(k) ? order.indexOf(k) : order.length)

// sorts by the readme's order; keys the table does not know go last, by key
export function bySdkOrder<T>(xs: T[], key: (x: T) => string): T[] {
  return xs.toSorted((a, b) => rank(key(a)) - rank(key(b)) || key(a).localeCompare(key(b)))
}

export function sdkName(key: string): string {
  return names[key] ?? key
}

// "the Go SDK", "soroban-sdk"
export function sdkLabel(key: string): string {
  return named.has(key) ? sdkName(key) : `${sdkName(key)} SDK`
}

// one phrase for a list of sdk keys: "the Go, JavaScript and Rust SDKs, soroban-sdk and stellar-cli"
export function sdkList(unsorted: string[]): string {
  const keys = bySdkOrder(unsorted, (k) => k)
  const langs = keys.filter((k) => !named.has(k)).map(sdkName)
  const others = keys.filter((k) => named.has(k)).map(sdkName)
  const parts = [...(langs.length > 0 ? [`the ${and(langs)} ${langs.length === 1 ? "SDK" : "SDKs"}`] : []), ...others]
  return and(parts)
}

export function and(xs: string[]): string {
  if (xs.length <= 1) return xs.join("")
  return `${xs.slice(0, -1).join(", ")} and ${xs.at(-1)}`
}
