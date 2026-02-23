(() => {
  const storageKey = "minerals.theme";
  const root = document.documentElement;

  const normalize = (value) => (value === "dark" ? "dark" : "light");

  const readStoredTheme = () => {
    try {
      return localStorage.getItem(storageKey);
    } catch (_error) {
      return null;
    }
  };

  const writeStoredTheme = (theme) => {
    try {
      localStorage.setItem(storageKey, theme);
    } catch (_error) {
      // Ignore storage failures (private mode / restricted environments).
    }
  };

  const getToggleButtons = () => Array.from(document.querySelectorAll("[data-theme-toggle]"));
  const getThemeLogos = () => Array.from(document.querySelectorAll("[data-logo-light][data-logo-dark]"));

  const applyTheme = (theme, persist) => {
    const next = normalize(theme);
    root.setAttribute("data-theme", next);

    if (persist) {
      writeStoredTheme(next);
    }

    const isDark = next === "dark";
    getToggleButtons().forEach((button) => {
      button.setAttribute("aria-pressed", isDark ? "true" : "false");
      button.setAttribute("title", isDark ? "Switch to light mode" : "Switch to dark mode");
    });

    getThemeLogos().forEach((img) => {
      const darkSrc = img.getAttribute("data-logo-dark");
      const lightSrc = img.getAttribute("data-logo-light");
      if (!darkSrc || !lightSrc) return;
      img.setAttribute("src", isDark ? darkSrc : lightSrc);
    });
  };

  const initialize = () => {
    const stored = readStoredTheme();
    applyTheme(stored ? stored : "light", false);

    getToggleButtons().forEach((button) => {
      button.addEventListener("click", () => {
        const current = normalize(root.getAttribute("data-theme"));
        const next = current === "dark" ? "light" : "dark";
        applyTheme(next, true);
      });
    });

  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", initialize);
  } else {
    initialize();
  }
})();
