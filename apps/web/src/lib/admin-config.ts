/** Admin list settings sourced from public env vars, with sane fallbacks. */
function parseSizes(raw: string | undefined): number[] {
  const sizes = (raw ?? "")
    .split(",")
    .map((value) => Number(value.trim()))
    .filter((value) => Number.isInteger(value) && value > 0);
  return sizes.length ? sizes : [10, 25, 50];
}

export const adminPageSizeOptions = parseSizes(process.env.NEXT_PUBLIC_ADMIN_PAGE_SIZE_OPTIONS);
export const adminDefaultPageSize = adminPageSizeOptions[0];
export const adminSearchDebounceMs = Number(process.env.NEXT_PUBLIC_ADMIN_SEARCH_DEBOUNCE_MS) || 350;
