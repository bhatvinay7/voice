const MESSAGE_FIELDS = ["text", "response_text", "message", "error", "status"];

function readableValue(value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  if (Array.isArray(value)) return value.map(readableValue).filter(Boolean).join(", ");
  if (typeof value === "object") {
    return Object.entries(value as Record<string, unknown>)
      .map(([key, entry]) => `${humanizeKey(key)}: ${readableValue(entry)}`)
      .join(" · ");
  }
  return "—";
}

function humanizeKey(key: string): string {
  return key
    .replaceAll("_", " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function parsePayload(payload: string): unknown {
  try {
    return JSON.parse(payload) as unknown;
  } catch {
    return payload;
  }
}

export function formatEventPayload(payload: string): string {
  const value = parsePayload(payload);
  if (value && typeof value === "object" && !Array.isArray(value)) {
    const entries = Object.entries(value as Record<string, unknown>);
    const message = MESSAGE_FIELDS
      .map((field) => (typeof (value as Record<string, unknown>)[field] === "string"
        ? (value as Record<string, unknown>)[field] as string
        : ""))
      .filter(Boolean);
    const extraEntries = entries.filter(([key]) => !MESSAGE_FIELDS.includes(key));
    const details = extraEntries
      .map(([key, entry]) => `${humanizeKey(key)}: ${readableValue(entry)}`)
      .join(" · ");
    return [message.join(" · "), details].filter(Boolean).join(" · ") || "Event recorded";
  }
  return readableValue(value);
}

export function getConversationText(payload: string): string {
  const value = parsePayload(payload);
  if (value && typeof value === "object" && !Array.isArray(value)) {
    const record = value as Record<string, unknown>;
    for (const field of ["text", "response_text", "message"]) {
      if (typeof record[field] === "string" && record[field]) return record[field] as string;
    }
  }
  return formatEventPayload(payload);
}
