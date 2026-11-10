import { useState } from "react";
import { Button } from "@skill-studio/ui";

const PAGE_SIZE = 20;

export function useLabPage<T>(items: T[]) {
  const [requestedPage, setRequestedPage] = useState(0);
  const pageCount = Math.max(1, Math.ceil(items.length / PAGE_SIZE));
  const page = Math.min(requestedPage, pageCount - 1);
  return {
    items: items.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE),
    page,
    pageCount,
    setPage: setRequestedPage,
  };
}

export function LabPagination({
  page,
  pageCount,
  setPage,
  label,
}: {
  page: number;
  pageCount: number;
  setPage: (page: number) => void;
  label: string;
}) {
  if (pageCount <= 1) return null;
  return (
    <nav aria-label={label} className="flex flex-wrap items-center gap-2">
      <Button variant="outline" disabled={page === 0} onClick={() => setPage(page - 1)}>
        Previous
      </Button>
      <span className="text-body tabular-nums">
        Page {page + 1} of {pageCount}
      </span>
      <Button variant="outline" disabled={page + 1 === pageCount} onClick={() => setPage(page + 1)}>
        Next
      </Button>
    </nav>
  );
}
