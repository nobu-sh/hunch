import { useRef, type KeyboardEvent } from 'react';
import { movedIndex } from './roving-focus';

export interface TabItem<T extends string> {
  id: T;
  label: string;
}

export interface TabsProps<T extends string> {
  items: readonly TabItem<T>[];
  selected: T;
  onSelect: (id: T) => void;
  /** Names the tab bar for screen readers. */
  label: string;
  /** Prefix for the tab and panel element ids; see `tabPanelProps`. */
  idPrefix: string;
}

function tabElementId(params: { idPrefix: string; id: string }): string {
  return `${params.idPrefix}-tab-${params.id}`;
}

function panelElementId(params: { idPrefix: string; id: string }): string {
  return `${params.idPrefix}-panel-${params.id}`;
}

/** Spread on the element that shows the selected tab's content. */
export function tabPanelProps(params: { idPrefix: string; id: string }) {
  return {
    role: 'tabpanel',
    id: panelElementId(params),
    'aria-labelledby': tabElementId(params),
  } as const;
}

/** A tab bar. Arrow keys, Home and End move between tabs and select them. */
export function Tabs<T extends string>({
  items,
  selected,
  onSelect,
  label,
  idPrefix,
}: TabsProps<T>) {
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = items.findIndex((item) => item.id === selected);
    const next = movedIndex({
      key: event.key,
      index,
      count: items.length,
      orientation: 'horizontal',
    });
    const item = next === null ? undefined : items[next];
    if (next === null || item === undefined) return;
    event.preventDefault();
    onSelect(item.id);
    tabRefs.current[next]?.focus();
  };

  return (
    <div className="tabs" role="tablist" aria-label={label} onKeyDown={onKeyDown}>
      {items.map((item, index) => {
        const isSelected = item.id === selected;
        return (
          <button
            key={item.id}
            ref={(element) => {
              tabRefs.current[index] = element;
            }}
            className="tab"
            role="tab"
            id={tabElementId({ idPrefix, id: item.id })}
            aria-selected={isSelected}
            aria-controls={panelElementId({ idPrefix, id: item.id })}
            tabIndex={isSelected ? 0 : -1}
            onClick={() => {
              onSelect(item.id);
            }}
          >
            {item.label}
          </button>
        );
      })}
    </div>
  );
}
