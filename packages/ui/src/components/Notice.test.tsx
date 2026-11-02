import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Notice } from './Notice';

describe('Notice', () => {
  it('announces errors as alerts with their detail', () => {
    render(<Notice tone="error" message="Couldn't listen to Speakers" detail="device in use" />);

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent("Couldn't listen to Speakers");
    expect(alert).toHaveTextContent('device in use');
    expect(alert).toHaveAttribute('data-tone', 'error');
  });

  it('announces warnings politely, without a dismiss button unless asked', () => {
    render(<Notice tone="warning" message="Spotify closed" />);

    expect(screen.getByRole('status')).toHaveTextContent('Spotify closed');
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('calls back from the dismiss button', () => {
    const onDismiss = vi.fn();
    render(<Notice tone="info" message="Hi" onDismiss={onDismiss} />);

    fireEvent.click(screen.getByRole('button', { name: 'Dismiss' }));

    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
