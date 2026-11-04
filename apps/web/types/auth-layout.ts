export interface AuthHighlight {
  title: string;
  description: string;
}

export interface AuthStat {
  value: string;
  label: string;
}

export interface AuthShowcaseContent {
  eyebrow: string;
  headline: string;
  accentWord: string;
  description: string;
  highlights: AuthHighlight[];
  stats: AuthStat[];
  quote?: {
    text: string;
    author: string;
    role: string;
  };
}
