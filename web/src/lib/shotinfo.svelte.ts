import { SvelteMap } from "svelte/reactivity";
import { api } from "./api";
import type { Photo, ShotInfo } from "./types";

const LIMIT = 400;

export class ShotInfoCache {
  #known = new SvelteMap<string, ShotInfo | null>();
  #pending: Record<string, boolean> = {};

  get(photo: Photo): ShotInfo | null | undefined {
    const key = `${photo.id}:${photo.version}`;
    if (this.#known.has(key)) return this.#known.get(key);
    if (!this.#pending[key]) {
      this.#pending[key] = true;
      void api
        .shotInfo(photo)
        .then(
          (info) => this.#remember(key, info),
          () => this.#remember(key, null),
        )
        .finally(() => {
          this.#pending[key] = false;
        });
    }
    return undefined;
  }

  #remember(key: string, info: ShotInfo | null) {
    this.#known.set(key, info);
    if (this.#known.size > LIMIT) {
      const oldest = this.#known.keys().next().value;
      if (oldest !== undefined) this.#known.delete(oldest);
    }
  }
}

export const shotInfo = new ShotInfoCache();

export function exposureLine(info: ShotInfo): string {
  const parts = [
    info.exposure_time ? `${info.exposure_time} s` : null,
    info.f_number ? `f/${formatNumber(info.f_number)}` : null,
    info.iso ? `ISO ${info.iso}` : null,
  ];
  return parts.filter(Boolean).join(" · ");
}

export function formatNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1).replace(/\.0$/, "");
}

export function exposureBias(value: number | null): string | null {
  if (value === null || Math.abs(value) < 0.01) return value === null ? null : "0 EV";
  const thirds = Math.round(value * 3);
  const whole = Math.trunc(thirds / 3);
  const rest = Math.abs(thirds % 3);
  const sign = thirds < 0 ? "−" : "+";
  const fraction = rest === 1 ? "⅓" : rest === 2 ? "⅔" : "";
  const integer = Math.abs(whole) > 0 || !fraction ? String(Math.abs(whole)) : "";
  return `${sign}${integer}${fraction} EV`;
}

export function focalLine(info: ShotInfo): string | null {
  if (!info.focal_length) return null;
  const real = `${formatNumber(info.focal_length)} mm`;
  return info.focal_length_35mm && info.focal_length_35mm !== Math.round(info.focal_length)
    ? `${real} (${info.focal_length_35mm} mm eq.)`
    : real;
}
