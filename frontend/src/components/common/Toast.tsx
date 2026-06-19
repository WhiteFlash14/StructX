"use client";

type Props = {
  message: string | null;
};

export function Toast({ message }: Props) {
  if (!message) return null;
  return (
    <div className="toast" role="status" aria-live="polite">
      <span className="toast-dot" aria-hidden />
      {message}
    </div>
  );
}
