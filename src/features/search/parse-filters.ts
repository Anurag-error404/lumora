export type ParsedFilters = {
  text: string;
  camera?: string;
  minRating?: number;
  before?: string;
  after?: string;
  mediaType?: "image" | "video";
  favoriteOnly?: boolean;
};

/** Mirror of Rust filter tokens for UI hints / client-side validation. */
export function parseFilters(raw: string): ParsedFilters {
  const parts = raw.trim().split(/\s+/).filter(Boolean);
  const out: ParsedFilters = { text: "" };
  const textParts: string[] = [];

  for (const token of parts) {
    if (token.startsWith("camera:")) {
      out.camera = token.slice("camera:".length);
    } else if (token.startsWith("rating>=")) {
      out.minRating = Number(token.slice("rating>=".length));
    } else if (token.startsWith("rating>")) {
      out.minRating = Number(token.slice("rating>".length));
    } else if (token.startsWith("before:")) {
      out.before = token.slice("before:".length);
    } else if (token.startsWith("after:")) {
      out.after = token.slice("after:".length);
    } else if (token.startsWith("type:")) {
      const v = token.slice("type:".length).toLowerCase();
      if (v === "image" || v === "video") out.mediaType = v;
    } else if (/^fav(orite)?:true$/i.test(token)) {
      out.favoriteOnly = true;
    } else {
      textParts.push(token);
    }
  }

  out.text = textParts.join(" ");
  return out;
}
