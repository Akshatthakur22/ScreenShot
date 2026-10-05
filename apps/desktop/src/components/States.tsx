type StateProps = { message: string; action?: () => void; actionLabel?: string };

export function LoadingState({ message = "Loading screenshots…" }: { message?: string }) {
  return <div className="state loading-state" role="status"><span className="spinner" />{message}</div>;
}

export function EmptyState({ message, action, actionLabel }: StateProps) {
  return <div className="state empty-state"><span className="empty-mark" aria-hidden="true">▧</span><p>{message}</p>{action && actionLabel && <button className="text-button" onClick={action}>{actionLabel}</button>}</div>;
}

export function ErrorState({ message, action }: Pick<StateProps, "message" | "action">) {
  return <div className="state error-state" role="alert"><p>{message}</p>{action && <button onClick={action}>Try again</button>}</div>;
}
