import { Link } from "react-router"
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Button } from "@/components/ui/button"
import { PageHeader } from "@/components/common"

export function NotFound() {
  return (
    <>
    <PageHeader title="No page at this address" />
    <Empty>
      <EmptyHeader>
        <EmptyTitle>Nothing to show here</EmptyTitle>
        <EmptyDescription>This build of the site has no page, CAP or corpus entry at this address.</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button asChild>
          <Link to="/dashboard">Open the dashboard start page</Link>
        </Button>
      </EmptyContent>
    </Empty>
    </>
  )
}
