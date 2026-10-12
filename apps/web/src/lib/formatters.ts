import { CurrencyFormatOptions, NumberFormatOptions, DateFormatOptions } from "../../types/format";
import { TenancyContract } from "../../types/contract";
import { CallSummaryRecord } from "../../types/log";

/**
 * Safely format numbers with comma separation and optional fraction control.
 * Defensively handles null, undefined, NaN, and unexpected types.
 */
export function formatNumber(
  value: number | string | null | undefined,
  options: NumberFormatOptions = {}
): string {
  const { fallback = "0", minimumFractionDigits = 0, maximumFractionDigits = 0 } = options;

  if (value === null || value === undefined || value === "") {
    return fallback;
  }

  const numericValue = typeof value === "number" ? value : Number(value);
  if (Number.isNaN(numericValue)) {
    return fallback;
  }

  try {
    return numericValue.toLocaleString("en-US", {
      minimumFractionDigits,
      maximumFractionDigits,
    });
  } catch {
    return fallback;
  }
}

/**
 * Safely format monetary amounts with currency symbol and robust fallback.
 * Guaranteed never to throw TypeError: Cannot read properties of undefined.
 */
export function formatCurrency(
  value: number | string | null | undefined,
  options: CurrencyFormatOptions = {}
): string {
  const {
    currency = "AED",
    fallback = "0",
    showCurrency = true,
    minimumFractionDigits = 0,
    maximumFractionDigits = 0,
  } = options;

  const formattedNum = formatNumber(value, {
    fallback,
    minimumFractionDigits,
    maximumFractionDigits,
  });

  if (!showCurrency) {
    return formattedNum;
  }

  if (currency === "USD" || currency === "$") {
    return `$${formattedNum}`;
  }

  return `${currency} ${formattedNum}`;
}

/**
 * Safely format date and time strings.
 */
export function formatDate(
  value: string | number | Date | null | undefined,
  options: DateFormatOptions = {}
): string {
  const { fallback = "—", includeTime = false, locale = "en-US" } = options;

  if (!value) {
    return fallback;
  }

  try {
    const dateObj = value instanceof Date ? value : new Date(value);
    if (Number.isNaN(dateObj.getTime())) {
      return fallback;
    }

    if (includeTime) {
      return dateObj.toLocaleString(locale, {
        dateStyle: "medium",
        timeStyle: "short",
      });
    }

    return dateObj.toLocaleDateString(locale, {
      dateStyle: "medium",
    });
  } catch {
    return fallback;
  }
}

/**
 * Normalizes rent from a TenancyContract supporting both annual_rent_aed and annual_rent.
 */
export function getContractRent(contract: Partial<TenancyContract> | null | undefined): number {
  if (!contract) return 0;
  return contract.annual_rent_aed ?? contract.annual_rent ?? 0;
}

/**
 * Normalizes security deposit from a TenancyContract.
 */
export function getContractSecurityDeposit(contract: Partial<TenancyContract> | null | undefined): number {
  if (!contract) return 0;
  return contract.security_deposit_aed ?? contract.security_deposit ?? 0;
}

/**
 * Normalizes budget from a CallSummaryRecord supporting both qualified_budget_aed and qualified_budget.
 */
export function getCallBudget(summary: Partial<CallSummaryRecord> | null | undefined): number {
  if (!summary) return 0;
  return summary.qualified_budget_aed ?? summary.qualified_budget ?? 0;
}

/**
 * Formats a duration in seconds into human-readable minutes and seconds (e.g. 2m 45s).
 */
export function formatDuration(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || seconds <= 0) {
    return "0s";
  }
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (mins === 0) {
    return `${secs}s`;
  }
  return `${mins}m ${secs.toString().padStart(2, "0")}s`;
}

/**
 * Formats an ISO timestamp or date into exact time with seconds (e.g. 08:15:35 AM).
 */
export function formatTimeOnly(value: string | number | Date | null | undefined): string {
  if (!value) return "—";
  try {
    const d = value instanceof Date ? value : new Date(value);
    if (Number.isNaN(d.getTime())) return "—";
    return d.toLocaleTimeString("en-US", {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return "—";
  }
}

/**
 * Formats a call timestamp into clean human-readable timing (e.g. Oct 12, 2026 • 08:15 AM).
 */
export function formatCallTiming(value: string | number | Date | null | undefined): string {
  if (!value) return "—";
  try {
    const d = value instanceof Date ? value : new Date(value);
    if (Number.isNaN(d.getTime())) return "—";
    const dateStr = d.toLocaleDateString("en-US", { dateStyle: "medium" });
    const timeStr = d.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit" });
    return `${dateStr} • ${timeStr}`;
  } catch {
    return "—";
  }
}
