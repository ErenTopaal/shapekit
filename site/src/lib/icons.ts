import { BookOpen, Boxes, Download, FileText, Layers, ListChecks, Repeat, ScanSearch, ShieldCheck, Terminal, Wrench, type LucideIcon } from "lucide-react"

// the icons the content script names (scripts/docs-content.ts), imported one by one so the
// browser bundle carries these and not lucide's whole set
export const icons: Record<string, LucideIcon> = { BookOpen, Boxes, Download, FileText, Layers, ListChecks, Repeat, ScanSearch, ShieldCheck, Terminal, Wrench }
