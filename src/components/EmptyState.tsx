import { Icon, type IconName } from "./icons";

export function EmptyState({
  icon,
  title,
  description,
  action,
  secondaryAction,
}: {
  icon: IconName;
  title: string;
  description: string;
  action?: { label: string; onClick: () => void };
  secondaryAction?: { label: string; onClick: () => void };
}) {
  return (
    <section className="empty-state" role="status" aria-live="polite">
      <div className="empty-state-visual" aria-hidden="true">
        <span className="empty-state-frame frame-back" />
        <span className="empty-state-frame frame-middle" />
        <span className="empty-state-frame frame-front">
          <Icon name={icon} />
        </span>
      </div>
      <div className="empty-state-copy">
        <h2>{title}</h2>
        <p>{description}</p>
      </div>
      {(action || secondaryAction) && (
        <div className="empty-state-actions">
          {action && (
            <button className="primary" type="button" onClick={action.onClick}>
              {action.label}
            </button>
          )}
          {secondaryAction && (
            <button type="button" onClick={secondaryAction.onClick}>
              {secondaryAction.label}
            </button>
          )}
        </div>
      )}
    </section>
  );
}
