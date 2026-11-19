import { glossary } from "@/lib/terms"
import { Id, PageHeader, useHashScroll } from "@/components/common"

// the one glossary of the dashboard: every page links its terms to an anchor here
export function GlossaryPage() {
  useHashScroll()
  return (
    <div className="flex flex-col">
      <PageHeader title="Glossary">
        <p>
          This page defines the terms of the dashboard. Each term has one meaning on every page. When the repository or the
          command line uses another word, the definition gives that word.
        </p>
      </PageHeader>
      <nav aria-label="Groups of terms" className="pb-2 text-sm">
        <ul className="flex flex-wrap gap-x-4 gap-y-1">
          {glossary.map((g) => (
            <li key={g.id}>
              <a href={`#${g.id}`} className="text-primary underline decoration-primary/40 hover:decoration-primary">
                {g.title}
              </a>
            </li>
          ))}
        </ul>
      </nav>
      {glossary.map((g) => (
        <section key={g.id} id={g.id} aria-labelledby={`${g.id}-h`} className="flex scroll-mt-20 flex-col gap-2 py-5">
          <h2 id={`${g.id}-h`} className="text-lg font-semibold tracking-tight">
            {g.title}
          </h2>
          {g.note ? <p className="text-sm text-muted-foreground">{g.note}</p> : null}
          <dl className="flex flex-col divide-y border-y">
            {g.terms.map((t) => (
              <div key={t.id} id={t.id} className="grid scroll-mt-20 gap-x-6 gap-y-1 py-2.5 text-sm target:bg-accent/40 sm:grid-cols-[14rem_minmax(0,1fr)]">
                <dt className="font-medium">
                  {t.name}
                  {t.repo ? (
                    <span className="block text-xs font-normal text-muted-foreground">
                      in the repository: <Id>{t.repo}</Id>
                    </span>
                  ) : null}
                </dt>
                <dd className="text-muted-foreground">{t.meaning}</dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
    </div>
  )
}
