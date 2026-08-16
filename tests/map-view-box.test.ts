import { describe, expect, test } from "bun:test";
import {
  clampViewBox,
  panViewBox,
  zoomViewBox,
} from "../src/features/places/mapViewBox";

describe("map viewBox", () => {
  test("pan past the world edge clamps", () => {
    const v = { x: -180, y: -90, w: 360, h: 180 };
    expect(panViewBox(v, -40, -20)).toEqual(v);
  });

  test("zoom around a point keeps the anchor stable", () => {
    const v = { x: 0, y: 0, w: 40, h: 20 };
    const next = zoomViewBox(v, 0.5, 10, 5);
    expect(next).toEqual({ x: 5, y: 2.5, w: 20, h: 10 });
  });

  test("zoom out from the world stays the world", () => {
    const world = { x: -180, y: -90, w: 360, h: 180 };
    expect(zoomViewBox(world, 2, 0, 0)).toEqual(world);
  });

  test("clamp rejects inverted or oversized boxes", () => {
    expect(clampViewBox({ x: -400, y: -200, w: 800, h: 400 })).toEqual({
      x: -180,
      y: -90,
      w: 360,
      h: 180,
    });
  });
});
