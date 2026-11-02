import { useId, useState, type KeyboardEvent } from 'react';
import { sourceBadges, type AudioSource, type SourceKind, type SourceList } from '../sources';
import { Notice } from './Notice';
import { OptionList, type ListOption } from './OptionList';
import { tabPanelProps, Tabs, type TabItem } from './Tabs';

export interface SourcePickerProps {
  /** Null while the list loads. */
  sources: SourceList | null;
  /** The source being listened to, marked in the list. */
  currentId: string | null;
  /** The source being switched to. */
  pendingId: string | null;
  /** Why loading or switching failed. */
  error: string | null;
  onSelect: (id: string) => void;
  /** Returns to the readouts. Escape does the same. */
  onBack: () => void;
  /** The tab to open on; by default the current source's kind. */
  initialTab?: SourceKind;
}

const APPS_TAB: TabItem<SourceKind> = { id: 'app', label: 'Apps' };
const DEVICES_TAB: TabItem<SourceKind> = { id: 'device', label: 'Devices' };

const EMPTY_TEXT: Record<SourceKind, string> = {
  app: 'No apps are playing sound.',
  device: 'No audio devices found.',
};

function sourceOption(source: AudioSource): ListOption {
  return { id: source.id, label: source.name, badges: sourceBadges(source) };
}

function openingTab(params: { sources: SourceList | null; currentId: string | null }): SourceKind {
  const current = params.sources?.sources.find((source) => source.id === params.currentId);
  return current?.kind ?? APPS_TAB.id;
}

/** Chooses what to listen to: one app or a whole device. It stands in for the panel's body. */
export function SourcePicker({
  sources,
  currentId,
  pendingId,
  error,
  onSelect,
  onBack,
  initialTab,
}: SourcePickerProps) {
  const idPrefix = useId();
  const [chosenTab, setChosenTab] = useState<SourceKind | null>(initialTab ?? null);
  const listsApps = sources?.listsApps ?? true;
  const tabs = listsApps ? [APPS_TAB, DEVICES_TAB] : [DEVICES_TAB];
  const preferredTab = chosenTab ?? openingTab({ sources, currentId });
  const tab = listsApps ? preferredTab : DEVICES_TAB.id;
  const options = (sources?.sources ?? [])
    .filter((source) => source.kind === tab)
    .map(sourceOption);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    onBack();
  };

  return (
    <div className="picker" data-ref="picker" onKeyDown={onKeyDown}>
      <div className="picker-top">
        <button
          className="icon"
          data-ref="pickerBack"
          title="Back to readouts"
          aria-label="Back"
          onClick={onBack}
        >
          ‹
        </button>
        <span className="picker-title">Listen to</span>
      </div>
      <Tabs
        items={tabs}
        selected={tab}
        onSelect={setChosenTab}
        label="Source kind"
        idPrefix={idPrefix}
      />
      <div className="picker-body" {...tabPanelProps({ idPrefix, id: tab })}>
        {sources === null ? (
          <div className="option-empty">Loading…</div>
        ) : (
          <OptionList
            key={tab}
            options={options}
            selectedId={currentId}
            onSelect={onSelect}
            label={tab === 'app' ? 'Apps' : 'Devices'}
            emptyText={EMPTY_TEXT[tab]}
            pendingId={pendingId}
          />
        )}
      </div>
      {error !== null && <Notice tone="error" message={error} />}
    </div>
  );
}
