import type { NoticeTone } from '../sources';

export interface NoticeProps {
  tone: NoticeTone;
  message: string;
  /** Smaller text under the message. */
  detail?: string | null;
  /** Shows a dismiss button when given. */
  onDismiss?: () => void;
}

/** A one-line banner inside the panel. Errors are announced at once; the rest politely. */
export function Notice({ tone, message, detail, onDismiss }: NoticeProps) {
  return (
    <div
      className="notice"
      data-ref="notice"
      data-tone={tone}
      role={tone === 'error' ? 'alert' : 'status'}
    >
      <div className="notice-text">
        <div data-ref="noticeMessage">{message}</div>
        {detail && <div className="notice-detail">{detail}</div>}
      </div>
      {onDismiss && (
        <button
          className="icon notice-dismiss"
          title="Dismiss"
          aria-label="Dismiss"
          onClick={onDismiss}
        >
          ×
        </button>
      )}
    </div>
  );
}
