export type ViewBox = { x: number; y: number; w: number; h: number };

/** SVG space: x = lon, y = -lat. */
export const WORLD: ViewBox = { x: -180, y: -90, w: 360, h: 180 };
const MIN_W = 2;
const MIN_H = 1;

export function clampViewBox(v: ViewBox): ViewBox {
  const w = Math.min(Math.max(v.w, MIN_W), WORLD.w);
  const h = Math.min(Math.max(v.h, MIN_H), WORLD.h);
  return {
    x: Math.min(Math.max(v.x, WORLD.x), WORLD.x + WORLD.w - w),
    y: Math.min(Math.max(v.y, WORLD.y), WORLD.y + WORLD.h - h),
    w,
    h,
  };
}

export function panViewBox(v: ViewBox, dx: number, dy: number): ViewBox {
  return clampViewBox({ ...v, x: v.x + dx, y: v.y + dy });
}

/** `factor` < 1 zooms in. `ax`/`ay` are the SVG-space anchor. */
export function zoomViewBox(
  v: ViewBox,
  factor: number,
  ax: number,
  ay: number,
): ViewBox {
  return clampViewBox({
    x: ax - (ax - v.x) * factor,
    y: ay - (ay - v.y) * factor,
    w: v.w * factor,
    h: v.h * factor,
  });
}

export function viewBoxCenter(v: ViewBox) {
  return { x: v.x + v.w / 2, y: v.y + v.h / 2 };
}
