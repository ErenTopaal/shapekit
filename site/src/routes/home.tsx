import { Fragment, useEffect, useRef, type CSSProperties } from "react"
import { useLocation, useNavigate } from "react-router"
import "@fontsource-variable/plus-jakarta-sans"
import "@fontsource/instrument-serif/400-italic.css"
import "@/landing/landing.css"
import type { Route } from "./+types/home"
import { loadLandingStats, type LandingFinding, type LandingStats } from "@/landing/stats"
import { setupLanding } from "@/landing/setup"
import { breakSegments, statement } from "@/landing/statement"
import { sdkName } from "@/lib/sdks"

// the landing page. every number on it is read at build time from the json files `shapekit site
// data` writes (src/landing/stats.ts); the motion and the interactive parts are in
// src/landing/setup.ts, and every design value is in src/landing/tokens.css.
export async function loader() {
  return loadLandingStats()
}

export function meta() {
  return [
    { title: "shapekit: rare Stellar ledger data for SDK and indexer tests" },
    {
      name: "description",
      content: "A shared test kit for Stellar SDKs and indexers: rare but valid ledger shapes, recorded on a real stellar-core, and the tools that run them through your code.",
    },
    { tagName: "link", rel: "canonical", href: "/" },
  ]
}

const css = (o: Record<string, string | number>) => o as CSSProperties
const f = (n: number) => n.toLocaleString("en-US")

const sevKey: Record<string, string> = { "data-loss": "dl", "wrong-data": "wd", divergence: "dv", robustness: "rb" }
const sevLabel: Record<string, string> = { "data-loss": "data loss", "wrong-data": "wrong data", divergence: "divergence", robustness: "robustness" }
const sevOrder = ["data-loss", "wrong-data", "divergence", "robustness"]
const bySeverity = (a: LandingFinding, b: LandingFinding) =>
  sevOrder.indexOf(a.severity) - sevOrder.indexOf(b.severity) || a.id.localeCompare(b.id, "en", { numeric: true })
// the periodic-table symbol of each sdk key; the name comes from the site's one name table
const sdkSymbol: Record<string, string> = {
  rust: "Rs",
  go: "Go",
  js: "Js",
  python: "Py",
  java: "Jv",
  dart: "Dt",
  php: "Ph",
  dotnet: "Ne",
  kmp: "Kt",
  soroban: "Sb",
  "stellar-cli": "Cl",
}
const chipName: Record<string, string> = { kmp: "Kotlin" }
const componentKind: Record<string, string> = {
  "wallet-backend": "Indexer",
  "go-stellar-sdk": "Go SDK",
  "js-stellar-sdk": "JavaScript SDK",
  "rs-stellar-xdr": "Rust XDR",
  "stellar-core": "Validator",
  "stellar_flutter_sdk": "Dart SDK",
  "soneso/stellar-php-sdk": "PHP SDK",
  "stellar-dotnet-sdk": ".NET SDK",
  "kmp-stellar-sdk": "Kotlin Multiplatform SDK",
  "stellar-cli": "CLI",
  "stellar-etl": "Indexer",
}
const componentOrder = Object.keys(componentKind)
const plural = (n: number, one: string, many: string) => `${f(n)} ${n === 1 ? one : many}`

function Kit({ s }: { s: LandingStats }) {
  const finds = [...s.findings].toSorted((a, b) => sevOrder.indexOf(a.severity) - sevOrder.indexOf(b.severity) || Number(b.filed) - Number(a.filed) || a.id.localeCompare(b.id, "en", { numeric: true }))
  const bySev = (k: string) => s.findings.filter((x) => x.severity === k).length
  return (
    <figure className="kit" aria-label={`What shapekit does: a corpus of ${f(s.entries)} shapes runs against ${s.sdks.length} SDK targets and ${s.runs.length} indexers and finds bugs`}>
      <div className="kit-col">
        <span className="kit-step"><b>1</b>The corpus</span>
        <span className="kit-h"><span className="tnum" data-count="">{f(s.entries)}</span> rare but valid shapes</span>
        <canvas className="kit-canvas" aria-hidden="true"></canvas>
        <span className="kit-key"><span data-g="0"><i style={css({ "--sw": "var(--c-teal)" })}></i>{f(s.coreEmitted)} written by stellar-core</span><span data-g="1"><i style={css({ "--sw": "var(--c-sun)" })}></i>{f(s.synthetic)} synthetic</span><span data-g="2"><i style={css({ "--sw": "var(--c-flare)" })}></i>{f(s.refused)} refused by stellar-core</span></span>
        <a className="arrow-link kit-go" href="#coverage">Coverage</a>
      </div>
      <span className="kit-flow f1" aria-hidden="true"></span>
      <div className="kit-col">
        <span className="kit-step"><b>2</b>Runs through your code</span>
        <span className="kit-h">SDKs and indexers</span>
        <div>
          <div className="runner"><span className="runner-h">SDK conformance runner <span>{plural(s.sdks.length, "target", "targets")}</span></span><span className="chips" style={css({ "--d": "1100ms" })}>{s.sdks.map((k, i) => <span key={k.key} className="chipx tip" data-tip={`${k.package} ${k.current}`} style={css({ "--i": i })}>{chipName[k.key] ?? sdkName(k.key)}</span>)}</span></div>
          <div className="runner"><span className="runner-h">Indexer replay harness <span>{plural(s.runs.length, "indexer", "indexers")}</span></span><span className="chips" style={css({ "--d": "1500ms" })}>{s.runs.map((r, i) => <span key={r.target} className="chipx tip" data-tip={r.note ? r.note.charAt(0).toUpperCase() + r.note.slice(1) : r.target} style={css({ "--i": s.sdks.length + i })}>{r.target}</span>)}</span></div>
        </div>
        <span className="kit-note">Plus indexerlint for Go indexer code.</span>
        <a className="arrow-link kit-go" href="#targets">Every target</a>
      </div>
      <span className="kit-flow f2" aria-hidden="true"></span>
      <div className="kit-col k3">
        <span className="kit-step"><b>3</b>Finds what breaks</span>
        <span className="kit-h"><span className="tnum" data-count="">{f(s.findings.length)}</span> findings</span>
        <span className="finds" style={css({ "--d": "1900ms" })}>
          {finds.map((x, i) => <a key={x.id} className={`fi tip sev-${sevKey[x.severity] ?? "dv"}${x.filed ? "" : " o"}`} href="#findings" data-tip={tipOf(x)} aria-label={x.id} style={css({ "--i": i })}></a>)}
        </span>
        <span className="kit-sev">{sevOrder.map((k) => <span key={k}><i className={`sev-${sevKey[k]}`}></i>{bySev(k)} {sevLabel[k]}</span>)}</span>
        <span className="kit-key"><span><i style={css({ "--sw": "var(--c-ink)" })}></i>{plural(s.issuesFiled, "issue", "issues")} filed</span><span><i style={css({ "--sw": "transparent", border: "var(--bw-strong) solid var(--c-ink)" })}></i>{s.issuesReported} reproduced</span></span>
        <a className="arrow-link kit-go" href="#findings">Every finding</a>
      </div>
    </figure>
  )
}

function FindingsHead({ s }: { s: LandingStats }) {
  const components = new Set(s.findings.map((x) => x.component)).size
  const n = s.findingsIntended.length
  const intended = n === 0 ? "" : n === 1 ? ` One more record, ${s.findingsIntended[0]}, is intended behavior and is not filed.` : ` ${n} more records, ${s.findingsIntended.join(", ")}, are intended behavior and are not filed.`
  return (
    <div className="fd-big" data-r="">
      <span className="fd-num tnum" data-count="">{f(s.findings.length)}</span>
      <span className="fd-num-cap"><strong>findings in {components} components.</strong><span>{plural(s.issuesFiled, "issue", "issues")} filed upstream by shapekit. {plural(s.issuesReported, "issue", "issues")} reported before by others, now reproduced.{intended}</span></span>
    </div>
  )
}

const defaultFinding = (s: LandingStats) => s.findings.find((x) => x.id === "RS-1") ?? s.findings[0]

const tipOf = (x: LandingFinding) => {
  const st = statement(x.title)
  return `${x.id}${st.question ? " (question)" : ""}: ${st.text}`
}

function FindingRows({ s }: { s: LandingStats }) {
  const comps = [...new Set(s.findings.map((x) => x.component))].toSorted((a, b) => {
    const ia = componentOrder.indexOf(a)
    const ib = componentOrder.indexOf(b)
    return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib)
  })
  const first = defaultFinding(s)
  return (
    <div className="fd-rows">
      {comps.map((c) => (
        <div key={c} className="fd-row"><span className="fd-comp">{c}<span>{componentKind[c] ?? ""}</span></span><div className="fd-tiles">
          {s.findings.filter((x) => x.component === c).toSorted(bySeverity).map((x) => ({ x, st: statement(x.title) })).map(({ x, st }) => (
            <button key={x.id} type="button" className={`tile sev-${sevKey[x.severity] ?? "dv"}${x.filed ? "" : " dup"}`} data-id={x.id} data-sev={sevLabel[x.severity] ?? x.severity} data-comp={x.component} data-st={x.filed ? "Filed by shapekit" : "Reported before, reproduced by shapekit"} data-link={x.link ?? undefined} data-ref={x.ref ?? undefined} data-title={st.text} data-q={st.question ? "true" : undefined} aria-label={x.id} aria-pressed={x.id === first?.id ? "true" : "false"}></button>
          ))}
        </div></div>
      ))}
    </div>
  )
}

function Inspector({ s }: { s: LandingStats }) {
  const x = defaultFinding(s)
  const st = statement(x?.title ?? "")
  return (
    <aside className="insp" aria-live="polite" data-r="" style={css({ "--i": "2" })}>
      <div className="insp-top"><span className="insp-id" data-o="id" style={css({ "--n": x?.id.length ?? 4 })}>{x?.id}</span><span className={`insp-sev sev-${sevKey[x?.severity ?? ""] ?? "dv"}`} data-o="sev">{sevLabel[x?.severity ?? ""] ?? x?.severity}</span></div>
      <p className="insp-title"><span className="insp-q" data-o="q" hidden={!st.question}>Question</span><span data-o="title">{breakSegments(st.text).map((p, i) => <Fragment key={i}>{i > 0 && <wbr />}{p}</Fragment>)}</span></p>
      <dl className="insp-meta">
        <dt>Component</dt><dd data-o="comp">{x?.component}</dd>
        <dt>Status</dt><dd data-o="st">{x?.filed ? "Filed by shapekit" : "Reported before, reproduced by shapekit"}</dd>
        <dt>Issue</dt><dd data-o="ref">{x?.ref}</dd>
      </dl>
      <div className="insp-acts">
        <a className="btn" data-o="link" href={x?.link ?? undefined} hidden={!x?.link}>Open the issue <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M7 17L17 7M9 7h8v8"></path></svg></a>
        <a className="arrow-link" href="/dashboard/findings">All findings</a>
      </div>
    </aside>
  )
}

function SdkTable({ s }: { s: LandingStats }) {
  return (
    <div className="ptable" data-r="">
      {s.sdks.map((k, i) => (
        <div key={k.key} className="el"><span className="el-n"><span>{String(i + 1).padStart(2, "0")}</span><span>{k.current}</span></span><span className="el-sym">{sdkSymbol[k.key] ?? k.key.slice(0, 2)}</span><span><span className="el-name">{sdkName(k.key)}</span><span className="el-pkg">{k.package}</span></span></div>
      ))}
      <div className="el el-key"><strong data-count="">{s.sdks.length}</strong><span>SDK targets at their current release. Older versions run too, to find where a bug starts and ends.</span></div>
    </div>
  )
}

function IndexerRuns({ s }: { s: LandingStats }) {
  const entries = s.runs[0]?.entries ?? 0
  return (
    <>
      <div className="ix-head" data-r="">
        <h3 className="h2" style={css({ fontSize: "var(--fs-2xl)" })}>{plural(s.runs.length, "indexer", "indexers")}. {f(entries)} entries <span className="acc">each.</span></h3>
        <a className="arrow-link" href="/dashboard/indexer">Indexer results</a>
      </div>
      {s.runs.map((r) => {
        const segs: [string, number, string][] = [
          ["s-pass", r.pass, "pass"],
          ["s-mis", r.mishandled, "mishandled"],
          ["s-drop", r.dropped, "dropped"],
          ["s-oos", r.outOfScope, "out of scope"],
          ["s-ne", r.notEvaluated, "not evaluated"],
        ]
        const shown = segs.filter((g) => g[1] > 0)
        return (
          <div key={r.target} className="ix-row" data-r="">
            <div className="ix-top"><span className="ix-name">{r.target}<span>{r.note}</span></span><span className="ix-pass"><b className="tnum" data-count="">{f(r.pass)}</b> pass</span></div>
            <p className="sr-only">{`${r.target}: ${shown.map((g) => `${f(g[1])} ${g[2]}`).join(", ")}, of ${f(r.entries)} entries`}</p>
            <div className="bar" aria-hidden="true">
              {shown.map((g, i) => <span key={g[0]} className={`seg ${g[0]}`} style={css({ "--n": g[1], "--i": i })}>{g[0] === "s-pass" ? `${f(g[1])} pass` : g[1] / r.entries >= 0.03 || g[0] === "s-ne" ? f(g[1]) : ""}</span>)}
            </div>
          </div>
        )
      })}
    </>
  )
}

function Score({ s }: { s: LandingStats }) {
  const n = s.bugs.length
  return (
    <div className="score" data-r="">
      <div><b data-count="">{n}</b><span>known bugs, run again from their own versions</span></div>
      <div style={css({ "--k": "var(--c-flare-ink)" })}><b>{n} of {n}</b><span>fail on the version before the fix</span></div>
      <div style={css({ "--k": "var(--c-teal)" })}><b>{n} of {n}</b><span>pass on the version with the fix</span></div>
      <div className="pairs" aria-hidden="true" style={css({ gridTemplateColumns: `repeat(${n},minmax(0,1fr))` })}>{s.bugs.map((b, i) => <i key={b.id} style={css({ "--i": i })}></i>)}</div>
    </div>
  )
}

function Dock({ s }: { s: LandingStats }) {
  const groups = [...new Set(s.bugs.map((b) => b.component))]
  return (
    <div className="dock" role="tablist" aria-label="Known bugs">
      {groups.map((g) => (
        <div key={g} className="dock-g"><p className="dock-gh"><b>{g}</b><span>{s.bugs.find((b) => b.component === g)?.harness ? "Indexer, by commit" : "SDK, by release"}</span></p>
          {s.bugs.map((b, i) => b.component !== g ? null : (
            <button key={b.id} type="button" className="tr-b" role="tab" data-i={i} aria-selected={i === 0 ? "true" : "false"}><em>{String(i + 1).padStart(2, "0")}</em>{b.title}<span className="tr-dots" aria-hidden="true"><i className="df"></i><i className="dp"></i></span></button>
          ))}
        </div>
      ))}
    </div>
  )
}

function Version({ v }: { v: { label: string; commit: string | null } | undefined }) {
  if (!v) return null
  return v.commit ? <a href={v.commit}>{v.label}</a> : <b>{v.label}</b>
}

const guideClass: Record<string, string> = { yes: "v-yes", partial: "v-part", no: "v-no" }

function Guide({ s }: { s: LandingStats }) {
  const count = (v: string) => s.guide.filter((g) => g.validated === v).length
  return (
    <>
      <fieldset className="gd-filters" aria-label="Show fields">
        <button type="button" className="chip" data-f="all" aria-pressed="true">All <b>{s.guide.length}</b></button>
        <button type="button" className="chip" data-f="no" aria-pressed="false"><i style={css({ "--sev": "var(--c-flare)" })} aria-hidden="true"></i>Not validated <b>{count("no")}</b></button>
        <button type="button" className="chip" data-f="part" aria-pressed="false"><i style={css({ "--sev": "var(--c-sun)" })} aria-hidden="true"></i>Partly validated <b>{count("partial")}</b></button>
        <button type="button" className="chip" data-f="yes" aria-pressed="false"><i style={css({ "--sev": "var(--c-line-strong)" })} aria-hidden="true"></i>Validated <b>{count("yes")}</b></button>
      </fieldset>
      <p className="ms" data-f="all">
        {s.guide.map((g) => <span key={g.id}><span className={`fld ${guideClass[g.validated] ?? "v-yes"}`}>{g.id}</span> </span>)}
      </p>
    </>
  )
}

export default function Home({ loaderData: s }: Route.ComponentProps) {
  const { hash } = useLocation()
  const navigate = useNavigate()
  const root = useRef<HTMLDivElement>(null)
  // links from before the dashboard moved under /dashboard (/#/sdk) still land there
  useEffect(() => {
    if (hash.startsWith("#/")) void navigate(`/dashboard${hash.slice(1)}`, { replace: true })
  }, [hash, navigate])
  useEffect(() => {
    if (!root.current) return
    return setupLanding(root.current, {
      kit: [s.coreEmitted, s.synthetic, s.refused],
      arms: { covered: s.arms.covered, refused: s.arms.refused, never: s.arms.never, other: s.arms.other },
      tiers: s.tiers,
      bugs: s.bugs,
      rehearsedProtocol: s.protocol + 1,
    })
  }, [s])
  const pct = ((s.arms.covered / s.arms.reachable) * 100).toFixed(1)
  const js = s.sdks.find((k) => k.key === "js")
  const tierRows = Object.entries(s.tiers).map(([p, t]) => [Number(p), t[0] + t[1] + t[2] + t[3]] as const).toSorted((a, b) => a[0] - b[0])
  const recorded = tierRows.reduce((a, r) => a + r[1], 0)
  const skyLabel = `${f(recorded)} recorded corpus entries by protocol: ${tierRows.map(([p, n]) => `${n} at protocol ${p}`).join(", ")}. Protocol ${s.protocol + 1} is rehearsed.`
  return (
    <div className="sk" ref={root}>

    <header className="nav">
    <span className="nav-prog" aria-hidden="true"></span>
    <div className="wrap nav-in">
    <a className="logo" href="/" aria-label="shapekit home">
    <svg width="28" height="28" viewBox="0 0 16 16" aria-hidden="true"><rect width="16" height="16" rx="3" fill="currentColor"></rect><rect x="3" y="3" width="4" height="4" rx="1" fill="var(--c-paper)"></rect><rect x="9" y="3" width="4" height="4" rx="1" fill="var(--c-paper)"></rect><rect x="3" y="9" width="4" height="4" rx="1" fill="var(--c-paper)"></rect><rect x="9.5" y="9.5" width="3" height="3" rx="0.6" fill="none" stroke="var(--c-paper)"></rect></svg>
    <span>shapekit</span>
    </a>
    <nav className="nav-links" aria-label="Main">
    <a className="nav-a" href="/docs">Docs</a>
    <a className="nav-a nav-hide" href="/dashboard">Dashboard</a>
    <a className="nav-a nav-hide" href="https://github.com/ErenTopaal/shapekit">GitHub</a>
    <a className="btn btn-ink nav-cta" href="#start">Get started</a>
    </nav>
    </div>
    </header>

    <main>

    <section className="hero" aria-labelledby="hero-h">
    <div className="wrap">
    <div className="hero-top">
    <h1 id="hero-h" data-r="">Test your Stellar code on the ledger data that <span className="acc">breaks it.</span></h1>
    <div className="hero-side">
    <p data-r="" style={css({ "--i": "1" })}><strong>shapekit is a shared test kit for Stellar SDKs and indexers.</strong> It holds rare but valid ledger shapes, most of them recorded on a real stellar-core, and the tools that run them through your code.</p>
    <div className="hero-actions" data-r="" style={css({ "--i": "2" })}>
    <a className="btn btn-ink" href="/docs">Read the docs <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"></path></svg></a>
    <a className="btn btn-line" href="https://github.com/ErenTopaal/shapekit">GitHub</a>
    </div>
    <div className="hero-tags" data-r="" style={css({ "--i": "3" })}><span className="tag">Corpus {s.corpusVersion}</span><span className="tag">Protocol {s.protocol}</span><span className="tag">Apache-2.0</span></div>
    </div>
    </div>

    <Kit s={s} />
    <div className="repro" data-r="">
    <p>Every finding comes with its smallest corpus entry and the command that runs it again.</p>
    <div className="repro-links"><a className="pill" href="/dashboard/findings">All findings</a><a className="pill" href="/dashboard/corpus">Browse the corpus</a><a className="pill" href="/docs/guide/indexer-hardening">Indexer-hardening guide</a></div>
    </div>
    </div>
    </section>

    <section className="prob theme-night" id="problem" aria-labelledby="pm-h">
    <div className="wrap">
    <div className="head">
    <p className="kicker" data-r="">The problem</p>
    <h2 id="pm-h" className="h2" data-r="" style={css({ "--i": "1" })}>Rare data is still <span className="acc">valid</span> data.</h2>
    <p className="lede" data-r="" style={css({ "--i": "2" })}>Shapes like these are rare on the network, so tests do not cover them. Each one below broke software that the ecosystem uses. Each team found its bug alone.</p>
    </div>
    <div className="pb" data-r="">
    <div className="pb-list" role="tablist" aria-label="Six shapes that broke software">
    <button type="button" className="pb-tab" role="tab" data-k="0" aria-selected="true"><span className="pb-n">01</span><span className="pb-t"><b>Signed-payload signer</b><span>wallet-backend</span></span><span className="pb-bar"><span></span></span></button>
    <button type="button" className="pb-tab" role="tab" data-k="1" aria-selected="false"><span className="pb-n">02</span><span className="pb-t"><b>Muxed Soroban source</b><span>wallet-backend</span></span><span className="pb-bar"><span></span></span></button>
    <button type="button" className="pb-tab" role="tab" data-k="2" aria-selected="false"><span className="pb-n">03</span><span className="pb-t"><b>Asset Contract trustline</b><span>wallet-backend</span></span><span className="pb-bar"><span></span></span></button>
    <button type="button" className="pb-tab" role="tab" data-k="3" aria-selected="false"><span className="pb-n">04</span><span className="pb-t"><b>Colliding map keys</b><span>js-stellar-sdk</span></span><span className="pb-bar"><span></span></span></button>
    <button type="button" className="pb-tab" role="tab" data-k="4" aria-selected="false"><span className="pb-n">05</span><span className="pb-t"><b>Issuer byte order</b><span>js-stellar-sdk</span></span><span className="pb-bar"><span></span></span></button>
    <button type="button" className="pb-tab" role="tab" data-k="5" aria-selected="false"><span className="pb-n">06</span><span className="pb-t"><b>Deep auth tree</b><span>go-stellar-sdk</span></span><span className="pb-bar"><span></span></span></button>
    </div>
    <div className="pb-stage" aria-live="polite">

    <div className="pb-scene on show" data-k="0" role="tabpanel">
    <div className="pb-viz v1" aria-hidden="true"><div className="v1-wrap">
    <div className="v1-track"><span className="v1-head"><b>wallet-backend</b></span><span className="v1-l v1-ok" style={css({ "--dl": "200ms" })}>63</span><span className="v1-l v1-ok" style={css({ "--dl": "1000ms" })}>64</span><span className="v1-l v1-bad">65</span><span className="v1-l v1-ghost">66</span></div>
    <p className="v1-note"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round"><path d="M20 12a8 8 0 1 1-2.3-5.7"></path><path d="M20 4v5h-5"></path></svg>Retrying ledger 65. It never reaches 66.</p>
    </div></div>
    <div className="pb-text">
    <p className="pb-when"><span className="acc">When</span> a SetOptions adds a signed-payload signer,</p>
    <p className="pb-then">wallet-backend retried ledger 65 forever.</p>
    <p className="pb-meta">CAP-40 · <a href="https://github.com/stellar/wallet-backend/pull/729">wallet-backend#729</a></p>
    </div>
    </div>

    <div className="pb-scene" data-k="1" role="tabpanel">
    <div className="pb-viz v2" aria-hidden="true"><div className="v2-wrap">
    <div className="v2-in"><span className="vk">One Soroban transaction</span><span className="v2-tok" style={css({ "--dl": "300ms", "--flyv": "7.5rem" })}>Muxed source <i>M</i></span><span className="v2-tok" style={css({ "--dl": "1300ms", "--flyv": "4.5rem" })}>Its base account <i>G</i></span></div>
    <div className="vbox"><span className="vk">transactions_accounts</span><span className="v2-row v2-r1"><b>base account</b><span>this transaction</span></span><span className="v2-row v2-r2"><b>base account</b><span>this transaction</span></span><span className="v2-err">Duplicate key, error 23505</span><span className="v2-stop"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><rect x="5" y="5" width="14" height="14" rx="2"></rect></svg>Ingestion stopped</span></div>
    </div></div>
    <div className="pb-text">
    <p className="pb-when"><span className="acc">When</span> a muxed account is the source of a Soroban call,</p>
    <p className="pb-then">wallet-backend stored one account twice and stopped.</p>
    <p className="pb-meta">CAP-27 · <a href="https://github.com/stellar/wallet-backend/pull/753">wallet-backend#753</a></p>
    </div>
    </div>

    <div className="pb-scene" data-k="2" role="tabpanel">
    <div className="pb-viz v3" aria-hidden="true"><div className="v3-wrap">
    <div className="vbox v3-src va"><span className="vk">Ledger 67, transaction meta</span><b>trustline-created</b><span>TRST, limit 9223372036854775807</span></div>
    <span className="v3-wire"><b>no event</b></span>
    <div className="vbox va" style={css({ "--dl": "300ms" })}><span className="vk">wallet-backend rows</span><span className="v3-empty">No trustline row</span></div>
    </div></div>
    <div className="pb-text">
    <p className="pb-when"><span className="acc">When</span> the Stellar Asset Contract creates a trustline with no event,</p>
    <p className="pb-then">wallet-backend wrote no trustline row.</p>
    <p className="pb-meta">CAP-73 · <a href="https://github.com/stellar/wallet-backend/issues/731">wallet-backend#731</a></p>
    </div>
    </div>

    <div className="pb-scene" data-k="3" role="tabpanel">
    <div className="pb-viz v4" aria-hidden="true"><div className="v4-wrap">
    <div className="vbox va"><span className="vk">Soroban map</span><span className="v4-e v4-e1"><i>u32</i>1<em>→</em>10</span><span className="v4-e"><i>symbol</i>"1"<em>→</em>20</span><span className="v4-why va" style={css({ "--dl": "900ms" })}>In JavaScript, both keys are "1".</span></div>
    <span className="v4-fn va" style={css({ "--dl": "500ms" })}>scValToNative</span>
    <div className="vbox va" style={css({ "--dl": "1700ms" })}><span className="vk">JavaScript object</span><code className="v4-obj-c">&#123; "1": 20 &#125;</code><span className="v4-lost">The entry 1 → 10 is gone.</span></div>
    </div></div>
    <div className="pb-text">
    <p className="pb-when"><span className="acc">When</span> two map keys become one key in JavaScript,</p>
    <p className="pb-then">js-stellar-sdk merged the two entries into one.</p>
    <p className="pb-meta">Soroban · <a href="https://github.com/stellar/js-stellar-sdk/issues/1789">js-stellar-sdk#1789</a></p>
    </div>
    </div>

    <div className="pb-scene" data-k="4" role="tabpanel">
    <div className="pb-viz v5" aria-hidden="true"><div className="v5-wrap">
    <div className="vbox va"><span className="vk">The network: key bytes</span><div className="v5-list"><span className="v5-i">GAAA… <small>00 00 00</small></span><span className="v5-i">GA5D… <small>3a 3a 3a</small></span></div><span className="v5-ok vk">Valid pool pair</span></div>
    <div className="vbox va" style={css({ "--dl": "400ms" })}><span className="vk">js-stellar-sdk 17.2.0: text</span><div className="v5-list"><span className="v5-i v5-b">GAAA… <small>00 00 00</small></span><span className="v5-i v5-a">GA5D… <small>3a 3a 3a</small></span></div><span className="v5-x">Refused</span></div>
    </div></div>
    <div className="pb-text">
    <p className="pb-when"><span className="acc">When</span> the network orders pool assets by key bytes, not by text,</p>
    <p className="pb-then">js-stellar-sdk refused valid pool pairs.</p>
    <p className="pb-meta">CAP-38 · <a href="https://github.com/stellar/js-stellar-sdk/pull/1766">js-stellar-sdk#1766</a></p>
    </div>
    </div>

    <div className="pb-scene" data-k="5" role="tabpanel">
    <div className="pb-viz v6" aria-hidden="true"><div className="v6-wrap">
    <div className="v6-top"><span className="vk">Auth tree depth that go-stellar-sdk decoded</span><span className="v6-num"><span className="v6-c" data-to="988">0</span><small>levels</small></span></div>
    <div className="v6-scale"><span className="v6-fill"></span><span className="v6-cap"><b>stellar-core stops at 493</b></span></div>
    <div className="v6-ends"><span>0</span><span>988</span></div>
    </div></div>
    <div className="pb-text">
    <p className="pb-when"><span className="acc">When</span> an auth chain is 988 levels deep and stellar-core stops at 493,</p>
    <p className="pb-then">go-stellar-sdk decoded it anyway.</p>
    <p className="pb-meta">Soroban · <a href="https://github.com/stellar/go-stellar-sdk/issues/6047">go-stellar-sdk#6047</a></p>
    </div>
    </div>

    </div>
    </div>
    <div className="pm-outro" data-r=""><p>shapekit gives every team <span className="acc">the same shapes</span>, and the tools to run them.</p><a className="arrow-link" href="/dashboard/corpus">See every shape in the corpus</a></div>
    </div>
    </section>

    <section className="flow" aria-labelledby="fl-h">
    <div className="wrap">
    <div className="head" style={css({ marginBottom: "0" })}>
    <p className="kicker" data-r="">How it works</p>
    <h2 id="fl-h" className="h2" data-r="" style={css({ "--i": "1" })}>From a real stellar-core to <span className="acc">your</span> code.</h2>
    <p className="fl-entry" data-r="" style={css({ "--i": "2" })}>Follow one chain: <b>cap40.signed-payload-signer.p30</b></p>
    </div>
    <div className="fl-stage" aria-hidden="true">
    <svg className="fl-svg" viewBox="0 0 1200 340" preserveAspectRatio="none">
    <path className="fl-base" d="M40 170 H540 C610 170 610 70 680 70 H840 C910 70 910 170 980 170 H1160"></path>
    <path className="fl-base" d="M540 170 C610 170 610 270 680 270 H840 C910 270 910 170 980 170"></path>
    <path className="fl-run" id="fl-top" pathLength="1" vectorEffect="non-scaling-stroke" d="M40 170 H540 C610 170 610 70 680 70 H840 C910 70 910 170 980 170 H1160"></path>
    <path className="fl-run" id="fl-bot" pathLength="1" vectorEffect="non-scaling-stroke" d="M40 170 H540 C610 170 610 270 680 270 H840 C910 270 910 170 980 170 H1160"></path>
    </svg>
    <span className="fl-node" data-at="0.01" style={css({ left: "3.33%", top: "50%" })}></span>
    <span className="fl-node" data-at="0.22" style={css({ left: "25%", top: "50%" })}></span>
    <span className="fl-node" data-at="0.6" style={css({ left: "63.33%", top: "20.6%" })}></span>
    <span className="fl-node" data-at="0.6" style={css({ left: "63.33%", top: "79.4%" })}></span>
    <span className="fl-node" data-at="0.96" style={css({ left: "96.67%", top: "50%" })}></span>
    <div className="fl-lab below" data-at="0.01" style={css({ left: "3.33%", top: "50%" })}><h3>Record</h3><p>A recipe runs on stellar-core {s.coreVersion}, on a Docker network with no route off the host.</p></div>
    <div className="fl-lab above" data-at="0.22" style={css({ left: "25%", top: "50%" })}><h3>Check</h3><p>Each entry re-encodes to the same bytes under both passphrases and agrees with the goldens of stellar-core.</p></div>
    <div className="fl-lab above" data-at="0.6" style={css({ left: "63.33%", top: "20.6%" })}><h3>SDK conformance runner</h3><p>Sends each entry to an SDK. Writes a divergence matrix.</p></div>
    <div className="fl-lab below" data-at="0.6" style={css({ left: "63.33%", top: "79.4%" })}><h3>Replay harness</h3><p>Serves the recorded chain to an indexer. Reads back what it stored.</p></div>
    <div className="fl-lab end" data-at="0.96" style={css({ left: "96.67%", top: "50%" })}><h3>Report</h3><p>Groups failures by finding, with the smallest entry and the command that runs it again.</p></div>
    </div>
    <ol className="fl-list">
    <li><h3>Record</h3><p>A recipe runs on stellar-core {s.coreVersion}, on a Docker network with no route off the host.</p></li>
    <li><h3>Check</h3><p>Each entry re-encodes to the same bytes under both passphrases and agrees with the goldens of stellar-core.</p></li>
    <li><h3>SDK conformance runner</h3><p>Sends each entry to an SDK. Writes a divergence matrix.</p></li>
    <li><h3>Replay harness</h3><p>Serves the recorded chain to an indexer. Reads back what it stored.</p></li>
    <li><h3>Report</h3><p>Groups failures by finding, with the smallest entry and the command that runs it again.</p></li>
    </ol>
    </div>
    </section>

    <section className="truth surf-2" aria-labelledby="ok-h">
    <div className="wrap">
    <div className="head">
    <p className="kicker" data-r="">How it knows what is correct</p>
    <h2 id="ok-h" className="h2" data-r="" style={css({ "--i": "1" })}>The right answer is written down <span className="acc">before</span> any SDK runs.</h2>
    <p className="lede" data-r="" style={css({ "--i": "2" })}>Every entry in the corpus carries its own expected answer. shapekit compares your code with that answer, not with another SDK.</p>
    </div>

    <div className="ok" data-r="">
    <div className="ok-left">
    <p className="ok-cap">The {f(s.entries)} entries, by expected answer. Select a group to open one real entry.</p>
    <div className="dec-bar" aria-hidden="true"><i className="d-acc" style={css({ "--n": s.decode.accept })}></i><i className="d-rej" style={css({ "--n": s.decode.reject })}></i><i className="d-def" style={css({ "--n": s.decode.defined })}></i></div>
    <div className="dec" role="tablist" aria-label="Expected answers">
    <button type="button" className="dec-seg d-acc" role="tab" data-e="0" aria-selected="true" style={css({ "--n": s.decode.accept })}><b className="tnum" data-count="">{f(s.decode.accept)}</b><span>must accept</span></button>
    <button type="button" className="dec-seg d-rej" role="tab" data-e="1" aria-selected="false" style={css({ "--n": s.decode.reject })}><b className="tnum" data-count="">{f(s.decode.reject)}</b><span>must reject</span></button>
    <button type="button" className="dec-seg d-def" role="tab" data-e="2" aria-selected="false" style={css({ "--n": s.decode.defined })}><b className="tnum" data-count="">{f(s.decode.defined)}</b><span>no single answer</span></button>
    </div>
    <ol className="gates">
    <li><span className="gate-n">1</span><div><b>stellar-core wrote most of them.</b> {f(s.coreEmitted)} entries are bytes that stellar-core wrote into a ledger.</div></li>
    <li><span className="gate-n">2</span><div><b>They round-trip.</b> Each entry decodes and re-encodes to the same bytes under both passphrases. <code>shapekit manifest validate</code></div></li>
    <li><span className="gate-n">3</span><div><b>They agree with stellar-core's own tests.</b> The corpus is checked against the goldens of stellar-core. <code>shapekit goldens agree</code></div></li>
    </ol>
    </div>

    <div className="pass-stack">
    <article className="pp on" data-e="0" aria-label="Entry that must be accepted">
    <span className="stamp st-acc" aria-hidden="true">Must accept</span>
    <p className="pp-k">Corpus entry</p>
    <h3 className="pp-id">cap40.signed-payload-signer.p28-account-four-payload-signers</h3>
    <p className="pp-what">An account entry with four signed-payload signers: 1, 4, 32 and 64 byte payloads, one of them sponsored.</p>
    <dl className="pp-rows">
    <dt>Bytes from</dt><dd>stellar-core {s.coreVersion}, ledger protocol 28, chain cap40.signed-payload-signer.p28</dd>
    <dt>Expected</dt><dd>Decode it. Re-encode it to the same bytes. Render it as the expected XDR-JSON.</dd>
    <dt>An SDK passes when</dt><dd>its bytes and its JSON are equal to the expected ones.</dd>
    </dl>
    </article>
    <article className="pp" data-e="1" aria-label="Entry that must be rejected">
    <span className="stamp st-rej" aria-hidden="true">Must reject</span>
    <p className="pp-k">Corpus entry</p>
    <h3 className="pp-id">cap23.claimable-balances.synth-strkey-b-invalid</h3>
    <p className="pp-what">Claimable balance IDs written as strkeys that are not valid: wrong lengths, set trailing bits, padding and lowercase.</p>
    <dl className="pp-rows">
    <dt>Bytes from</dt><dd>Synthetic. A strkey is valid only if re-encoding it gives back the same text (SEP-23 1.4.0).</dd>
    <dt>Expected</dt><dd>Refuse each one, as stellar-strkey 1.0.0 does.</dd>
    <dt>An SDK passes when</dt><dd>it refuses the text. go-stellar-sdk accepted one of them: finding <a href="https://github.com/stellar/go-stellar-sdk/issues/6046">GO-1</a>.</dd>
    </dl>
    </article>
    <article className="pp" data-e="2" aria-label="Entry with no single correct answer">
    <span className="stamp st-def" aria-hidden="true">Answers recorded</span>
    <p className="pp-k">Corpus entry</p>
    <h3 className="pp-id">soroban.scval-shapes.synth-bool-value-2</h3>
    <p className="pp-what">An ScVal bool with the value word 2. A bool must be 0 or 1, but the references do not agree on this value.</p>
    <dl className="pp-rows">
    <dt>Bytes from</dt><dd>Synthetic. stellar-core reads the word as true. stellar-xdr reads it as false.</dd>
    <dt>Expected</dt><dd>No single answer. shapekit does not take a vote. It keeps every answer in the matrix:</dd>
    </dl>
    <div className="pp-split">
    <div><b className="t-true">true</b><span>stellar-core, Python, Dart, Kotlin</span></div>
    <div><b className="t-false">false</b><span>Rust, .NET</span></div>
    <div><b className="t-no">refuses</b><span>Go, JavaScript, Java, PHP</span></div>
    </div>
    </article>
    </div>
    </div>

    <div className="oracle" data-r="">
    <div className="or-head">
    <p className="oracle-k">For an indexer, the ledger itself is the judge.</p>
    <p>To test an indexer, you need the right answer for each transaction. There are three places to get it.</p>
    </div>
    <div className="src">
    <div className="src-c" style={css({ "--i": "0" })}><span className="src-x">Option 1</span><h3>Another indexer</h3><p>Compare your rows with its rows. If both indexers have the same bug, the bug passes.</p></div>
    <div className="src-c" style={css({ "--i": "1" })}><span className="src-x">Option 2</span><h3>Rows you write by hand</h3><p>You test only the cases that you think of, and each new case is more work.</p></div>
    <div className="src-c me" style={css({ "--i": "2" })}><span className="src-x">shapekit</span><h3>The ledger</h3><p>stellar-core already wrote what each transaction did into the transaction meta. The harness reads the answer from there, so <strong>every one of the {f(s.runs[0]?.entries ?? 0)} entries has its answer</strong>, and nobody writes it by hand.</p></div>
    </div>
    <div className="verd"><p className="verd-k">Then each fact gets one result.</p>
    <div className="vt k-pass" style={css({ "--i": "0" })}><span className="vt-pic" aria-hidden="true"><i></i><i></i></span><b>pass</b><span>The indexer stored the same value.</span></div>
    <div className="vt k-drop" style={css({ "--i": "1" })}><span className="vt-pic" aria-hidden="true"><i></i><i></i></span><b>dropped</b><span>The indexer stored nothing.</span></div>
    <div className="vt k-mis" style={css({ "--i": "2" })}><span className="vt-pic" aria-hidden="true"><i></i><i></i></span><b>mishandled</b><span>A wrong value, an extra fact or a stop.</span></div>
    </div>
    </div>
    </div>
    </section>

    <section className="cov" id="coverage" aria-labelledby="cv-h">
    <div className="wrap">
    <div className="head">
    <p className="kicker" data-r="">Coverage</p>
    <h2 id="cv-h" className="h2" data-r="" style={css({ "--i": "1" })}>Coverage you can <span className="acc">count</span>, square by square.</h2>
    </div>
    <div className="cv-top">
    <div data-r="">
    <span className="cv-num tnum" data-count="">{pct}<small>%</small></span>
    <p className="cv-cap"><strong>{f(s.arms.covered)} of {f(s.arms.reachable)}</strong> union arms that a LedgerCloseMeta can reach have an entry that stellar-core emitted. Each of the {f(s.arms.reachable - s.arms.covered)} gaps has a written reason.</p>
    <fieldset className="cv-keys" aria-label="Highlight union arms">
    <button type="button" className="chip" data-g="cov" aria-pressed="false"><i style={css({ "--sev": "var(--c-teal)" })} aria-hidden="true"></i>Covered <b>{f(s.arms.covered)}</b></button>
    <button type="button" className="chip" data-g="ref" aria-pressed="false"><i style={css({ "--sev": "var(--c-flare)" })} aria-hidden="true"></i>Refused before a ledger <b>{f(s.arms.refused)}</b></button>
    <button type="button" className="chip" data-g="nev" aria-pressed="false"><i style={css({ "--sev": "var(--c-sun)" })} aria-hidden="true"></i>Never emitted by stellar-core <b>{f(s.arms.never)}</b></button>
    <button type="button" className="chip" data-g="oth" aria-pressed="false"><i style={css({ "--sev": "var(--c-slate)" })} aria-hidden="true"></i>Other reasons <b>{f(s.arms.other)}</b></button>
    </fieldset>
    </div>
    <div data-r="" style={css({ "--i": "1" })}><canvas className="cv-canvas" aria-hidden="true"></canvas><p className="sr-only">{`${s.arms.reachable} squares, one for each union arm: ${s.arms.covered} covered, ${s.arms.refused} refused before a ledger, ${s.arms.never} never emitted by stellar-core, ${s.arms.other} for other reasons`}</p></div>
    </div>
    <div className="cv-facts">
    <div className="cv-fact" data-r=""><strong className="tnum" data-count="">{f(s.entries)}</strong><span>corpus entries. stellar-core emitted {f(s.coreEmitted)} of them.</span></div>
    <div className="cv-fact" data-r="" style={css({ "--i": "1" })}><strong className="tnum">{s.caps.covered}/{s.caps.visible}</strong><span>CAPs that an indexer can see have an entry from stellar-core.</span></div>
    <div className="cv-fact" data-r="" style={css({ "--i": "2" })}><strong className="tnum">{s.fields.exercised}/{s.fields.notValidated}</strong><span>fields that stellar-core does not fully validate are in the corpus.</span></div>
    </div>
    <div className="cv-foot"><span><span className="mono">shapekit coverage --check</span> fails when these counts are out of date.</span><a className="arrow-link" href="/dashboard/coverage">Coverage for each CAP</a></div>
    </div>
    </section>

    <section className="found surf-sand" id="findings" aria-labelledby="fd-h">
    <div className="wrap">
    <div className="head">
    <p className="kicker" data-r="">What it found</p>
    <h2 id="fd-h" className="h2" data-r="" style={css({ "--i": "1" })}>Every square is a <span className="acc">real</span> bug report.</h2>
    </div>
    <div className="fd-grid">
    <div>
    <FindingsHead s={s} />
    <div className="fd-board" data-r="" style={css({ "--i": "1" })}>
    <FindingRows s={s} />
    <fieldset className="fd-keys" aria-label="Highlight findings">
    <button type="button" className="chip" data-k="sev" data-v="dl" aria-pressed="false"><i className="sev-dl" aria-hidden="true"></i>Data loss</button>
    <button type="button" className="chip" data-k="sev" data-v="wd" aria-pressed="false"><i className="sev-wd" aria-hidden="true"></i>Wrong data</button>
    <button type="button" className="chip" data-k="sev" data-v="dv" aria-pressed="false"><i className="sev-dv" aria-hidden="true"></i>Divergence</button>
    <button type="button" className="chip" data-k="sev" data-v="rb" aria-pressed="false"><i className="sev-rb" aria-hidden="true"></i>Robustness</button>
    <button type="button" className="chip" data-k="st" data-v="filed" aria-pressed="false"><i className="solid" aria-hidden="true"></i>Filed by shapekit</button>
    <button type="button" className="chip" data-k="st" data-v="dup" aria-pressed="false"><i className="hollow" aria-hidden="true"></i>Reported before</button>
    </fieldset>
    </div>
    </div>
    <Inspector s={s} />
    </div>
    </div>
    </section>

    <section className="targets" id="targets" aria-labelledby="tg-h">
    <div className="wrap">
    <div className="head">
    <p className="kicker" data-r="">What it runs against</p>
    <h2 id="tg-h" className="h2" data-r="" style={css({ "--i": "1" })}>The table of <span className="acc">every</span> SDK it tests.</h2>
    </div>
    <SdkTable s={s} />

    <div className="ix">
    <IndexerRuns s={s} />
    <div className="ix-keys" data-r="">
    <span><i className="s-pass" aria-hidden="true"></i>pass</span>
    <span><i className="s-mis" aria-hidden="true"></i>mishandled</span>
    <span><i className="s-drop" aria-hidden="true"></i>dropped</span>
    <span><i className="s-oos" aria-hidden="true"></i>out of scope</span>
    <span><i className="s-ne" aria-hidden="true"></i>not evaluated</span>
    </div>
    </div>
    </div>
    </section>

    <section className="proof surf-kraft" id="proof" aria-labelledby="pf-h">
    <div className="wrap">
    <div className="head">
    <p className="kicker" data-r="">Proof on real bugs</p>
    <h2 id="pf-h" className="h2" data-r="" style={css({ "--i": "1" })}>It catches bugs that <span className="acc">already</span> happened.</h2>
    <p className="lede" data-r="" style={css({ "--i": "2" })}>Their own teams found and fixed these {s.bugs.length} bugs. shapekit goes back to the version just before each fix and runs the corpus on it. The run must fail there, and pass on the version with the fix.</p>
    </div>
    <Score s={s} />
    <div className="tr" data-r="">
    <Dock s={s} />
    <div className="panel" data-st="before" aria-live="polite">
    <div className="pn-head"><span data-t="comp">{s.bugs[0]?.component}</span><span className="pn-count">Bug <b data-t="n">1</b> of {s.bugs.length}</span></div>
    <div className="pn-top"><a className="pn-title" data-t="title" href={s.bugs[0]?.link}>{s.bugs[0]?.title}</a><p className="pn-obs" data-t="obs">{s.bugs[0]?.observed}</p></div>
    <div className="pn-run">
    <div className="run-line" aria-hidden="true"><span className="run-fix">the fix</span><span className="run-head"></span></div>
    <div className="side side-a"><span className="side-k">Before the fix</span><div className="side-v" data-t="va"><Version v={s.bugs[0]?.before} /></div><span className="res res-f"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round"><path d="M6 6l12 12M18 6L6 18"></path></svg><span data-t="fv">{s.bugs[0]?.verdict}</span></span><p className="side-why" data-t="fw">{s.bugs[0]?.why}</p></div>
    <div className="side side-b"><span className="side-k">With the fix</span><div className="side-v" data-t="vb"><Version v={s.bugs[0]?.after} /></div><span className="res res-p"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><path d="M5 12l5 5 9-10"></path></svg>pass</span><p className="side-why" data-t="pw">{s.bugs[0]?.passWhy}</p><p className="side-wait">Runs next.</p></div>
    </div>
    <div className="pn-foot"><span>Corpus entry <code data-t="entry">{s.bugs[0]?.entry}</code></span><span className="pn-cmd"><code data-t="cmd">{s.bugs[0]?.command}</code> fails if either result is wrong.</span></div>
    </div>
    </div>
    </div>
    </section>

    <section className="guide surf-2" aria-labelledby="gd-h">
    <div className="wrap">
    <div className="head">
    <p className="kicker" data-r="">The indexer-hardening guide</p>
    <h2 id="gd-h" className="h2" data-r="" style={css({ "--i": "1" })}>{s.guide.length} envelope fields. Which ones does stellar-core <span className="acc">check?</span></h2>
    </div>
    <div className="gd-grid">
    <div className="gd-sheet" data-r="">
    <Guide s={s} />
    </div>
    <aside className="gd-side" data-r="" style={css({ "--i": "1" })}>
    <blockquote className="quote">A “yes” does not make a value true.</blockquote>
    <p>A ledger also holds failed transactions. Trust an envelope value only when the transaction and the operation succeeded. Every row of the guide cites the stellar-core source it was read from.</p>
    <a className="btn btn-ink" href="/docs/guide/indexer-hardening" style={css({ justifySelf: "start" })}>Read the guide <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"></path></svg></a>
    <nav className="gd-toc" aria-label="Guide sections">
    <a href="/docs/guide/indexer-hardening#validated-and-unvalidated-fields">Validated and unvalidated fields <span>1</span></a>
    <a href="/docs/guide/indexer-hardening#recommended-caps">Recommended caps <span>2</span></a>
    <a href="/docs/guide/indexer-hardening#participants-from-meta">Participants from meta <span>3</span></a>
    <a href="/docs/guide/indexer-hardening#muxed-and-signed-payload-addresses">Muxed and signed-payload addresses <span>4</span></a>
    <a href="/docs/guide/indexer-hardening#self-audit-checklist">Self-audit checklist <span>5</span></a>
    </nav>
    </aside>
    </div>
    </div>
    </section>

    <section className="rules" aria-labelledby="rl-h">
    <div className="wrap">
    <div className="head">
    <p className="kicker" data-r="">indexerlint</p>
    <h2 id="rl-h" className="h2" data-r="" style={css({ "--i": "1" })}>Three rules from the guide, <span className="acc">checked</span> in your Go code.</h2>
    </div>
    <div className="rl-grid">
    <article className="rl" data-r=""><span className="rl-n" aria-hidden="true">1</span><h3>Do not trust a field that stellar-core does not check.</h3><div className="rl-foot"><span className="rl-code">IDX001</span><a href="/docs/guide/indexer-hardening#validated-and-unvalidated-fields">Guide: fields</a></div></article>
    <article className="rl" data-r="" style={css({ "--i": "1" })}><span className="rl-n" aria-hidden="true">2</span><h3>Put a cap on depth and size when you decode.</h3><div className="rl-foot"><span className="rl-code">IDX002</span><a href="/docs/guide/indexer-hardening#recommended-caps">Guide: caps</a></div></article>
    <article className="rl" data-r="" style={css({ "--i": "2" })}><span className="rl-n" aria-hidden="true">3</span><h3>Take participants from meta, not from the envelope.</h3><div className="rl-foot"><span className="rl-code">IDX003</span><a href="/docs/guide/indexer-hardening#participants-from-meta">Guide: participants</a></div></article>
    </div>
    <div className="rl-end" data-r=""><span>For indexers built on go-stellar-sdk. For other languages, the guide has the same checks as a checklist.</span><a className="arrow-link" href="/docs/linter/indexerlint">Rules, suppressions and baselines</a></div>
    </div>
    </section>

    <section className="fuzz theme-night" aria-labelledby="fz-h">
    <div className="wrap">
    <div className="head" style={css({ marginBottom: "0" })}>
    <p className="kicker" data-r="">Fuzzing</p>
    <h2 id="fz-h" className="h2" data-r="" style={css({ "--i": "1" })}>113,063 inputs. Eight classes. <span className="acc">One</span> cause.</h2>
    </div>
    </div>
    <div className="fz-stage">
    <canvas className="fz-canvas" aria-hidden="true"></canvas><p className="sr-only">Inputs that got different answers flow into eight classes, one for each type that carries a bool, and the eight classes flow into one cause</p>
    <span className="fz-lab fz-big fz-l" style={css({ top: "4%" })}><b className="tnum" data-count="">113,063</b>inputs with different answers</span>
    <span className="fz-lab fz-lane" style={css({ left: "41%", top: "16%" })}>ContractEvent</span>
    <span className="fz-lab fz-lane" style={css({ left: "41%", top: "25.7%" })}>LedgerCloseMeta</span>
    <span className="fz-lab fz-lane" style={css({ left: "41%", top: "35.4%" })}>LedgerEntry</span>
    <span className="fz-lab fz-lane" style={css({ left: "41%", top: "45.1%" })}>ScVal</span>
    <span className="fz-lab fz-lane" style={css({ left: "41%", top: "54.9%" })}>SorobanAuthorizationEntry</span>
    <span className="fz-lab fz-lane" style={css({ left: "41%", top: "64.6%" })}>TransactionEnvelope</span>
    <span className="fz-lab fz-lane" style={css({ left: "41%", top: "74.3%" })}>TransactionResultMeta</span>
    <span className="fz-lab fz-lane" style={css({ left: "41%", top: "84%" })}>TransactionResultMetaV1</span>
    <span className="fz-lab fz-big fz-one fz-r" style={css({ top: "4%" })}><b>1 cause</b>a bool word of 2</span>
    </div>
    <div className="wrap">
    <div className="fz-row">
    <div className="fz-copy" data-r="">
    <p>The differential fuzzer sends the same input to rs-stellar-xdr and go-stellar-sdk. In its first run, 113,063 inputs got different answers. They fell into eight classes, one for each type that carries a bool, and all eight had one cause.</p>
    <a className="arrow-link" href="/docs/operations/differential-fuzzing" style={css({ justifySelf: "start" })}>How the fuzzers work</a>
    </div>
    <div className="fz-copy" data-r="" style={css({ "--i": "1" })}>
    <p>The coverage-guided search runs the Rust, Go and JavaScript SDKs side by side. An input that reaches new code in one SDK goes to the other two. With this feedback, each SDK reached more code in 100,000 inputs:</p>
    <div className="gain">
    <div className="gain-row"><span>Rust</span><span className="tnum">16,010 to 16,128 code ranges</span><b className="tnum">+118</b></div>
    <div className="gain-row"><span>Go</span><span className="tnum">6,446 to 6,529 code ranges</span><b className="tnum">+83</b></div>
    <div className="gain-row"><span>JavaScript</span><span className="tnum">4,086 to 4,106 code ranges</span><b className="tnum">+20</b></div>
    </div>
    </div>
    </div>


    </div>
    </section>

    <section className="keep" aria-labelledby="kp-h">
    <div className="wrap">
    <div className="head">
    <p className="kicker" data-r="">Stays current</p>
    <h2 id="kp-h" className="h2" data-r="" style={css({ "--i": "1" })}>Ready for the <span className="acc">next</span> protocol before it ships.</h2>
    </div>
    <div className="sky" data-r="">
    <div className="sky-cap"><span>Corpus entries by the protocol of their ledger</span><span>Protocol {s.protocol + 1}: rehearsed</span></div>
    <canvas className="sky-canvas" aria-hidden="true"></canvas><p className="sr-only">{skyLabel}</p>
    <div className="sky-tip" aria-hidden="true" data-next={`On the next-protocol build of stellar-core, all 144 transaction test cases pass at every protocol from 0 to ${s.protocol + 1}.`}></div>
    <div className="sky-keys"><span><i style={css({ "--sw": "var(--c-teal)" })}></i>Local stellar-core <b>{f(s.sources.local)}</b></span><span><i style={css({ "--sw": "var(--c-teal-2)" })}></i>stellar-core test goldens <b>{f(s.sources.golden)}</b></span><span><i style={css({ "--sw": "var(--c-sun)" })}></i>Mainnet and testnet history <b>{f(s.sources.history)}</b></span><span><i style={css({ "--sw": "var(--c-flare)" })}></i>Refused by stellar-core <b>{f(s.sources.refused)}</b></span></div>
    <p className="sky-foot">The {f(s.synthetic)} synthetic entries have no ledger, so they have no protocol.</p>
    </div>
    <div className="kp-grid">
    <article className="kp" data-r=""><span className="kp-when">Every Monday</span><h3>Upstream changes open an issue.</h3><p>A scheduled job checks stellar-core, the XDR definitions, every SDK and stellar-cli. A new release opens a tracking issue with the upgrade checklist and a first run against the new version.</p></article>
    <article className="kp" data-r="" style={css({ "--i": "1" })}><span className="kp-when">Before protocol 31</span><h3>The upgrade is already rehearsed.</h3><p>On the next-protocol build of stellar-core, all 144 transaction test cases pass at every protocol from 0 to 31, and 452,555 transaction metas convert with every hash matching.</p></article>
    <article className="kp" data-r="" style={css({ "--i": "2" })}><span className="kp-when">Through protocol 31</span><h3>A named maintainer, then an open runbook.</h3><p>Eren Topal maintains shapekit through the protocol 31 upgrade. The runbook, the corpus format and the interfaces are documented, so anyone can continue.</p></article>
    </div>
    <div className="kp-foot" data-r=""><a className="arrow-link" href="/docs/operations/maintenance">Maintenance</a><a className="arrow-link" href="/docs/operations/protocol-upgrade-runbook">Protocol upgrade runbook</a></div>
    </div>
    </section>

    <section className="start surf-2" id="start" data-p="sdk" aria-labelledby="st-h">
    <div className="wrap">
    <p className="kicker" data-r="">Get started</p>
    <h2 id="st-h" className="st-ask" data-r="" style={css({ "--i": "1" })}><span>I want to</span><span className="st-opts"><button type="button" className="st-opt" data-p="sdk" aria-pressed="true">check an SDK.</button><button type="button" className="st-opt" data-p="idx" aria-pressed="false">replay into an indexer.</button><button type="button" className="st-opt" data-p="go" aria-pressed="false">lint Go indexer code.</button></span></h2>
    <div className="st-panels" data-r="" style={css({ "--i": "2" })}>
    <div className="st-panel p-sdk">
    <ol className="steps">
    <li className="step"><span className="step-note">Check that you have the tools for the JavaScript SDK.</span><div className="cmd"><code>shapekit doctor sdk --sdk js</code><button type="button" className="copy" data-c="shapekit doctor sdk --sdk js">Copy</button></div></li>
    <li className="step"><span className="step-note">Run the corpus against {js ? `${js.package} ${js.current}` : "the JavaScript SDK"}.</span><div className="cmd"><code>shapekit check sdk --sdk js</code><button type="button" className="copy" data-c="shapekit check sdk --sdk js">Copy</button></div></li>
    <li className="step"><span className="step-note">Read the divergence matrix in target/conformance/MATRIX.md.</span></li>
    </ol>
    <div className="st-side"><p>Use --sdk go, rust, python or another SDK. Leave it out to check every SDK.</p><a href="/docs/install">Install shapekit <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"></path></svg></a><a href="/docs/sdk/sdk-ci">Run it in the CI of an SDK <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"></path></svg></a><a href="/docs/sdk/testee-protocol">Put your own SDK behind shapekit <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"></path></svg></a></div>
    </div>
    <div className="st-panel p-idx">
    <ol className="steps">
    <li className="step"><span className="step-note">Pull the pinned database image. You need Linux and Docker.</span><div className="cmd"><code>docker pull "$(scripts/pin.sh image timescaledb)"</code><button type="button" className="copy" data-c='docker pull "$(scripts/pin.sh image timescaledb)"'>Copy</button></div></li>
    <li className="step"><span className="step-note">Replay one chain into wallet-backend.</span><div className="cmd"><code>shapekit check indexer --adapter "adapters/bin/wb-adapter --commit $(scripts/pin.sh git wallet-backend)" --chain cap40.signed-payload-signer.p30</code><button type="button" className="copy" data-c='shapekit check indexer --adapter "adapters/bin/wb-adapter --commit $(scripts/pin.sh git wallet-backend)" --chain cap40.signed-payload-signer.p30'>Copy</button></div></li>
    <li className="step"><span className="step-note">Read the results in harness-out/HARNESS.md.</span></li>
    </ol>
    <div className="st-side"><p>Leave out --chain to replay every chain in the corpus.</p><a href="/docs/install">Install shapekit <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"></path></svg></a><a href="https://github.com/ErenTopaal/shapekit/blob/main/docs/indexer-ci.md">Run it in the CI of an indexer <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"></path></svg></a><a href="/docs/harness/adapter-interface">Put your own indexer behind the harness <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"></path></svg></a></div>
    </div>
    <div className="st-panel p-go">
    <ol className="steps">
    <li className="step"><span className="step-note">Run indexerlint on your Go indexer module.</span><div className="cmd"><code>shapekit lint &lt;your indexer&gt;</code><button type="button" className="copy" data-c="shapekit lint &lt;your indexer&gt;">Copy</button></div></li>
    <li className="step"><span className="step-note">Keep known findings in a baseline file. Fail only on new ones.</span><div className="cmd"><code>shapekit lint &lt;your indexer&gt; --baseline indexerlint-baseline.json --json</code><button type="button" className="copy" data-c="shapekit lint &lt;your indexer&gt; --baseline indexerlint-baseline.json --json">Copy</button></div></li>
    </ol>
    <div className="st-side"><p>indexerlint and the Go toolchain must be on PATH.</p><a href="/docs/install">Install shapekit <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"></path></svg></a><a href="/docs/linter/indexerlint">indexerlint rules <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 12h14M13 6l6 6-6 6"></path></svg></a></div>
    </div>
    </div>
    </div>
    </section>

    </main>

    <footer className="foot">
    <div className="wrap">
    <div className="ft-grid">
    <ul className="ft-facts">
    <li>Corpus {s.corpusVersion} for protocol {s.protocol}: stellar-core {s.coreVersion} and stellar-xdr {s.xdrTag}.</li>
    <li>No telemetry. Toolchains, images and SDK versions are pinned.</li>
    <li>Maintained through protocol 31. The upgrade runbook is already tested on it.</li>
    </ul>
    <div className="ft-col"><h3>Use</h3><ul><li><a href="/docs">Docs</a></li><li><a href="/docs/quick-start">Quick start</a></li><li><a href="/docs/guide/indexer-hardening">Indexer-hardening guide</a></li><li><a href="/docs/corpus/adding-a-shape">Add a shape</a></li></ul></div>
    <div className="ft-col"><h3>Results</h3><ul><li><a href="/dashboard">Dashboard</a></li><li><a href="/dashboard/corpus">Corpus explorer</a></li><li><a href="/dashboard/sdk">SDK matrix</a></li><li><a href="/dashboard/findings">Findings</a></li></ul></div>
    <div className="ft-col"><h3>Project</h3><ul><li><a href="https://github.com/ErenTopaal/shapekit">GitHub</a></li><li><a href="https://github.com/ErenTopaal/shapekit/releases">Releases</a></li><li><a href="https://github.com/ErenTopaal/shapekit/blob/main/STATUS.md">Status log</a></li><li><a href="https://github.com/ErenTopaal/shapekit/blob/main/SECURITY.md">Security policy</a></li></ul></div>
    </div>
    <div className="ft-mark" aria-hidden="true"><svg viewBox="0 0 16 16"><rect width="16" height="16" rx="3" fill="currentColor"></rect><rect x="3" y="3" width="4" height="4" rx="1" fill="var(--c-brand)"></rect><rect x="9" y="3" width="4" height="4" rx="1" fill="var(--c-brand)"></rect><rect x="3" y="9" width="4" height="4" rx="1" fill="var(--c-brand)"></rect><rect x="9.5" y="9.5" width="3" height="3" rx="0.6" fill="none" stroke="var(--c-brand)"></rect></svg>shapekit</div>
    <div className="ft-base"><span>Apache License 2.0</span><span>Maintained by Eren Topal</span></div>
    </div>
    </footer>

    </div>
  )
}
