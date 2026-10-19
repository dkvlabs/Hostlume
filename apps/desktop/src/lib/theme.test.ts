import { describe, expect, it } from "vitest";
import { applyAppearance, resolveTheme } from "./theme";

describe("resolveTheme", () => {
  it("follows the OS preference in system mode", () => {
    expect(resolveTheme("system", true)).toBe("light");
    expect(resolveTheme("system", false)).toBe("dark");
  });

  it("honors explicit choices", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("light", false)).toBe("light");
    expect(resolveTheme("dark", true)).toBe("dark");
  });
});

describe("applyAppearance", () => {
  it("writes data-theme (dark canonical default) and lang", () => {
    document.documentElement.removeAttribute("data-theme");
    applyAppearance("dark", true, "en");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.lang).toBe("en");

    applyAppearance("system", true, "ro");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.lang).toBe("ro");

    applyAppearance("system", false);
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
