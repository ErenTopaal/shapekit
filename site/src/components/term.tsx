import { useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from "react"
import { Link } from "react-router"
import { cn } from "@/lib/utils"
import { glossaryHref, termDefinition } from "@/lib/terms"

// a term marks its words with a dotted underline: the reader can see that it has a definition
export const termClass = "underline decoration-dotted decoration-foreground/40 underline-offset-2 hover:decoration-foreground"

// a glossary term on a page. the term links to its glossary anchor and shows its short
// definition in place: on hover, on keyboard focus and on tap. a tap outside or escape closes it
export function Term({ id, children, className }: { id: string; children: ReactNode; className?: string }) {
  const definition = termDefinition(id)
  if (!definition) {
    return (
      <Link to={glossaryHref(id)} className={cn(termClass, className)}>
        {children}
      </Link>
    )
  }
  return (
    <TermTip id={id} definition={definition} className={className}>
      {children}
    </TermTip>
  )
}

// the space between the tip and the trigger, and between the tip and the viewport edge
const gap = 6
const margin = 8

function TermTip({ id, definition, children, className }: { id: string; definition: string; children: ReactNode; className?: string }) {
  const tipId = useId()
  const [open, setOpen] = useState(false)
  const wrap = useRef<HTMLSpanElement>(null)
  const trigger = useRef<HTMLAnchorElement>(null)
  const tip = useRef<HTMLSpanElement>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  // the pointer of the last press on the trigger, and whether the tip was open before it
  const press = useRef<{ type: string; wasOpen: boolean } | null>(null)

  const later = (next: boolean, ms: number) => {
    clearTimeout(timer.current)
    timer.current = setTimeout(() => setOpen(next), ms)
  }
  const now = (next: boolean) => {
    clearTimeout(timer.current)
    setOpen(next)
  }

  // the tip is fixed to the viewport: below the term, above it when there is no room below,
  // and moved left so its right edge stays in the viewport. it stays invisible until it is placed
  useLayoutEffect(() => {
    const t = trigger.current
    const el = tip.current
    if (!open || !t || !el) return
    const place = () => {
      const r = t.getClientRects()[0] ?? t.getBoundingClientRect()
      const vw = document.documentElement.clientWidth
      const vh = document.documentElement.clientHeight
      const left = Math.max(margin, Math.min(r.left, vw - el.offsetWidth - margin))
      const below = r.bottom + gap
      const above = r.top - gap - el.offsetHeight
      const top = below + el.offsetHeight > vh - margin && above >= margin ? above : below
      el.style.left = `${left}px`
      el.style.top = `${top}px`
      el.style.visibility = "visible"
    }
    place()
    window.addEventListener("scroll", place, true)
    window.addEventListener("resize", place)
    return () => {
      window.removeEventListener("scroll", place, true)
      window.removeEventListener("resize", place)
      el.style.visibility = ""
    }
  }, [open])

  // escape and a press outside the term close the tip. the escape listener runs in the capture
  // phase of the window, before the radix dialog layers listen on the document, and prevents the
  // default: an escape that closes a tip does not also close the sheet or dialog around it
  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return
      e.preventDefault()
      if (tip.current?.contains(document.activeElement)) trigger.current?.focus()
      clearTimeout(timer.current)
      setOpen(false)
    }
    const onDown = (e: PointerEvent) => {
      if (wrap.current?.contains(e.target as Node)) return
      clearTimeout(timer.current)
      setOpen(false)
    }
    window.addEventListener("keydown", onKey, true)
    document.addEventListener("pointerdown", onDown)
    return () => {
      window.removeEventListener("keydown", onKey, true)
      document.removeEventListener("pointerdown", onDown)
    }
  }, [open])

  useEffect(() => () => clearTimeout(timer.current), [])

  return (
    <span
      ref={wrap}
      onPointerEnter={(e) => e.pointerType === "mouse" && later(true, 120)}
      onPointerLeave={(e) => e.pointerType === "mouse" && later(false, 150)}
      onFocus={() => now(true)}
      onBlur={(e) => {
        if (!wrap.current?.contains(e.relatedTarget as Node | null)) now(false)
      }}
    >
      <Link
        ref={trigger}
        to={glossaryHref(id)}
        className={cn(termClass, className)}
        aria-describedby={`${tipId}-d`}
        onPointerDown={(e) => {
          press.current = { type: e.pointerType, wasOpen: open }
        }}
        onClick={(e) => {
          // a tap shows the definition; the glossary link in the tip goes to the glossary
          const p = press.current
          press.current = null
          if (p && p.type !== "mouse") {
            e.preventDefault()
            now(!p.wasOpen)
          }
        }}
      >
        {children}
      </Link>
      <span
        ref={tip}
        id={tipId}
        hidden={!open}
        className="invisible fixed top-0 left-0 z-50 w-max max-w-[min(20rem,calc(100vw-1rem))] rounded-md border bg-popover px-3 py-2 text-left text-xs leading-relaxed font-normal tracking-normal whitespace-normal text-popover-foreground normal-case shadow-md"
      >
        <span id={`${tipId}-d`} className="block">
          {definition}
        </span>
        <Link
          to={glossaryHref(id)}
          className="mt-1 inline-block text-primary underline decoration-primary/40 hover:decoration-primary"
        >
          Glossary
        </Link>
      </span>
    </span>
  )
}
