import { defineUnlistedScript } from 'wxt/utils/define-unlisted-script';
import { mountOverlay } from '@/overlay/mount';

// Injected by the background with `scripting.executeScript` into enabled tabs only.
export default defineUnlistedScript(() => {
  mountOverlay().catch((error: unknown) => {
    console.error('Hunch failed to mount its overlay', error);
  });
});
