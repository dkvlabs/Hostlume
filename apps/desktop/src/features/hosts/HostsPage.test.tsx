/**
 * Hosts page (phase 10): the UI honesty gates. A stale or offline host is
 * never rendered as live; removal is double-confirmed with an explicit
 * purge choice; pairing failures surface their typed reason; and an empty
 * registry renders the honest empty state with no network promise.
 */
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type * as ClientModule from "@/lib/ipc/client";
import type { HostLiveState } from "@/lib/ipc/types";
import { TooltipProvider } from "@/components/ui/tooltip";
import { HostsPage } from "./HostsPage";
import { useHostsStore } from "@/stores/hostsStore";

const mocks = vi.hoisted(() => ({
  pairHost: vi.fn(),
  removeHost: vi.fn(),
  renameHost: vi.fn(),
  setHostRecording: vi.fn(),
  /** What the mocked `getHostsState` reports — kept in lockstep with the
   * store seed so the page's own refresh cannot wipe the fixture. */
  hosts: [] as HostLiveState[],
}));

vi.mock("@/lib/ipc/client", async (importOriginal) => {
  const actual = await importOriginal<typeof ClientModule>();
  return {
    ...actual,
    ipc: {
      ...actual.ipc,
      getHostsState: vi.fn(async () => mocks.hosts),
      pairHost: mocks.pairHost,
      removeHost: mocks.removeHost,
      renameHost: mocks.renameHost,
      setHostRecording: mocks.setHostRecording,
    },
    onHostsUpdated: vi.fn(async () => () => undefined),
  };
});

function hostFixture(overrides: Partial<HostLiveState> = {}): HostLiveState {
  return {
    host_id: "remote-1",
    label: "Office Mac",
    endpoint: "192.168.1.5:45454",
    fingerprint_sha256: "ab".repeat(32),
    device_id: "console-1",
    protocol_version: 1,
    record_history: false,
    status: "online",
    last_success_ms: 1_792_368_000_000,
    age_ms: 1_200,
    last_error_code: null,
    last_error: null,
    remote_collected_at_utc: "2026-10-19T12:00:00.000Z",
    remote_sequence: 42,
    clock_skew_ms: 250,
    no_sample_yet: false,
    agent_version: "0.1.0",
    agent_display_name: "Hostlume host",
    families: ["cpu", "memory"],
    headline: [["cpu.total.usage", [{ ts_utc_ms: 1_792_368_000_000, value: 31.5 }]]],
    readings: [
      {
        metric_id: "cpu.total.usage",
        entity_id: "host",
        value: 31.5,
        unit: "percent",
        collected_at_utc: "2026-10-19T12:00:00.000Z",
        sequence: 42,
      },
    ],
    ...overrides,
  };
}

function seed(hosts: HostLiveState[]) {
  mocks.hosts = hosts;
  useHostsStore.setState({ hosts, status: "ready", error: null });
}

function renderPage() {
  return render(
    <TooltipProvider>
      <HostsPage />
    </TooltipProvider>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  seed([]);
  mocks.pairHost.mockReset();
  mocks.removeHost.mockReset();
  mocks.hosts = [];
});

describe("HostsPage", () => {
  it("renders the honest empty state and never a placeholder host", () => {
    renderPage();
    expect(screen.getByTestId("hosts-empty")).toBeTruthy();
    expect(screen.queryByTestId(/host-card-/)).toBeNull();
  });

  it("never renders a stale host as live: values stay visible under the stale badge", () => {
    seed([hostFixture({ status: "stale", age_ms: 24_000 })]);
    renderPage();
    const card = screen.getByTestId("host-card-remote-1");
    expect(card.getAttribute("data-status")).toBe("stale");
    expect(within(card).getByText("Stale")).toBeTruthy();
    expect(within(card).queryByText("Live")).toBeNull();
    // The last value is still shown — with its age, not as current.
    expect((screen.getByTestId("host-cpu").textContent ?? "").length).toBeGreaterThan(0);
    expect(within(card).getByText(/24 s/)).toBeTruthy();
  });

  it("stale detail view carries the explicit warning, not a live claim", async () => {
    const user = userEvent.setup();
    seed([hostFixture({ status: "stale", age_ms: 24_000 })]);
    renderPage();
    await user.click(screen.getByRole("button", { name: "Readings" }));
    const detail = await screen.findByTestId("host-detail");
    expect(detail.textContent).toContain("Values below are stale");
    // The agent's timestamp appears for both the clock row and the reading.
    expect(
      within(detail).getAllByText("2026-10-19T12:00:00.000Z").length
    ).toBeGreaterThanOrEqual(2);
  });

  it("an offline host with a terminal error shows the reason, not zeros", () => {
    seed([
      hostFixture({
        status: "cert_mismatch",
        last_error_code: "cert_mismatch",
        last_error: "certificate does not match the pinned fingerprint",
      }),
    ]);
    renderPage();
    const card = screen.getByTestId("host-card-remote-1");
    expect(within(card).getByText("Certificate changed — remove and pair again")).toBeTruthy();
    expect(within(card).getByTestId("host-error").textContent).toContain("fingerprint");
  });

  it("removal is double-confirmed and honours the purge choice exactly once", async () => {
    const user = userEvent.setup();
    mocks.removeHost.mockResolvedValue({
      host_id: "remote-1",
      history_purged: true,
      history_deleted: { raw_deleted: 3, rollup_1m_deleted: 1, rollup_1h_deleted: 1 },
    });
    seed([hostFixture()]);
    renderPage();
    await user.click(screen.getByRole("button", { name: "Remove" }));
    // Cancel first: nothing is called.
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(mocks.removeHost).not.toHaveBeenCalled();
    // Confirm with purge checked: called once, with the flag.
    await user.click(screen.getByRole("button", { name: "Remove" }));
    await user.click(screen.getByTestId("remove-purge-row").querySelector("input")!);
    await user.click(screen.getByRole("button", { name: "Remove host" }));
    await waitFor(() => expect(mocks.removeHost).toHaveBeenCalledTimes(1));
    expect(mocks.removeHost).toHaveBeenCalledWith("remote-1", true);
    await waitFor(() =>
      expect(screen.queryByTestId("host-card-remote-1")).toBeNull()
    );
  });

  it("pairing failures surface their typed reason and the code field", async () => {
    const user = userEvent.setup();
    const { IpcCommandError } = await vi.importActual<typeof ClientModule>(
      "@/lib/ipc/client"
    );
    mocks.pairHost.mockRejectedValue(
      new IpcCommandError({
        code: "HOST_CODE_INVALID",
        messageKey: "errors.hostCodeInvalid",
        reason: undefined,
      })
    );
    renderPage();
    await user.click(screen.getByRole("button", { name: "Pair a host" }));
    await user.type(screen.getByLabelText("Address (host:port)"), "192.168.1.5:45454");
    await user.type(screen.getByLabelText("Pairing code"), "ABCDEFGH");
    await user.type(screen.getByLabelText("Name shown here"), "Office Mac");
    await user.click(screen.getByRole("button", { name: "Pair" }));
    await waitFor(() => expect(mocks.pairHost).toHaveBeenCalledTimes(1));
    expect(mocks.pairHost).toHaveBeenCalledWith("192.168.1.5:45454", "ABCDEFGH", "Office Mac");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "That pairing code is not well-formed (8 letters/digits)."
    );
  });

  it("a successful pairing pins the fingerprint and shows it back to the user", async () => {
    const user = userEvent.setup();
    mocks.pairHost.mockResolvedValue(hostFixture());
    renderPage();
    await user.click(screen.getByRole("button", { name: "Pair a host" }));
    await user.type(screen.getByLabelText("Address (host:port)"), "192.168.1.5:45454");
    await user.type(screen.getByLabelText("Pairing code"), "abcdefgh");
    await user.type(screen.getByLabelText("Name shown here"), "Office Mac");
    await user.click(screen.getByRole("button", { name: "Pair" }));
    expect(await screen.findByTestId("pair-form")).toHaveTextContent(
      "ab".repeat(32)
    );
  });
});
