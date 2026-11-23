// Fails when an installed npm dependency (production or development) has a
// license outside the allow-list. Run: pnpm licenses:check (REL-002).
// Needs `pnpm install --frozen-lockfile` first; it reads installed packages.

import { ALLOWED_NPM_LICENSES, licenseAllowed, listNpmPackages } from "./license-lib.mjs";

const packages = listNpmPackages({ prodOnly: false });
const denied = packages.filter((pkg) => !licenseAllowed(pkg.license));

for (const pkg of denied) {
  console.error(`DENIED: ${pkg.name}@${pkg.version} has license "${pkg.license}"`);
}
if (denied.length > 0) {
  console.error(
    `${denied.length} npm package(s) have licenses outside the allow-list: ${[...ALLOWED_NPM_LICENSES].join(", ")}`,
  );
  console.error("See docs/legal/license-policy.md before changing the allow-list.");
  process.exit(1);
}
console.log(`npm licenses OK: ${packages.length} package(s) checked, none denied.`);
