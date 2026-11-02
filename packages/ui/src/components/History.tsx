import type { HistoryRowView } from '../overlay-view';
import { CopyButton } from './CopyButton';

export interface HistoryProps {
  /** Newest first. */
  rows: HistoryRowView[];
  onCopy: (text: string) => Promise<void>;
}

export function History({ rows, onCopy }: HistoryProps) {
  return (
    <div className="history" data-ref="history" hidden={rows.length === 0}>
      <div className="label history-title">History</div>
      <ol data-ref="historyList">
        {rows.map((row, index) => (
          // New rows go on top, so counting from the end keeps each row's key and copy feedback.
          <li key={rows.length - index}>
            <span className="time">{row.time}</span>
            <span className="key-cell">{row.key}</span>
            <span className="bpm-cell">{row.bpm}</span>
            <CopyButton text={row.copyText} title={row.copyText} onCopy={onCopy} />
          </li>
        ))}
      </ol>
    </div>
  );
}
