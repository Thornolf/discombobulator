export interface ViewOptions {
  info: boolean;
  focusPoint: boolean;
  histogram: boolean;
  colorHistogram: boolean;
  clipping: boolean;
  phoneInfo: boolean;
}

const storageKey = "discombobulator.view";

export const defaultView: ViewOptions = {
  info: true,
  focusPoint: true,
  histogram: true,
  colorHistogram: true,
  clipping: false,
  phoneInfo: false,
};

export function restoreView(stored: unknown): ViewOptions {
  const view = { ...defaultView };
  if (stored && typeof stored === "object") {
    for (const name of Object.keys(defaultView) as (keyof ViewOptions)[]) {
      const value = (stored as Record<string, unknown>)[name];
      if (typeof value === "boolean") view[name] = value;
    }
  }
  return view;
}

export class ViewState {
  options = $state<ViewOptions>(load());

  toggle(name: keyof ViewOptions) {
    this.options = { ...this.options, [name]: !this.options[name] };
    try {
      localStorage.setItem(storageKey, JSON.stringify(this.options));
    } catch {
      return;
    }
  }
}

function load(): ViewOptions {
  try {
    return restoreView(JSON.parse(localStorage.getItem(storageKey) ?? "null"));
  } catch {
    return { ...defaultView };
  }
}
