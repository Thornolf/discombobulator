export type Action =
  | "approve"
  | "delete"
  | "unsure"
  | "favorite"
  | "clear"
  | "undo"
  | "rate0"
  | "rate1"
  | "rate2"
  | "rate3"
  | "rate4"
  | "rate5"
  | "focusPrev"
  | "focusNext"
  | "stackPrev"
  | "stackNext"
  | "scenePrev"
  | "sceneNext"
  | "solo"
  | "back"
  | "nextUndecided"
  | "apply"
  | "select"
  | "keep"
  | "duplicates"
  | "open"
  | "help";

export interface Binding {
  key?: string;
  code?: string;
  ctrl?: boolean;
}

export type Keymap = Record<Action, Binding[]>;

const digit = (n: number): Binding[] => [{ code: `Digit${n}` }, { code: `Numpad${n}` }];

export const defaultKeymap: Keymap = {
  approve: [{ key: "a" }, { key: " " }, { key: "l" }],
  delete: [{ key: "x" }, { key: "Delete" }],
  unsure: [{ key: "u" }],
  favorite: [{ key: "f" }],
  clear: [{ key: "Backspace" }],
  undo: [{ key: "z", ctrl: true }],
  rate0: digit(0),
  rate1: digit(1),
  rate2: digit(2),
  rate3: digit(3),
  rate4: digit(4),
  rate5: digit(5),
  focusPrev: [{ key: "ArrowLeft" }],
  focusNext: [{ key: "ArrowRight" }],
  stackPrev: [{ key: "ArrowUp" }],
  stackNext: [{ key: "ArrowDown" }],
  scenePrev: [{ key: "PageUp" }],
  sceneNext: [{ key: "PageDown" }],
  solo: [{ key: "Enter" }],
  back: [{ key: "Escape" }],
  nextUndecided: [{ key: "n" }],
  apply: [{ key: "e" }],
  select: [{ key: "s" }],
  keep: [{ key: "k" }],
  duplicates: [{ key: "d" }],
  open: [{ key: "o" }],
  help: [{ key: "?" }, { key: "h" }],
};

export const actionLabels: Record<Action, string> = {
  approve: "LR approved",
  delete: "Delete",
  unsure: "Unsure",
  favorite: "Favorite",
  clear: "Clear mark",
  undo: "Undo",
  rate0: "Clear rating",
  rate1: "1 star",
  rate2: "2 stars",
  rate3: "3 stars",
  rate4: "4 stars",
  rate5: "5 stars",
  focusPrev: "Previous frame",
  focusNext: "Next frame",
  stackPrev: "Previous stack",
  stackNext: "Next stack",
  scenePrev: "Previous scene",
  sceneNext: "Next scene",
  solo: "Solo view",
  back: "Leave solo / close",
  nextUndecided: "Next undecided stack",
  apply: "Apply",
  select: "Select frame",
  keep: "Keep selected, delete undecided",
  duplicates: "Duplicates",
  open: "Open folder",
  help: "Help",
};

export interface KeyInput {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}

export function matches(binding: Binding, event: KeyInput): boolean {
  if (event.altKey) {
    return false;
  }
  const ctrl = event.ctrlKey || event.metaKey;
  if (Boolean(binding.ctrl) !== ctrl) {
    return false;
  }
  if (binding.code) {
    return binding.code === event.code;
  }
  if (!binding.key) {
    return false;
  }
  return event.key.length === 1 ? event.key.toLowerCase() === binding.key : event.key === binding.key;
}

export function actionFor(keymap: Keymap, event: KeyInput): Action | null {
  for (const [action, bindings] of Object.entries(keymap) as [Action, Binding[]][]) {
    if (bindings.some((binding) => matches(binding, event))) {
      return action;
    }
  }
  return null;
}

export function bindingFor(event: KeyInput): Binding {
  const ctrl = event.ctrlKey || event.metaKey;
  if (/^(Digit|Numpad)[0-9]$/.test(event.code)) {
    return { code: event.code, ctrl };
  }
  return { key: event.key.length === 1 ? event.key.toLowerCase() : event.key, ctrl };
}

export function describe(binding: Binding): string {
  const names: Record<string, string> = {
    " ": "Space",
    ArrowLeft: "←",
    ArrowRight: "→",
    ArrowUp: "↑",
    ArrowDown: "↓",
    Backspace: "⌫",
    Delete: "Del",
    Escape: "Esc",
    PageUp: "PgUp",
    PageDown: "PgDn",
  };
  const base = binding.code
    ? binding.code.replace(/^Digit/, "").replace(/^Numpad/, "Num ")
    : (names[binding.key ?? ""] ?? (binding.key ?? "").toUpperCase());
  return binding.ctrl ? `Ctrl+${base}` : base;
}

const storageKey = "discombobulator.keymap";

export function loadKeymap(): Keymap {
  try {
    const stored = JSON.parse(localStorage.getItem(storageKey) ?? "null") as Partial<Keymap> | null;
    return { ...defaultKeymap, ...(stored ?? {}) };
  } catch {
    return { ...defaultKeymap };
  }
}

export function saveKeymap(keymap: Keymap): void {
  try {
    localStorage.setItem(storageKey, JSON.stringify(keymap));
  } catch {
    return;
  }
}

export function rebind(keymap: Keymap, action: Action, binding: Binding): Keymap {
  const same = (other: Binding) =>
    other.key === binding.key && other.code === binding.code && Boolean(other.ctrl) === Boolean(binding.ctrl);
  const next = Object.fromEntries(
    Object.entries(keymap).map(([name, bindings]) => [name, bindings.filter((other) => !same(other))]),
  ) as Keymap;
  next[action] = [binding];
  return next;
}
