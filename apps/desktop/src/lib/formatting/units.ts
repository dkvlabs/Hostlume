/**
 * Unit formatting at the display boundary only (contract §1). Binary
 * prefixes (KiB/GiB) for capacities; decimal SI prefixes (MB/s) for rates,
 * per requirements.md ("Distinguish SI rates from binary capacities").
 */
import type { MetricUnit } from "@/lib/ipc/types";

const BINARY_STEPS = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"] as const;
const DECIMAL_STEPS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;
const DECIMAL_BIT_STEPS = ["b", "Kb", "Mb", "Gb", "Tb", "Pb"] as const;

function formatStepped(bytes: number, steps: readonly string[], base: number, decimals: number): string {
  if (!Number.isFinite(bytes)) return "—";
  const sign = bytes < 0 ? "-" : "";
  let magnitude = Math.abs(bytes);
  let step = 0;
  while (magnitude >= base && step < steps.length - 1) {
    magnitude /= base;
    step += 1;
  }
  const digits = step === 0 ? 0 : decimals;
  return `${sign}${magnitude.toFixed(digits)} ${steps[step]}`;
}

export function formatByUnit(value: number, unit: MetricUnit, precision = 1): string {
  // Non-finite numbers never render as values (contract: no NaN).
  if (!Number.isFinite(value)) return "—";
  switch (unit) {
    case "percent":
      return `${value.toFixed(precision)}%`;
    case "bytes":
      return formatStepped(value, BINARY_STEPS, 1024, 1);
    case "bytes_per_second":
      return `${formatStepped(value, DECIMAL_STEPS, 1000, 1)}/s`;
    case "bits_per_second":
      return `${formatStepped(value, DECIMAL_BIT_STEPS, 1000, 1)}/s`;
    case "celsius":
      return `${value.toFixed(precision)} °C`;
    case "rpm":
      return `${Math.round(value)} RPM`;
    case "seconds":
      return `${value.toFixed(precision)} s`;
    case "count":
      return Math.round(value).toLocaleString();
    case "watts":
      return `${value.toFixed(precision)} W`;
    case "hertz": {
      const ghz = value / 1e9;
      if (Math.abs(ghz) >= 1) return `${ghz.toFixed(2)} GHz`;
      const mhz = value / 1e6;
      if (Math.abs(mhz) >= 1) return `${mhz.toFixed(0)} MHz`;
      return `${value.toFixed(0)} Hz`;
    }
    case "boolean":
      return value ? "on" : "off";
    case "text":
      return String(value);
  }
}
