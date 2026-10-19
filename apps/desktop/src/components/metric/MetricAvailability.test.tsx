import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MetricAvailability } from "./MetricAvailability";
import { TooltipProvider } from "@/components/ui/tooltip";

function renderChip(availability: Parameters<typeof MetricAvailability>[0]["availability"]) {
  return render(
    <TooltipProvider>
      <MetricAvailability availability={availability} reasonCode="REASON_X" />
    </TooltipProvider>
  );
}

describe("MetricAvailability", () => {
  it("renders a labeled chip for every non-available state", () => {
    const cases: [Parameters<typeof MetricAvailability>[0]["availability"], string][] = [
      ["unsupported", "Not supported"],
      ["permission_required", "Permission required"],
      ["temporarily_unavailable", "Temporarily unavailable"],
      ["stale", "Stale"],
      ["error", "Error"],
    ];
    for (const [availability, label] of cases) {
      const { unmount } = renderChip(availability);
      expect(screen.getByText(label), `state ${availability}`).toBeInTheDocument();
      unmount();
    }
  });

  it("renders nothing for available (live value speaks for itself)", () => {
    const { container } = renderChip("available");
    expect(container).toBeEmptyDOMElement();
  });
});
