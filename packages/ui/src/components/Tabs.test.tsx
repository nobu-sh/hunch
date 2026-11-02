import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it } from 'vitest';
import { tabPanelProps, Tabs } from './Tabs';

const ITEMS = [
  { id: 'one', label: 'One' },
  { id: 'two', label: 'Two' },
  { id: 'three', label: 'Three' },
] as const;

type ItemId = (typeof ITEMS)[number]['id'];

function TabsHarness() {
  const [selected, setSelected] = useState<ItemId>('one');
  return (
    <>
      <Tabs items={ITEMS} selected={selected} onSelect={setSelected} label="Numbers" idPrefix="t" />
      <div {...tabPanelProps({ idPrefix: 't', id: selected })}>{selected} panel</div>
    </>
  );
}

describe('Tabs', () => {
  it('names the tab bar and marks only the selected tab', () => {
    render(<TabsHarness />);

    expect(screen.getByRole('tablist', { name: 'Numbers' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'One' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: 'Two' })).toHaveAttribute('aria-selected', 'false');
  });

  it('keeps only the selected tab in the tab order', () => {
    render(<TabsHarness />);

    const tabIndexes = screen.getAllByRole('tab').map((tab) => tab.tabIndex);
    expect(tabIndexes).toEqual([0, -1, -1]);
  });

  it('links the panel to its tab', () => {
    render(<TabsHarness />);

    expect(screen.getByRole('tabpanel', { name: 'One' })).toHaveTextContent('one panel');
  });

  it('selects on click', () => {
    render(<TabsHarness />);

    fireEvent.click(screen.getByRole('tab', { name: 'Three' }));

    expect(screen.getByRole('tabpanel', { name: 'Three' })).toBeInTheDocument();
  });

  it('moves selection and focus with the arrow keys, wrapping, and Home and End', () => {
    render(<TabsHarness />);
    const tab = (name: string) => screen.getByRole('tab', { name });

    fireEvent.keyDown(tab('One'), { key: 'ArrowLeft' });
    expect(tab('Three')).toHaveFocus();
    expect(tab('Three')).toHaveAttribute('aria-selected', 'true');

    fireEvent.keyDown(tab('Three'), { key: 'ArrowRight' });
    expect(tab('One')).toHaveFocus();

    fireEvent.keyDown(tab('One'), { key: 'End' });
    expect(tab('Three')).toHaveFocus();

    fireEvent.keyDown(tab('Three'), { key: 'Home' });
    expect(tab('One')).toHaveAttribute('aria-selected', 'true');
  });
});
