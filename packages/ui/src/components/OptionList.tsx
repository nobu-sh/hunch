import { useRef, useState, type KeyboardEvent } from 'react';
import { movedIndex } from './roving-focus';

export interface ListOption {
  id: string;
  label: string;
  /** A second, quieter line under the label. */
  detail?: string;
  /** Short tags after the label, such as `default`. */
  badges?: readonly string[];
}

export interface OptionListProps {
  options: readonly ListOption[];
  /** The option marked as chosen, if it's in the list. */
  selectedId: string | null;
  onSelect: (id: string) => void;
  /** Names the list for screen readers. */
  label: string;
  /** Shown in place of the list when there are no options. */
  emptyText: string;
  /** An option whose selection is under way; the list ignores picks until it settles. */
  pendingId?: string | null;
  pendingText?: string;
}

const SELECT_KEYS = new Set(['Enter', ' ']);

/** A single-choice list. Arrow keys, Home and End move between options; Enter or Space picks. */
export function OptionList({
  options,
  selectedId,
  onSelect,
  label,
  emptyText,
  pendingId = null,
  pendingText = 'switching…',
}: OptionListProps) {
  const selectedIndex = options.findIndex((option) => option.id === selectedId);
  const [focusedIndex, setFocusedIndex] = useState(Math.max(selectedIndex, 0));
  const optionRefs = useRef<(HTMLLIElement | null)[]>([]);
  const isPending = pendingId !== null;
  // The options can change under a remembered index.
  const tabStop = Math.min(focusedIndex, options.length - 1);

  if (options.length === 0) {
    return (
      <div className="option-empty" data-ref="optionEmpty">
        {emptyText}
      </div>
    );
  }

  const pick = (id: string) => {
    if (!isPending) onSelect(id);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const option = options[tabStop];
    if (SELECT_KEYS.has(event.key) && option !== undefined) {
      event.preventDefault();
      pick(option.id);
      return;
    }
    const next = movedIndex({
      key: event.key,
      index: tabStop,
      count: options.length,
      orientation: 'vertical',
    });
    if (next === null) return;
    event.preventDefault();
    setFocusedIndex(next);
    optionRefs.current[next]?.focus();
  };

  return (
    <ul
      className="options"
      role="listbox"
      aria-label={label}
      aria-busy={isPending}
      onKeyDown={onKeyDown}
    >
      {options.map((option, index) => {
        const isOptionPending = option.id === pendingId;
        const badges = isOptionPending ? [pendingText] : (option.badges ?? []);
        return (
          <li
            key={option.id}
            ref={(element) => {
              optionRefs.current[index] = element;
            }}
            className="option"
            role="option"
            aria-selected={option.id === selectedId}
            aria-disabled={isPending}
            data-pending={String(isOptionPending)}
            tabIndex={index === tabStop ? 0 : -1}
            onFocus={() => {
              setFocusedIndex(index);
            }}
            onClick={() => {
              pick(option.id);
            }}
          >
            <span className="option-mark" aria-hidden="true" />
            <span className="option-text">
              <span className="option-label">{option.label}</span>
              {option.detail && <span className="option-detail">{option.detail}</span>}
            </span>
            {badges.map((badge) => (
              <span key={badge} className="badge">
                {badge}
              </span>
            ))}
          </li>
        );
      })}
    </ul>
  );
}
