import { ICON_PATHS, type IconName } from "./paths";

export function Icon({
  name,
  className,
}: {
  name: IconName;
  className?: string;
}) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      aria-hidden
      focusable="false"
    >
      <path d={ICON_PATHS[name]} />
    </svg>
  );
}
