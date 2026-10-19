import { describe, expect, it } from "vitest";
import { formatByUnit } from "./units";

describe("formatByUnit", () => {
  it("formats percents with precision", () => {
    expect(formatByUnit(12.46, "percent", 1)).toBe("12.5%");
    expect(formatByUnit(0, "percent", 0)).toBe("0%");
  });

  it("uses binary prefixes for byte capacities", () => {
    expect(formatByUnit(1024, "bytes")).toBe("1.0 KiB");
    expect(formatByUnit(1.5 * 1024 ** 3, "bytes")).toBe("1.5 GiB");
    expect(formatByUnit(512, "bytes")).toBe("512 B");
  });

  it("uses decimal SI prefixes for rates", () => {
    expect(formatByUnit(1_500_000, "bytes_per_second")).toBe("1.5 MB/s");
    expect(formatByUnit(2_400_000_000, "bits_per_second")).toBe("2.4 Gb/s");
  });

  it("formats temperature, fan, frequency and power", () => {
    expect(formatByUnit(55.02, "celsius", 1)).toBe("55.0 °C");
    expect(formatByUnit(2400.4, "rpm")).toBe("2400 RPM");
    expect(formatByUnit(4_056_000_000, "hertz")).toBe("4.06 GHz");
    expect(formatByUnit(17.2, "watts", 1)).toBe("17.2 W");
  });

  it("formats counts and booleans", () => {
    expect(formatByUnit(811, "count")).toBe("811");
    expect(formatByUnit(1, "boolean")).toBe("on");
    expect(formatByUnit(0, "boolean")).toBe("off");
  });

  it("renders non-finite input as a dash, never a number", () => {
    expect(formatByUnit(Number.NaN, "percent")).toBe("—");
  });
});
