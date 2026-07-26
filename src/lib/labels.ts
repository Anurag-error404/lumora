import { LABEL_COLORS } from "./constants";

export function labelHex(label: string | null | undefined): string | null {
  if (!label) return null;
  return LABEL_COLORS.find((c) => c.id === label)?.hex ?? "#9aa0a6";
}

export function labelName(label: string): string {
  return label.charAt(0).toUpperCase() + label.slice(1);
}

export function trashDaysLeft(
  deletedAt: string | null | undefined,
  retentionDays: number,
): number | null {
  if (!deletedAt) return null;
  const deleted = Date.parse(deletedAt);
  if (Number.isNaN(deleted)) return null;
  const elapsedDays = (Date.now() - deleted) / 86_400_000;
  return Math.max(0, Math.ceil(retentionDays - elapsedDays));
}
