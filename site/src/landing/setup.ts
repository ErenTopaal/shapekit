// the motion and the interactive parts of the landing page: canvases, reveals, tabs, autoplay and
// count-ups. it runs once in the browser after the prerendered page hydrates and returns a cleanup.
import type { Bug } from "./stats"

export interface SetupData {
  kit: [number, number, number]
  arms: { covered: number; refused: number; never: number; other: number }
  tiers: Record<number, [number, number, number, number]>
  bugs: Bug[]
  rehearsedProtocol: number
}

type Off = () => void

const tokOf = (el: Element, name: string) => getComputedStyle(el).getPropertyValue(name).trim()
function rrect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath()
  ctx.roundRect(x, y, w, h, r)
  ctx.fill()
}
const ease = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2)
function setCanvas(c: HTMLCanvasElement, w: number, h: number) {
  const dpr = Math.min(2, window.devicePixelRatio || 1)
  c.width = Math.round(w * dpr)
  c.height = Math.round(h * dpr)
  c.style.height = `${h}px`
  const ctx = c.getContext("2d")
  if (!ctx) return null
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, w, h)
  return ctx
}
function version(el: HTMLElement | null, v: { label: string; commit: string | null }) {
  if (!el) return
  el.replaceChildren()
  const node = document.createElement(v.commit ? "a" : "b")
  if (v.commit) node.setAttribute("href", v.commit)
  node.textContent = v.label
  el.append(node)
}
const bez = (a: number, b: number, c: number, d: number, t: number) => {
  const u = 1 - t
  return u * u * u * a + 3 * u * u * t * b + 3 * u * t * t * c + t * t * t * d
}

export function setupLanding(root: HTMLElement, data: SetupData): Off {
  const cleanups: Off[] = []
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches
  const tok = (name: string) => tokOf(root, name)
  const msTok = (name: string) => {
    const v = tok(name)
    return v.endsWith("ms") ? parseFloat(v) : parseFloat(v) * 1000
  }
  function on<K extends keyof HTMLElementEventMap>(el: EventTarget | null, ev: K, fn: (e: HTMLElementEventMap[K]) => void, opt?: AddEventListenerOptions) {
    if (!el) return
    const h = fn as EventListener
    el.addEventListener(ev, h, opt)
    cleanups.push(() => el.removeEventListener(ev, h, opt))
  }
  const one = <T extends Element = HTMLElement>(sel: string, base: ParentNode = root) => base.querySelector<T>(sel)
  const all = <T extends Element = HTMLElement>(sel: string, base: ParentNode = root) => Array.from(base.querySelectorAll<T>(sel))
  const later = (fn: () => void, ms: number) => {
    const id = window.setTimeout(fn, ms)
    cleanups.push(() => window.clearTimeout(id))
  }
  function seen(el: Element | null, fn: () => void, threshold = 0.2) {
    if (!el) return
    const o = new IntersectionObserver((es) => {
      if (es[0]?.isIntersecting) {
        fn()
        o.disconnect()
      }
    }, { threshold })
    o.observe(el)
    cleanups.push(() => o.disconnect())
  }
  function inView(el: Element | null, fn: (v: boolean) => void, threshold = 0.1) {
    if (!el) return
    const o = new IntersectionObserver((es) => fn(es[0]?.isIntersecting ?? false), { threshold })
    o.observe(el)
    cleanups.push(() => o.disconnect())
  }
  function tween(ms: number, fn: (t: number) => void, done?: () => void) {
    if (reduce || !ms) {
      fn(1)
      done?.()
      return
    }
    const t0 = performance.now()
    let raf = 0
    const step = (now: number) => {
      const t = Math.min(1, (now - t0) / ms)
      fn(t)
      if (t < 1) raf = requestAnimationFrame(step)
      else done?.()
    }
    step(t0)
    cleanups.push(() => cancelAnimationFrame(raf))
  }

  // reveal on scroll
  root.classList.add("rv")
  const io = new IntersectionObserver((es) => {
    for (const e of es) {
      if (e.isIntersecting) {
        e.target.classList.add("in")
        io.unobserve(e.target)
      }
    }
  }, { rootMargin: "0px 0px -8% 0px", threshold: 0.08 })
  for (const el of all("[data-r]")) io.observe(el)
  cleanups.push(() => io.disconnect())

  // numbers count up to their real value when they come into view
  for (const el of all("[data-count]")) {
    const node = Array.from(el.childNodes).find((c) => c.nodeType === Node.TEXT_NODE && c.nodeValue?.trim())
    if (!node || reduce) continue
    const txt = node.nodeValue ?? ""
    const raw = txt.replace(/,/g, "")
    const end = parseFloat(raw)
    if (Number.isNaN(end)) continue
    const dec = (raw.split(".")[1] ?? "").length
    const comma = txt.includes(",")
    const fmt = (v: number) => {
      const s = v.toFixed(dec)
      return comma ? s.replace(/\B(?=(\d{3})+(?!\d))/g, ",") : s
    }
    node.nodeValue = fmt(0)
    seen(el, () => tween(msTok("--t-count"), (t) => (node.nodeValue = fmt(end * ease(t))), () => (node.nodeValue = txt)), 0.5)
  }

  // nav: how far down the page you are
  const prog = one(".nav-prog")
  const navProg = () => {
    const max = document.documentElement.scrollHeight - window.innerHeight
    prog?.style.setProperty("--p", max > 0 ? String(Math.min(1, window.scrollY / max)) : "0")
  }
  on(window, "scroll", navProg, { passive: true })
  navProg()

  // every control answers the press where it was pressed
  on(root, "pointerdown", (e) => {
    const b = (e.target as Element | null)?.closest<HTMLElement>(".btn,.chip,.dec-seg,.tr-b,.pb-tab,.copy")
    if (!b || reduce) return
    const r = b.getBoundingClientRect()
    b.classList.add("rip")
    b.style.setProperty("--rx", `${e.clientX - r.left}px`)
    b.style.setProperty("--ry", `${e.clientY - r.top}px`)
    b.style.setProperty("--rs", `${Math.max(r.width, r.height) * 2.4}px`)
    b.classList.remove("go")
    void b.offsetWidth
    b.classList.add("go")
  })

  // hero: the corpus as one square per entry, by source
  const kit = one(".kit")
  const kc = one<HTMLCanvasElement>(".kit-canvas")
  const kitKey = one(".kit-key")
  const kitTok = ["--c-teal", "--c-sun", "--c-flare"]
  const kitTotal = data.kit[0] + data.kit[1] + data.kit[2]
  let kt = reduce ? 1 : 0
  let kitHl = -1
  function drawKit() {
    if (!kc) return
    const w = kc.clientWidth
    if (!w) return
    const cols = 44
    const rows = Math.ceil(kitTotal / cols)
    const gap = 2
    const cell = (w - gap * (cols - 1)) / cols
    const ctx = setCanvas(kc, w, rows * cell + (rows - 1) * gap)
    if (!ctx) return
    let i = 0
    data.kit.forEach((count, gi) => {
      ctx.fillStyle = tok(kitTok[gi] ?? "--c-teal")
      for (let k = 0; k < count; k++, i++) {
        const col = i % cols
        const row = Math.floor(i / cols)
        const a = Math.max(0, Math.min(1, kt * 2.2 - (col / cols) * 1.0 - (row / rows) * 0.2))
        if (a <= 0) continue
        ctx.globalAlpha = a * (kitHl >= 0 && kitHl !== gi ? 0.16 : 1)
        const s = cell * (0.5 + 0.5 * a)
        const o = (cell - s) / 2
        rrect(ctx, col * (cell + gap) + o, row * (cell + gap) + o, s, s, Math.min(2, s * 0.25))
      }
    })
    ctx.globalAlpha = 1
  }
  if (kc) {
    tween(1100, (t) => {
      kt = t
      drawKit()
    })
    on(window, "resize", drawKit)
    for (const sp of all("[data-g]", kitKey ?? root)) {
      on(sp, "mouseenter", () => {
        kitHl = Number(sp.getAttribute("data-g"))
        kitKey?.setAttribute("data-hl", "")
        sp.classList.add("hl")
        drawKit()
      })
      on(sp, "mouseleave", () => {
        kitHl = -1
        kitKey?.removeAttribute("data-hl")
        sp.classList.remove("hl")
        drawKit()
      })
    }
  }
  on(kit, "pointermove", (e) => {
    if (!kit) return
    const r = kit.getBoundingClientRect()
    kit.style.setProperty("--mx", `${e.clientX - r.left}px`)
    kit.style.setProperty("--my", `${e.clientY - r.top}px`)
  })

  // problem: six bugs, one scene each
  const pb = one(".pb")
  const pbScenes = all(".pb-scene")
  const pbTabs = all(".pb-tab")
  const v6c = one(".v6-c")
  let v6raf = 0
  const offTimers = new Map<HTMLElement, number>()
  function depth() {
    if (!v6c) return
    cancelAnimationFrame(v6raf)
    const end = Number(v6c.getAttribute("data-to"))
    if (reduce) {
      v6c.textContent = String(end)
      return
    }
    v6c.textContent = "0"
    const t0 = performance.now() + 400
    const step = (now: number) => {
      const t = Math.max(0, Math.min(1, (now - t0) / 2400))
      v6c.textContent = String(Math.round(end * ease(t)))
      if (t < 1) v6raf = requestAnimationFrame(step)
    }
    step(performance.now())
  }
  cleanups.push(() => {
    cancelAnimationFrame(v6raf)
    offTimers.forEach((id) => window.clearTimeout(id))
  })
  function pbShow(k: number) {
    pbScenes.forEach((sc, i) => {
      if (i === k) {
        window.clearTimeout(offTimers.get(sc))
        sc.classList.remove("on")
        void sc.offsetWidth
        sc.classList.add("on", "show")
      } else if (sc.classList.contains("show")) {
        sc.classList.remove("show")
        offTimers.set(sc, window.setTimeout(() => sc.classList.remove("on"), msTok("--t-base") + 60))
      }
    })
    pbTabs.forEach((t, i) => {
      t.setAttribute("aria-selected", i === k ? "true" : "false")
      const bar = one(".pb-bar span", t)
      if (!bar) return
      bar.style.animation = "none"
      void bar.offsetWidth
      bar.style.animation = ""
    })
    if (pbScenes[k]?.querySelector(".v6-c")) depth()
  }
  if (pb) {
    if (reduce) pb.classList.add("static")
    pbTabs.forEach((t, i) => {
      on(t, "click", () => {
        pb.classList.add("hold")
        pbShow(i)
      })
      on(one(".pb-bar span", t), "animationend", () => {
        if (t.getAttribute("aria-selected") === "true") pbShow((i + 1) % pbTabs.length)
      })
    })
    on(pb, "mouseleave", () => pb.classList.remove("hold"))
    if (!reduce) inView(pb, (v) => pb.classList.toggle("run", v), 0.3)
  }

  // correctness: expected-answer tabs open one real entry each
  const decs = all(".dec-seg")
  const pps = all(".pp")
  for (const d of decs) {
    on(d, "click", () => {
      const k = d.getAttribute("data-e")
      decs.forEach((x) => x.setAttribute("aria-selected", x === d ? "true" : "false"))
      pps.forEach((pp) => pp.classList.toggle("on", pp.getAttribute("data-e") === k))
    })
  }

  // findings inspector
  const insp = one(".insp")
  const tiles = all(".tile")
  const sevClass: Record<string, string> = { "data loss": "sev-dl", "wrong data": "sev-wd", divergence: "sev-dv", robustness: "sev-rb" }
  let current: HTMLElement | null = null
  const setText = (sel: string, v: string | null) => {
    const el = insp ? one(sel, insp) : null
    if (el) el.textContent = v ?? ""
  }
  function pick(t: HTMLElement) {
    if (!insp || t === current) return
    current = t
    tiles.forEach((x) => x.setAttribute("aria-pressed", x === t ? "true" : "false"))
    setText('[data-o="id"]', t.getAttribute("data-id"))
    const sev = one('[data-o="sev"]', insp)
    const sv = t.getAttribute("data-sev") ?? ""
    if (sev) {
      sev.textContent = sv
      sev.className = `insp-sev ${sevClass[sv] ?? ""}`
    }
    setText('[data-o="title"]', t.getAttribute("data-title"))
    setText('[data-o="comp"]', t.getAttribute("data-comp"))
    setText('[data-o="st"]', t.getAttribute("data-st"))
    setText('[data-o="ref"]', t.getAttribute("data-ref"))
    const link = one('[data-o="link"]', insp)
    const href = t.getAttribute("data-link")
    if (link) {
      if (href) link.setAttribute("href", href)
      link.hidden = !href
    }
    insp.classList.remove("insp-swap")
    void insp.offsetWidth
    insp.classList.add("insp-swap")
  }
  for (const t of tiles) {
    if (t.getAttribute("aria-pressed") === "true") current = t
    on(t, "click", () => pick(t))
    on(t, "mouseenter", () => pick(t))
    on(t, "focus", () => pick(t))
  }
  const board = one(".fd-board")
  const fchips = all(".fd-keys .chip")
  for (const c of fchips) {
    on(c, "click", () => {
      const k = c.getAttribute("data-k") ?? ""
      const v = c.getAttribute("data-v") ?? ""
      const was = c.getAttribute("aria-pressed") === "true"
      fchips.forEach((x) => x.setAttribute("aria-pressed", "false"))
      board?.removeAttribute("data-sev")
      board?.removeAttribute("data-st")
      if (!was) {
        c.setAttribute("aria-pressed", "true")
        board?.setAttribute(`data-${k}`, v)
      }
    })
  }

  // flow: the line follows the scroll position
  const stage = one(".fl-stage")
  const pTop = one<SVGPathElement>("#fl-top")
  const pBot = one<SVGPathElement>("#fl-bot")
  const flLit = stage ? all(".fl-node, .fl-lab", stage) : []
  let ticking = false
  function flow() {
    ticking = false
    if (!stage || !stage.offsetParent || !pTop || !pBot) return
    const r = stage.getBoundingClientRect()
    const vh = window.innerHeight
    const p = reduce ? 1 : Math.max(0, Math.min(1, (vh * 0.8 - r.top) / (r.height + vh * 0.2)))
    pTop.style.strokeDashoffset = String(1 - p)
    pBot.style.strokeDashoffset = String(1 - p)
    flLit.forEach((n) => n.classList.toggle("lit", p >= parseFloat(n.getAttribute("data-at") ?? "1")))
  }
  const onScroll = () => {
    if (!ticking) {
      ticking = true
      requestAnimationFrame(flow)
    }
  }
  on(window, "scroll", onScroll, { passive: true })
  on(window, "resize", onScroll)
  flow()

  // coverage squares
  const cv = one<HTMLCanvasElement>(".cv-canvas")
  const groups: [string, number, string][] = [
    ["cov", data.arms.covered, "--c-teal"],
    ["ref", data.arms.refused, "--c-flare"],
    ["nev", data.arms.never, "--c-sun"],
    ["oth", data.arms.other, "--c-slate"],
  ]
  const armTotal = groups.reduce((a, g) => a + g[1], 0)
  const cvState: { hl: string | null; pin: string | null; t: number } = { hl: null, pin: null, t: reduce ? 1 : 0 }
  function drawWaffle() {
    if (!cv) return
    const w = cv.clientWidth
    if (!w) return
    const cols = w < 520 ? 20 : 25
    const rows = Math.ceil(armTotal / cols)
    const gap = Math.max(2, Math.round(w / 200))
    const cell = (w - gap * (cols - 1)) / cols
    const ctx = setCanvas(cv, w, rows * cell + (rows - 1) * gap)
    if (!ctx) return
    let i = 0
    const rad = Math.min(4, cell * 0.2)
    const active = cvState.hl ?? cvState.pin
    for (const g of groups) {
      ctx.fillStyle = tok(g[2])
      for (let k = 0; k < g[1]; k++, i++) {
        const appear = Math.max(0, Math.min(1, cvState.t * 1.7 - (i / armTotal) * 0.7))
        if (appear <= 0) continue
        const x = (i % cols) * (cell + gap)
        const y = Math.floor(i / cols) * (cell + gap)
        const s = cell * (0.4 + 0.6 * appear)
        const o = (cell - s) / 2
        ctx.globalAlpha = appear * (active && active !== g[0] ? 0.14 : 1)
        rrect(ctx, x + o, y + o, s, s, rad)
      }
    }
    ctx.globalAlpha = 1
  }
  if (cv) {
    drawWaffle()
    seen(cv, () => tween(1600, (t) => {
      cvState.t = t
      drawWaffle()
    }), 0.2)
    on(window, "resize", drawWaffle)
    const gchips = all(".cv-keys .chip")
    for (const c of gchips) {
      const g = c.getAttribute("data-g")
      on(c, "mouseenter", () => {
        cvState.hl = g
        drawWaffle()
      })
      on(c, "mouseleave", () => {
        cvState.hl = null
        drawWaffle()
      })
      on(c, "click", () => {
        const was = c.getAttribute("aria-pressed") === "true"
        gchips.forEach((x) => x.setAttribute("aria-pressed", "false"))
        cvState.pin = was ? null : g
        if (!was) c.setAttribute("aria-pressed", "true")
        drawWaffle()
      })
    }
  }

  // proof: run each known bug again, before and with its fix
  const panel = one(".panel")
  const trBtns = all(".tr-b")
  const pairs = all(".pairs i")
  let trCur = 0
  let trTimer = 0
  let trVisible = false
  let trHold = false
  const q = (name: string) => (panel ? one(`[data-t="${name}"]`, panel) : null)
  function trFill(i: number) {
    const b = data.bugs[i]
    if (!panel || !b) return
    trCur = i
    trBtns.forEach((x, n) => x.setAttribute("aria-selected", n === i ? "true" : "false"))
    pairs.forEach((x, n) => x.classList.toggle("on", n === i))
    const set = (k: string, v: string) => {
      const el = q(k)
      if (el) el.textContent = v
    }
    set("comp", b.component)
    set("n", String(i + 1))
    set("title", b.title)
    q("title")?.setAttribute("href", b.link)
    set("obs", b.observed)
    version(q("va"), b.before)
    version(q("vb"), b.after)
    set("fv", b.verdict)
    set("fw", b.why)
    set("pw", b.passWhy)
    set("entry", b.entry)
    set("cmd", b.command)
    panel.setAttribute("data-st", "before")
    panel.classList.remove("swap")
    void panel.offsetWidth
    panel.classList.add("swap")
  }
  const trStop = () => window.clearTimeout(trTimer)
  function trRun() {
    trStop()
    if (reduce || trHold || !trVisible || !panel) return
    const step = msTok("--t-trial")
    trTimer = window.setTimeout(() => {
      panel.setAttribute("data-st", "after")
      trTimer = window.setTimeout(() => {
        trFill((trCur + 1) % data.bugs.length)
        trRun()
      }, step * 1.6)
    }, step * 1.2)
  }
  cleanups.push(trStop)
  if (panel) {
    trFill(0)
    trBtns.forEach((b, i) => {
      on(b, "click", () => {
        trHold = true
        trStop()
        trFill(i)
        later(() => {
          if (trCur === i) panel.setAttribute("data-st", "after")
        }, msTok("--t-trial") * 1.1)
      })
    })
    on(one(".tr"), "mouseleave", () => {
      if (trHold) {
        trHold = false
        trRun()
      }
    })
    inView(panel, (v) => {
      trVisible = v
      if (v) trRun()
      else trStop()
    }, 0.35)
  }

  // guide filters
  const ms = one(".ms")
  const gf = all(".gd-filters .chip")
  for (const c of gf) {
    on(c, "click", () => {
      ms?.setAttribute("data-f", c.getAttribute("data-f") ?? "all")
      gf.forEach((x) => x.setAttribute("aria-pressed", x === c ? "true" : "false"))
    })
  }

  // fuzz: inputs stream into eight class lanes, and the lanes meet in one cause
  const fz = one<HTMLCanvasElement>(".fz-canvas")
  const fzSec = one(".fuzz")
  let fzRun = false
  let fzRaf = 0
  interface Particle { y0: number; lane: number; t: number; v: number }
  interface Fuzz { w: number; h: number; ctx: CanvasRenderingContext2D; lanes: number[]; X1: number; X2: number; SX: number; SY: number; ps: Particle[]; teal: string; flare: string; line: string }
  function fuzzInit(): Fuzz | null {
    if (!fz || !fzSec) return null
    const w = fz.clientWidth
    const h = fz.clientHeight
    if (!w || !h) return null
    const dpr = Math.min(2, window.devicePixelRatio || 1)
    fz.width = Math.round(w * dpr)
    fz.height = Math.round(h * dpr)
    const ctx = fz.getContext("2d")
    if (!ctx) return null
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    const n = 8
    const lanes: number[] = []
    for (let i = 0; i < n; i++) lanes.push(h * (0.16 + i * (0.68 / (n - 1))) + 10)
    const count = Math.round(Math.min(1400, w * 1.1))
    let seed = 20261112
    const rnd = () => {
      seed = (seed * 1664525 + 1013904223) >>> 0
      return seed / 4294967296
    }
    const ps: Particle[] = []
    for (let j = 0; j < count; j++) ps.push({ y0: h * (0.06 + rnd() * 0.88), lane: Math.floor(rnd() * n), t: rnd(), v: 0.00011 + rnd() * 0.00008 })
    return {
      w, h, ctx, lanes, ps,
      X1: w * 0.4, X2: w * 0.66, SX: w * 0.93, SY: h * 0.55,
      teal: tokOf(fzSec, "--c-teal"), flare: tokOf(fzSec, "--c-flare"), line: tokOf(fzSec, "--c-line-strong"),
    }
  }
  let F: Fuzz | null = null
  let last = 0
  function fuzzFrame(now: number) {
    if (!F) return
    const f = F
    const dt = last ? Math.min(48, now - last) : 16
    last = now
    const c = f.ctx
    const { w, h } = f
    c.clearRect(0, 0, w, h)
    c.globalAlpha = 0.6
    c.strokeStyle = f.line
    c.lineWidth = 1
    for (const ly of f.lanes) {
      c.beginPath()
      c.moveTo(f.X1, ly)
      c.lineTo(f.X2, ly)
      c.bezierCurveTo(w * 0.8, ly, w * 0.8, f.SY, f.SX, f.SY)
      c.stroke()
    }
    for (const p of f.ps) {
      if (!reduce) {
        p.t += p.v * dt
        if (p.t >= 1) {
          p.t -= 1
          p.lane = (p.lane + 3) % 8
        }
      }
      const t = p.t
      const ly = f.lanes[p.lane] ?? 0
      let x: number
      let y: number
      if (t < 0.38) {
        const k = t / 0.38
        x = bez(0, f.X1 * 0.55, f.X1 * 0.55, f.X1, k)
        y = bez(p.y0, p.y0, ly, ly, k)
        c.fillStyle = f.teal
        c.globalAlpha = 0.35 + 0.4 * k
      } else if (t < 0.66) {
        const k2 = (t - 0.38) / 0.28
        x = f.X1 + (f.X2 - f.X1) * k2
        y = ly
        c.fillStyle = f.teal
        c.globalAlpha = 0.9
      } else {
        const k3 = (t - 0.66) / 0.34
        x = bez(f.X2, w * 0.8, w * 0.8, f.SX, k3)
        y = bez(ly, ly, f.SY, f.SY, k3)
        c.fillStyle = k3 > 0.45 ? f.flare : f.teal
        c.globalAlpha = 0.95
      }
      c.fillRect(x - 1.5, y - 1.5, 3, 3)
    }
    c.globalAlpha = 0.22 + 0.12 * Math.sin(now / 400)
    c.fillStyle = f.flare
    rrect(c, f.SX - 22, f.SY - 22, 44, 44, 12)
    c.globalAlpha = 1
    rrect(c, f.SX - 11, f.SY - 11, 22, 22, 5)
    if (fzRun && !reduce) fzRaf = requestAnimationFrame(fuzzFrame)
  }
  if (fz) {
    F = fuzzInit()
    if (F) fuzzFrame(performance.now())
    inView(fz, (v) => {
      fzRun = v
      cancelAnimationFrame(fzRaf)
      last = 0
      if (fzRun && !reduce) fzRaf = requestAnimationFrame(fuzzFrame)
    }, 0.05)
    cleanups.push(() => cancelAnimationFrame(fzRaf))
    on(window, "resize", () => {
      F = fuzzInit()
      if (F && !fzRun) fuzzFrame(performance.now())
    })
  }

  // stays current: corpus entries by the protocol of their ledger
  const sky = one<HTMLCanvasElement>(".sky-canvas")
  const skyTip = one(".sky-tip")
  const tiers = data.tiers
  const srcNames = ["local stellar-core", "stellar-core test goldens", "mainnet and testnet history", "refused by stellar-core"]
  const srcTok = ["--c-teal", "--c-teal-2", "--c-sun", "--c-flare"]
  const tallest = Math.max(...Object.values(tiers).map((t) => t[0] + t[1] + t[2] + t[3]))
  const next = data.rehearsedProtocol
  let skyT = reduce ? 1 : 0
  let skyHot = -1
  let skyGeo: { slots: number[]; sw: number } | null = null
  function drawSky() {
    if (!sky) return
    const w = sky.clientWidth
    if (!w) return
    const narrow = w < 640
    const slots = [...Object.keys(tiers).map(Number).toSorted((a, b) => a - b), next]
    const sw = w / slots.length
    const per = narrow ? 4 : 10
    const cell = (sw * 0.78) / per
    const gap = Math.max(0.6, cell * 0.16)
    const sq = cell - gap
    const top = 30
    const axis = top + Math.ceil(tallest / per) * cell
    const ctx = setCanvas(sky, w, axis + 44)
    if (!ctx) return
    const font = tok("--f-body")
    skyGeo = { slots, sw }
    ctx.strokeStyle = tok("--c-line-strong")
    ctx.lineWidth = 1.5
    ctx.beginPath()
    ctx.moveTo(0, axis + 1)
    ctx.lineTo(w, axis + 1)
    ctx.stroke()
    ctx.textAlign = "center"
    slots.forEach((p, i) => {
      const x0 = i * sw + (sw - per * cell) / 2
      const cx = i * sw + sw / 2
      const t = tiers[p]
      const dim = skyHot >= 0 && skyHot !== i
      if (p === next) {
        ctx.globalAlpha = 1
        ctx.strokeStyle = tok("--c-flare")
        ctx.setLineDash([4, 4])
        ctx.lineWidth = 1.5
        ctx.beginPath()
        ctx.moveTo(cx, axis)
        ctx.lineTo(cx, top + 4)
        ctx.stroke()
        ctx.setLineDash([])
      }
      if (t) {
        const total = t[0] + t[1] + t[2] + t[3]
        const show = Math.ceil(total * ease(skyT))
        let n = 0
        for (let g = 0; g < 4; g++) {
          ctx.fillStyle = tok(srcTok[g] ?? "--c-teal")
          ctx.globalAlpha = dim ? 0.2 : 1
          for (let k = 0; k < t[g]! && n < show; k++, n++) {
            const col = n % per
            const row = Math.floor(n / per)
            rrect(ctx, x0 + col * cell, axis - (row + 1) * cell + gap / 2, sq, sq, Math.min(1.5, sq * 0.3))
          }
        }
        ctx.globalAlpha = dim ? 0.3 : skyT
        ctx.fillStyle = tok("--c-ink")
        ctx.font = `700 13px ${font}`
        if (!narrow || total === tallest) ctx.fillText(String(total), cx, axis - Math.ceil(total / per) * cell - 8)
      }
      ctx.globalAlpha = 1
      ctx.fillStyle = p === next ? tok("--c-flare-ink") : t ? tok("--c-ink") : tok("--c-ink-3")
      ctx.font = `${t || p === next ? "700 " : "500 "}${narrow ? 11 : 12}px ${font}`
      if (!narrow || i % 2 === 0 || p >= next - 1) ctx.fillText(String(p), cx, axis + 20)
    })
    ctx.globalAlpha = 1
    ctx.textAlign = "left"
    ctx.fillStyle = tok("--c-ink-3")
    ctx.font = `500 12px ${font}`
    ctx.fillText("Protocol", 0, axis + 40)
  }
  function skyLeave() {
    if (skyHot !== -1) {
      skyHot = -1
      drawSky()
    }
    skyTip?.classList.remove("on")
  }
  function skyMove(e: PointerEvent) {
    if (!skyGeo || !sky || !skyTip) return
    const r = sky.getBoundingClientRect()
    const i = Math.max(0, Math.min(skyGeo.slots.length - 1, Math.floor((e.clientX - r.left) / skyGeo.sw)))
    const p = skyGeo.slots[i] ?? 0
    const t = tiers[p]
    if (!t && p !== next) {
      skyLeave()
      return
    }
    if (skyHot !== i) {
      skyHot = i
      drawSky()
    }
    skyTip.replaceChildren()
    const head = document.createElement("b")
    if (!t) {
      head.textContent = `Protocol ${p}`
      skyTip.append(head, skyTip.getAttribute("data-next") ?? "")
    } else {
      const total = t[0] + t[1] + t[2] + t[3]
      head.textContent = `Protocol ${p}: ${total} ${total === 1 ? "entry" : "entries"}`
      skyTip.append(head)
      t.forEach((v, g) => {
        if (!v) return
        skyTip.append(`${v} ${srcNames[g] ?? ""}`, document.createElement("br"))
      })
    }
    const tw = skyTip.offsetWidth
    const px = (i + 0.5) * skyGeo.sw - tw / 2
    skyTip.style.left = `${Math.max(0, Math.min(r.width - tw, px))}px`
    skyTip.style.top = `${sky.offsetTop + 8}px`
    skyTip.classList.add("on")
  }
  if (sky) {
    drawSky()
    seen(sky, () => tween(1800, (t) => {
      skyT = t
      drawSky()
    }), 0.25)
    on(sky, "pointermove", skyMove)
    on(sky, "pointerleave", skyLeave)
    on(window, "resize", drawSky)
  }
  void document.fonts.ready.then(() => {
    drawKit()
    drawSky()
  })

  // get started
  const start = one(".start")
  const opts = start ? all(".st-opt", start) : []
  for (const o of opts) {
    on(o, "click", () => {
      start?.setAttribute("data-p", o.getAttribute("data-p") ?? "sdk")
      opts.forEach((x) => x.setAttribute("aria-pressed", x === o ? "true" : "false"))
    })
  }
  for (const c of all(".copy")) {
    on(c, "click", () => {
      const text = c.getAttribute("data-c") ?? ""
      const done = () => {
        c.classList.remove("done")
        void c.offsetWidth
        c.classList.add("done")
        c.textContent = "Copied"
        later(() => {
          c.classList.remove("done")
          c.textContent = "Copy"
        }, 1600)
      }
      void navigator.clipboard?.writeText(text).then(done, () => {})
    })
  }

  return () => {
    for (const f of cleanups) f()
    root.classList.remove("rv")
  }
}
