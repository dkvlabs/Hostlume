import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MetricCard } from "./MetricCard";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { MetricReading } from "@/lib/ipc/types";

function reading(overrides: Partial<MetricReading> = {}): MetricReading {
  return {
    metric_id: "cpu.total.usage",
    entity_id: "host",
    value: 12.5,
    unit: "percent",
    availability: "available",
    source: "test",
    collected_at_utc: "2026-10-19T00:00:00.000Z",
    monotonic_sequence: 1,
    precision: 1,
    ...overrides,
  };
}

function renderCard(props: Parameters<typeof MetricCard>[0]) {
  return render(
    <TooltipProvider>
      <MetricCard {...props} />
    </TooltipProvider>
  );
}

describe("MetricCard", () => {
  it("shows the formatted live value with unit and timestamp", () => {
    renderCard({ label: "CPU", unit: "percent", reading: reading() });
    expect(screen.getByText("12.5%")).toBeInTheDocument();
    expect(screen.getByText(/2026-10-19/)).toBeInTheDocument();
  });

  it("never renders a fabricated zero when there is no sample yet", () => {
    renderCard({ label: "CPU", unit: "percent", reading: null });
    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.getByText("Collecting…")).toBeInTheDocument();
    expect(screen.queryByText(/0(\.\d+)?%/)).not.toBeInTheDocument();
  });

  it("renders a dash with the state chip for unsupported metrics", () => {
    renderCard({
      label: "Sensor",
      unit: "celsius",
      reading: reading({
        metric_id: "sensor.temperature",
        unit: "celsius",
        value: null,
        availability: "unsupported",
        reason_code: "NO_PUBLIC_READABLE_SENSOR",
      }),
    });
    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.getByText("Not supported")).toBeInTheDocument();
    expect(screen.queryByText(/0(\.\d+)?°/)).not.toBeInTheDocument();
  });

  it("renders a dash for stale readings (value must be null per contract)", () => {
    renderCard({
      label: "CPU",
      unit: "percent",
      reading: reading({ availability: "stale", value: null }),
    });
    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.getByText("Stale")).toBeInTheDocument();
    const card = screen.getByText("CPU").closest("[data-availability]");
    expect(card).toHaveAttribute("data-availability", "stale");
  });

  it("renders the error state chip when a collector failed", () => {
    renderCard({
      label: "GPU",
      unit: "percent",
      reading: reading({
        value: null,
        availability: "error",
        reason_code: "DRIVER_QUERY_FAILED",
      }),
    });
    expect(screen.getByText("Error")).toBeInTheDocument();
    expect(screen.getByText("—")).toBeInTheDocument();
  });

  it("formats bytes in binary units at the display boundary", () => {
    renderCard({
      label: "Memory",
      unit: "bytes",
      reading: reading({
        value: 1.5 * 1024 ** 3,
        unit: "bytes",
        precision: 1,
      }),
    });
    expect(screen.getByText("1.5 GiB")).toBeInTheDocument();
  });

  it("applies compact density", () => {
    renderCard({ label: "CPU", unit: "percent", reading: reading(), compact: true });
    const card = screen.getByText("CPU").closest("[data-availability]");
    expect(card?.className).toContain("p-3");
  });
});
