import { describe, expect, test } from "bun:test";

const css = await Bun.file(new URL("../src/styles/app.css", import.meta.url)).text();

function rule(selector: string): string {
  const body = css.match(new RegExp(`\\${selector} \\{([^}]*)\\}`))?.[1];
  if (!body) throw new Error(`missing rule ${selector}`);
  return body;
}

/** Buttons default to `align-items: center`, which sizes a card's text block to
 *  its own content and lets long titles spill over the neighbouring card. */
describe("cover card buttons stretch their contents", () => {
  for (const selector of [
    ".memory-cover-open",
    ".person-cover-open",
    ".album-cover-open",
    ".place-cover-open",
    ".home-memory-card",
  ]) {
    test(selector, () => {
      expect(rule(selector)).toContain("align-items: stretch;");
    });
  }
});
