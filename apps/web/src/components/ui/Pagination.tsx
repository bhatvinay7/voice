import React from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { PaginationMeta } from "../../../types/log";

interface PaginationProps {
  meta: PaginationMeta;
  onPageChange: (page: number) => void;
  pageSizeOptions: number[];
  onPageSizeChange: (size: number) => void;
}

/** Compact page list: first, last, current ±1, with ellipses between gaps. */
function buildPages(current: number, total: number): Array<number | "gap"> {
  const wanted = new Set([1, total, current - 1, current, current + 1]);
  const pages = [...wanted].filter((p) => p >= 1 && p <= total).sort((a, b) => a - b);
  return pages.flatMap((page, index) => {
    const previous = pages[index - 1];
    return previous !== undefined && page - previous > 1 ? ["gap" as const, page] : [page];
  });
}

export function Pagination({ meta, onPageChange, pageSizeOptions, onPageSizeChange }: PaginationProps): React.ReactElement {
  const { page, total_pages: totalPages, total, page_size: pageSize } = meta;
  const first = total === 0 ? 0 : (page - 1) * pageSize + 1;
  const last = Math.min(page * pageSize, total);

  return (
    <nav aria-label="Pagination" className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div className="flex items-center gap-3 text-xs text-theme-sub">
        <span>
          Showing <span className="font-semibold text-theme-text">{first}–{last}</span> of{" "}
          <span className="font-semibold text-theme-text">{total}</span>
        </span>
        <label className="flex items-center gap-1.5">
          <span className="sr-only">Rows per page</span>
          <select
            id="pagination-page-size"
            value={pageSize}
            onChange={(event) => onPageSizeChange(Number(event.target.value))}
            className="rounded-lg border border-theme-border bg-theme-card px-2 py-1 text-xs text-theme-text outline-none focus:border-cyan-400/60"
          >
            {pageSizeOptions.map((size) => (
              <option key={size} value={size}>{size} / page</option>
            ))}
          </select>
        </label>
      </div>

      <div className="flex items-center gap-1">
        <button
          id="pagination-prev"
          type="button"
          onClick={() => onPageChange(page - 1)}
          disabled={page <= 1}
          aria-label="Previous page"
          className="page-btn"
        >
          <ChevronLeft className="h-4 w-4" />
        </button>
        {buildPages(page, totalPages).map((item, index) =>
          item === "gap" ? (
            <span key={`gap-${index}`} className="px-1.5 text-xs text-theme-sub">…</span>
          ) : (
            <button
              key={item}
              type="button"
              onClick={() => onPageChange(item)}
              aria-current={item === page ? "page" : undefined}
              className={item === page ? "page-btn page-btn-active" : "page-btn"}
            >
              {item}
            </button>
          ),
        )}
        <button
          id="pagination-next"
          type="button"
          onClick={() => onPageChange(page + 1)}
          disabled={page >= totalPages}
          aria-label="Next page"
          className="page-btn"
        >
          <ChevronRight className="h-4 w-4" />
        </button>
      </div>
    </nav>
  );
}
