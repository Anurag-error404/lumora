import type { ReactNode } from "react";

/** Shared page title + short description used across LUMORA views. */
export function PageHeader({
  title,
  description,
  actions,
  className = "",
}: {
  title: string;
  description: string;
  actions?: ReactNode;
  className?: string;
}) {
  return (
    <header className={`page-header ${className}`.trim()}>
      <div className="page-header-copy">
        <h2>{title}</h2>
        <p className="muted">{description}</p>
      </div>
      {actions ? <div className="page-header-actions">{actions}</div> : null}
    </header>
  );
}
