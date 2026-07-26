import { describe, expect, test } from "bun:test";
import { parseFilters } from "../src/features/search/parse-filters";

describe("parseFilters", () => {
  test("parses tokens", () => {
    const q = parseFilters(
      "beach camera:iphone rating>3 before:2024-01-01 type:image fav:true",
    );
    expect(q.text).toBe("beach");
    expect(q.camera).toBe("iphone");
    expect(q.minRating).toBe(3);
    expect(q.before).toBe("2024-01-01");
    expect(q.mediaType).toBe("image");
    expect(q.favoriteOnly).toBe(true);
  });

  test("empty is browse", () => {
    expect(parseFilters("").text).toBe("");
    expect(parseFilters("   ").text).toBe("");
  });
});
