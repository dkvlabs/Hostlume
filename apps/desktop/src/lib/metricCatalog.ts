/**
 * Shared catalog of numeric metric families with display labels (phase 08).
 * Mirrors `monitor_core::families::numeric_unit` — every id here must be a
 * numeric family the backend accepts, so Compare/Recording and the alert
 * editor never offer a series the engine would reject.
 */

export interface MetricCatalogEntry {
  id: string;
  label: string;
  unit: string;
  /** Typical entity prefix, used to pre-fill entity pickers. */
  entityHint: string;
}

export const METRIC_CATALOG: readonly MetricCatalogEntry[] = [
  { id: "cpu.total.usage", label: "CPU total usage", unit: "percent", entityHint: "host" },
  { id: "cpu.core.usage", label: "CPU core usage", unit: "percent", entityHint: "core:" },
  { id: "memory.physical.used", label: "Memory used", unit: "bytes", entityHint: "host" },
  { id: "memory.available", label: "Memory available", unit: "bytes", entityHint: "host" },
  { id: "memory.swap.used", label: "Swap used", unit: "bytes", entityHint: "host" },
  { id: "disk.volume.available", label: "Volume free space", unit: "bytes", entityHint: "disk:" },
  { id: "disk.device.read_rate", label: "Disk read rate", unit: "bytes_per_second", entityHint: "io:" },
  { id: "disk.device.write_rate", label: "Disk write rate", unit: "bytes_per_second", entityHint: "io:" },
  { id: "net.interface.rx_rate", label: "Network download", unit: "bytes_per_second", entityHint: "if:" },
  { id: "net.interface.tx_rate", label: "Network upload", unit: "bytes_per_second", entityHint: "if:" },
  { id: "gpu.engine.utilization", label: "GPU utilization", unit: "percent", entityHint: "gpu:" },
  { id: "sensor.temperature", label: "Temperature", unit: "celsius", entityHint: "sensor:" },
  { id: "sensor.fan.speed", label: "Fan speed", unit: "rpm", entityHint: "fan:" },
  { id: "battery.charge", label: "Battery charge", unit: "percent", entityHint: "battery:" },
  { id: "host.uptime", label: "Uptime", unit: "seconds", entityHint: "host" },
];

export function metricEntry(metricId: string): MetricCatalogEntry | undefined {
  return METRIC_CATALOG.find((entry) => entry.id === metricId);
}
