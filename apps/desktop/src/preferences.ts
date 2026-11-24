// The manager's colour scheme: system, light or dark, saved per user in
// the webview's local storage. Preferences puts the choice on
// <html data-color-scheme>, and brand.css resolves every colour from it.

import { createColorScheme, createLocalStorage } from "@ui-sdk/core";

export const colorScheme = createColorScheme({
  global: true,
  storage: createLocalStorage("glint-manager-color-scheme"),
});
