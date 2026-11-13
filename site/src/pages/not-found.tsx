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
        <EmptyDescription>The link may name a CAP or page that is not in this build of the data.</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button asChild>
          <Link to="/dashboard">Open the overview</Link>
        </Button>
      </EmptyContent>
    </Empty>
    </>
  )
}
