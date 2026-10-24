// Stand-in types for src/wasm/pkg before `pnpm build:wasm` has generated it. tsconfig `paths`
// tries the real package first, so these only apply while it's missing. Keep in sync with
// crates/hunch-wasm.
import type { Estimate, SessionEvent } from '@/lib/messages';

export interface InitInput {
  module_or_path?: string | URL | Request | Response | BufferSource | WebAssembly.Module;
}

export default function init(input?: InitInput): Promise<unknown>;
export function version(): string;

export class Analyzer {
  constructor(sampleRate: number);
  push(samples: Float32Array): Estimate | undefined;
  reset(): void;
  free(): void;
}

export class Session {
  constructor(sampleRate: number);
  push(samples: Float32Array): SessionEvent[];
  reset(): void;
  free(): void;
}
