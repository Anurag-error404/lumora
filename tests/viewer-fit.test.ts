import { expect, test } from "bun:test";

const css = await Bun.file(new URL("../src/styles/app.css", import.meta.url)).text();

function rule(selector: string): string {
  const body = css.match(new RegExp(`\\${selector} \\{([^}]*)\\}`))?.[1];
  if (!body) throw new Error(`missing rule ${selector}`);
  return body;
}

/** `max-height: 100%` on the viewer media only resolves if its parent has a
 *  definite height; with `max-height` there instead, tall photos render at
 *  intrinsic size and get clipped by the stage, looking like `object-fit: cover`. */
test("viewer zoom layer has a definite height", () => {
  const body = rule(".viewer-zoom-layer");
  expect(body).toContain("height: 100%;");
  expect(body).not.toContain("max-height");
});
