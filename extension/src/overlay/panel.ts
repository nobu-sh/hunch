import type { HistoryRowView, OverlayView } from '@/lib/overlay-view';

const COPIED_FEEDBACK_MS = 1200;
const EMPTY = '—';

// Static markup only; every value goes in through textContent.
const TEMPLATE = `
<div class="panel">
  <div class="header" data-ref="header">
    <span class="brand">Hunch</span>
    <span class="status" data-ref="status"></span>
    <button class="icon" data-ref="collapse" title="Collapse" aria-label="Collapse">–</button>
    <button class="icon" data-ref="close" title="Turn off Hunch for this tab" aria-label="Turn off">×</button>
  </div>
  <div class="readouts">
    <div class="key">
      <div class="label">Key <span class="tuning" data-ref="tuning"></span></div>
      <div class="value" data-ref="keyValue"><span data-ref="key"></span><span class="sub" data-ref="relative"></span></div>
      <div class="meter" data-ref="keyMeter"><div data-ref="keyBar"></div></div>
    </div>
    <div class="bpm">
      <div class="label">BPM</div>
      <div class="value" data-ref="bpmValue"><span data-ref="bpm"></span><span class="sub" data-ref="alternate"></span></div>
      <div class="meter" data-ref="bpmMeter"><div data-ref="bpmBar"></div></div>
    </div>
  </div>
  <div class="notes">
    <span class="notes-text" data-ref="notes"></span>
    <button class="copy" data-ref="copyNotes">Copy</button>
  </div>
  <div class="history" data-ref="history">
    <div class="label history-title">History</div>
    <ol data-ref="historyList"></ol>
  </div>
</div>
<button class="pill" data-ref="pill" title="Expand Hunch"><span class="dot" data-ref="pillDot"></span><span data-ref="pillText"></span></button>
`;

const REF_NAMES = [
  'header',
  'status',
  'collapse',
  'close',
  'tuning',
  'keyValue',
  'key',
  'relative',
  'keyMeter',
  'keyBar',
  'bpmValue',
  'bpm',
  'alternate',
  'bpmMeter',
  'bpmBar',
  'notes',
  'copyNotes',
  'history',
  'historyList',
  'pill',
  'pillDot',
  'pillText',
] as const;

type Refs = Record<(typeof REF_NAMES)[number], HTMLElement>;

export interface Panel {
  root: HTMLElement;
  header: HTMLElement;
  pill: HTMLElement;
  collapseButton: HTMLElement;
  closeButton: HTMLElement;
  render(view: OverlayView): void;
  setCollapsed(isCollapsed: boolean): void;
}

function queryRefs(root: HTMLElement): Refs {
  const entries = REF_NAMES.map((name) => {
    const element = root.querySelector<HTMLElement>(`[data-ref="${name}"]`);
    if (!element) throw new Error(`Hunch overlay template is missing [data-ref="${name}"]`);
    return [name, element] as const;
  });
  return Object.fromEntries(entries) as Refs;
}

async function copyWithFeedback(params: { button: HTMLElement; text: string }): Promise<void> {
  const { button, text } = params;
  const label = button.textContent;
  try {
    await navigator.clipboard.writeText(text);
    button.textContent = 'Copied';
  } catch (error) {
    // The async clipboard needs a secure, focused page.
    console.warn('Hunch could not copy to the clipboard', error);
    button.textContent = 'Failed';
  }
  setTimeout(() => {
    button.textContent = label;
  }, COPIED_FEEDBACK_MS);
}

function bindCopy(params: { button: HTMLElement; getText: () => string | null }): void {
  params.button.addEventListener('click', () => {
    const text = params.getText();
    if (text === null) return;
    void copyWithFeedback({ button: params.button, text });
  });
}

function renderMeter(params: {
  meter: HTMLElement;
  bar: HTMLElement;
  confidence: number;
  isLocked: boolean;
}): void {
  params.bar.style.width = `${String(Math.round(params.confidence * 100))}%`;
  params.meter.dataset.locked = String(params.isLocked);
}

function historyItem(row: HistoryRowView): HTMLLIElement {
  const item = document.createElement('li');
  const cells = [
    { className: 'time', text: row.time },
    { className: 'key-cell', text: row.key },
    { className: 'bpm-cell', text: row.bpm },
  ];
  for (const cell of cells) {
    const span = document.createElement('span');
    span.className = cell.className;
    span.textContent = cell.text;
    item.append(span);
  }
  const copy = document.createElement('button');
  copy.className = 'copy';
  copy.textContent = 'Copy';
  copy.title = row.copyText;
  bindCopy({ button: copy, getText: () => row.copyText });
  item.append(copy);
  return item;
}

export function createPanel(): Panel {
  const root = document.createElement('div');
  root.className = 'root';
  root.innerHTML = TEMPLATE;
  const refs = queryRefs(root);
  let currentNotes: string | null = null;

  bindCopy({ button: refs.copyNotes, getText: () => currentNotes });

  function render(view: OverlayView): void {
    refs.status.textContent = view.status;
    refs.status.title = view.status;
    refs.status.dataset.tone = view.statusTone;
    refs.pillDot.dataset.tone = view.statusTone;
    refs.pillText.textContent = view.pill;

    refs.key.textContent = view.key?.label ?? EMPTY;
    refs.relative.textContent = view.key?.relative ?? '';
    refs.keyValue.dataset.locked = String(view.key?.isLocked ?? false);
    renderMeter({
      meter: refs.keyMeter,
      bar: refs.keyBar,
      confidence: view.key?.confidence ?? 0,
      isLocked: view.key?.isLocked ?? false,
    });
    refs.tuning.textContent = view.tuning ?? '';

    refs.bpm.textContent = view.bpm?.label ?? EMPTY;
    refs.alternate.textContent = view.bpm?.alternate ?? '';
    refs.bpmValue.dataset.locked = String(view.bpm?.isLocked ?? false);
    renderMeter({
      meter: refs.bpmMeter,
      bar: refs.bpmBar,
      confidence: view.bpm?.confidence ?? 0,
      isLocked: view.bpm?.isLocked ?? false,
    });

    currentNotes = view.key?.notes ?? null;
    refs.notes.textContent = currentNotes ?? EMPTY;
    refs.copyNotes.hidden = currentNotes === null;

    refs.history.hidden = view.history.length === 0;
    refs.historyList.replaceChildren(...view.history.map(historyItem));
  }

  function setCollapsed(isCollapsed: boolean): void {
    root.dataset.collapsed = String(isCollapsed);
  }

  return {
    root,
    header: refs.header,
    pill: refs.pill,
    collapseButton: refs.collapse,
    closeButton: refs.close,
    render,
    setCollapsed,
  };
}
