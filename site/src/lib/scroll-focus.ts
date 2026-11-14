import { useEffect, useRef, useState } from "react"

// a wide table scrolls sideways inside its frame; while it does, the frame takes keyboard focus
// so it can be scrolled without a pointer
export function useScrollFocus<T extends HTMLElement>() {
  const ref = useRef<T>(null)
  const [scrolls, setScrolls] = useState(false)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const check = () => setScrolls(el.scrollWidth > el.clientWidth + 1)
    check()
    const ro = new ResizeObserver(check)
    ro.observe(el)
    if (el.firstElementChild) ro.observe(el.firstElementChild)
    return () => ro.disconnect()
  }, [])
  return { ref, scrolls }
}

