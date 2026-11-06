import type { AuthShowcaseContent } from "../../types/auth-layout";

export const clientAuthShowcase: AuthShowcaseContent = {
  eyebrow: "AI Voice for Real Estate",
  headline: "Find your next home just by talking",
  accentWord: "talking",
  description:
    "Describe what you are looking for in your own words. DWIN searches verified UAE listings, compares options and books viewings for you.",
  highlights: [
    { title: "Natural conversations", description: "Ask about price, area, amenities or payment plans like you would an agent." },
    { title: "Verified listings", description: "Every property is checked and kept up to date by our partner network." },
    { title: "Secure by design", description: "Your identity and documents are protected end to end." },
  ],
  stats: [
    { value: "24/7", label: "Availability" },
    { value: "<1s", label: "Reply time" },
    { value: "10k+", label: "Listings" },
  ],
  quote: {
    text: "I found a two-bed in Dubai Marina in one evening without filling out a single form.",
    author: "Aisha R.",
    role: "Homebuyer, Dubai",
  },
};

export const adminAuthShowcase: AuthShowcaseContent = {
  eyebrow: "Operations Console",
  headline: "Run your property desk with clarity",
  accentWord: "clarity",
  description:
    "Review conversations, verify customer documents, manage listings and keep every journey on track from one place.",
  highlights: [
    { title: "Customer verification", description: "Approve or reject identity documents with a full audit trail." },
    { title: "Conversation insights", description: "Read summaries and outcomes of every customer call." },
    { title: "Listing management", description: "Upload and refresh inventory in a few clicks." },
  ],
  stats: [
    { value: "Live", label: "Activity" },
    { value: "100%", label: "Audited" },
    { value: "Role", label: "Based access" },
  ],
};
