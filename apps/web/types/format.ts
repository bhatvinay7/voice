export type CurrencyCode = "AED" | "USD" | "EUR" | "GBP";

export interface CurrencyFormatOptions {
  currency?: CurrencyCode | string;
  fallback?: string;
  showCurrency?: boolean;
  minimumFractionDigits?: number;
  maximumFractionDigits?: number;
}

export interface NumberFormatOptions {
  fallback?: string;
  minimumFractionDigits?: number;
  maximumFractionDigits?: number;
}

export interface DateFormatOptions {
  fallback?: string;
  includeTime?: boolean;
  locale?: string;
}
