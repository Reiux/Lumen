// Compatibility layer replacing @wailsio/runtime with Tauri 2 underneath

export function hasTauri(): boolean {
  return typeof window !== 'undefined' && (!!(window as any).__TAURI__ || !!(window as any).__TAURI_INTERNALS__);
}

export async function tauriInvoke<T = any>(cmd: string, args?: Record<string, any>): Promise<T> {
  if (typeof window !== 'undefined') {
    const w = window as any;
    const inv = w.__TAURI__?.core?.invoke || w.__TAURI_INTERNALS__?.invoke || w.__TAURI__?.invoke;
    if (inv) {
      return inv(cmd, args);
    }
  }
  return Promise.resolve(null as any);
}

if (typeof window !== 'undefined') {
  (window as any).__setTab = (tab: string) => {
    const map: Record<string, string> = {
      'upload': 'Upload view',
      'folders': 'Backup folders view',
      'activity': 'Activity logs view',
      'accounts': 'Accounts view',
      'settings': 'Settings view',
      'about': 'About view',
    };
    const aria = map[tab] || tab;
    const btn = document.querySelector(`button[aria-label="${aria}"]`) as HTMLElement;
    if (btn) btn.click();
  };

  (window as any).__setTheme = (mode: 'dark' | 'light') => {
    import('../utils/theme').then(({ currentThemeSeed, currentPaletteVariant }) => {
      import('@materialwebunofficial/md3e-web').then(({ applyDynamicTheme }) => {
        const isDark = mode === 'dark';
        applyDynamicTheme(currentThemeSeed.value, isDark, currentPaletteVariant.value);
        if (isDark) {
          document.documentElement.classList.add('dark');
          document.documentElement.classList.remove('light');
        } else {
          document.documentElement.classList.remove('dark');
          document.documentElement.classList.add('light');
        }
      });
    });
  };

  (window as any).__openSignIn = () => {
    const accBtn = document.querySelector('button[aria-label="Accounts view"]') as HTMLElement;
    if (accBtn) accBtn.click();
    setTimeout(() => {
      const addBtn = Array.from(document.querySelectorAll('button')).find(b => b.textContent?.includes('Add account')) as HTMLElement;
      if (addBtn) addBtn.click();
    }, 200);
  };

  (window as any).__startUpload = (files: string[]) => {
    return tauriInvoke('start_upload', { files });
  };

}

const eventListeners: Record<string, Set<(event: { data: any }) => void>> = {};
const nativeListenAttached: Record<string, boolean> = {};

export const Events = {
  On(eventName: string, callback: (event: { data: any }) => void): () => void {
    if (!eventListeners[eventName]) {
      eventListeners[eventName] = new Set();
    }
    eventListeners[eventName].add(callback);

    if (hasTauri() && !nativeListenAttached[eventName]) {
      nativeListenAttached[eventName] = true;
      const w = window as any;
      const tauriListen = w.__TAURI__?.event?.listen || w.__TAURI_INTERNALS__?.listen;
      if (tauriListen) {
        tauriListen(eventName, (tauriEvent: any) => {
          const payload = tauriEvent.payload;
          console.log(`[EVENT RECEIVED] ${eventName}:`, JSON.stringify(payload));
          const set = eventListeners[eventName];
          if (set) {
            set.forEach((cb) => {
              try {
                cb({ data: payload });
              } catch (err) {
                console.error(`Error in listener for ${eventName}:`, err);
              }
            });
          }
        });
      }
    }

    return () => {
      const set = eventListeners[eventName];
      if (set) {
        set.delete(callback);
      }
    };
  },

  Once(eventName: string, callback: (event: { data: any }) => void): () => void {
    const unlisten = Events.On(eventName, (ev) => {
      unlisten();
      callback(ev);
    });
    return unlisten;
  },

  Off(eventName: string, callback?: (event: { data: any }) => void) {
    if (!callback) {
      delete eventListeners[eventName];
    } else {
      const set = eventListeners[eventName];
      if (set) {
        set.delete(callback);
      }
    }
  },

  async Emit(eventName: string, data?: any): Promise<void> {
    // Local dispatch
    const set = eventListeners[eventName];
    if (set) {
      set.forEach((cb) => {
        try {
          cb({ data });
        } catch (err) {
          console.error(`Error in local dispatch for ${eventName}:`, err);
        }
      });
    }

    if (hasTauri()) {
      if (eventName === 'startUpload') {
        return tauriInvoke('start_upload', { files: data?.files || [] });
      }
      if (eventName === 'uploadCancel') {
        return tauriInvoke('cancel_upload');
      }
      if (eventName === 'app:ready') {
        return tauriInvoke('app_ready');
      }
      if (eventName === 'dev:test-notification') {
        return tauriInvoke('test_notification');
      }

      const w = window as any;
      const tauriEmit = w.__TAURI__?.event?.emit || w.__TAURI_INTERNALS__?.emit;
      if (tauriEmit) {
        return tauriEmit(eventName, data);
      }
    }
  },
};

export const Browser = {
  async OpenURL(url: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('open_url', { url });
    }
    if (typeof window !== 'undefined') {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
  },
};

export const Clipboard = {
  async SetText(text: string): Promise<void> {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      try {
        await navigator.clipboard.writeText(text);
        return;
      } catch {
        // Fallback to tauri
      }
    }
    if (hasTauri()) {
      return tauriInvoke('clipboard_set_text', { text });
    }
  },

  async Text(): Promise<string> {
    if (navigator.clipboard && navigator.clipboard.readText) {
      try {
        return await navigator.clipboard.readText();
      } catch {
        // Fallback to tauri
      }
    }
    if (hasTauri()) {
      return (await tauriInvoke('clipboard_get_text')) || '';
    }
    return '';
  },
};

export const Window = {
  async SetResizable(resizable: boolean): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_resizable', { resizable });
    }
  },

  async SetMinSize(width: number, height: number): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_min_size', { width, height });
    }
  },

  async Center(): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('center_window');
    }
  },

  async SetTitle(title: string): Promise<void> {
    if (hasTauri()) {
      return tauriInvoke('set_title', { title });
    }
    if (typeof document !== 'undefined') {
      document.title = title;
    }
  },
};

export const Create = {
  Nullable: (fn: any) => (val: any) => (val === null || val === undefined ? null : fn ? fn(val) : val),
  Array: (fn: any) => (arr: any[]) => (Array.isArray(arr) ? arr.map((item) => (fn ? fn(item) : item)) : []),
  Map: (_k: any, _v: any) => (obj: any) => obj || {},
  Any: (val: any) => val,
};

export const Call = {
  async ByID(_id: number, ..._args: any[]): Promise<any> {
    return null;
  },
};

export type CancellablePromise<T> = Promise<T>;
