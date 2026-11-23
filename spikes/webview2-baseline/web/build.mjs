// SPK-008: bundles src/main.tsx into ../assets/app.js. The output is
// committed so the Rust spike embeds it with include_bytes! and never needs
// Node at cargo build time. Rebuild from this directory:
//   corepack pnpm@10.18.0 install --ignore-workspace --frozen-lockfile
//   corepack pnpm@10.18.0 --ignore-workspace run build
import { build } from 'esbuild';

await build({
  entryPoints: ['src/main.tsx'],
  outfile: '../assets/app.js',
  bundle: true,
  format: 'iife',
  // WebView2 (Chromium) is the only measured engine; SPK-009 sets WebKit targets.
  target: ['chrome120'],
  minify: true,
  legalComments: 'eof',
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': '"production"' },
  charset: 'utf8',
  logLevel: 'info',
});
