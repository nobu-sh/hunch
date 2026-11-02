export type Orientation = 'horizontal' | 'vertical';

const PREVIOUS_KEY: Record<Orientation, string> = { horizontal: 'ArrowLeft', vertical: 'ArrowUp' };
const NEXT_KEY: Record<Orientation, string> = { horizontal: 'ArrowRight', vertical: 'ArrowDown' };

/**
 * Where an arrow, Home or End key moves focus in a row or column of `count` items, wrapping at
 * the ends. Null for any other key.
 */
export function movedIndex(params: {
  key: string;
  index: number;
  count: number;
  orientation: Orientation;
}): number | null {
  const { key, index, count, orientation } = params;
  if (count === 0) return null;
  switch (key) {
    case PREVIOUS_KEY[orientation]:
      return (index - 1 + count) % count;
    case NEXT_KEY[orientation]:
      return (index + 1) % count;
    case 'Home':
      return 0;
    case 'End':
      return count - 1;
    default:
      return null;
  }
}
