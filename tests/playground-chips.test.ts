import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";

const html = await Bun.file(new URL("../index.html", import.meta.url)).text();
const js = await Bun.file(new URL("../site.js", import.meta.url)).text();

const chipQueries = [...html.matchAll(/class="play-chip[^"]*"\s+data-query="([^"]+)"/g)].map(
  (m) => m[1]
);
const shots = new Map(
  [...js.matchAll(/\["([^"]+)",\s*"(docs\/screenshots\/[^"]+)"\]/g)].map((m) => [m[1], m[2]])
);

describe("search playground chips", () => {
  test("every chip maps to a screenshot that exists", () => {
    expect(chipQueries.length).toBeGreaterThan(0);
    for (const q of chipQueries) {
      const src = shots.get(q);
      expect(src, `no screenshot mapped for chip "${q}"`).toBeDefined();
      expect(existsSync(new URL(`../${src}`, import.meta.url)), `missing ${src}`).toBe(true);
    }
  });

  test("no mapped screenshot is orphaned", () => {
    expect([...shots.keys()].filter((q) => !chipQueries.includes(q))).toEqual([]);
  });
});
