// Accept/reject port of the `semver` crate (1.0.x) parsers used by
// crates/glint-schema: `Version::parse` (strict SemVer 2.0) and
// `VersionReq::parse` (Cargo-style requirements). Only validity is mirrored;
// matching versions against requirements stays in Rust.
// tests/package-fixtures/manifests/version-strings.json pins the behaviour.

const U64_MAX = 18446744073709551615n;
const MAX_COMPARATORS = 32;

type Step = { readonly rest: string } | null;

function isDigit(c: string | undefined): boolean {
  return c !== undefined && c >= "0" && c <= "9";
}

function isIdentChar(c: string | undefined): boolean {
  return (
    c !== undefined &&
    ((c >= "A" && c <= "Z") || (c >= "a" && c <= "z") || c === "-" || (c >= "0" && c <= "9"))
  );
}

function trimSpaces(text: string): string {
  let i = 0;
  while (text[i] === " ") i += 1;
  return text.slice(i);
}

/** Decimal u64 without leading zeros (`numeric_identifier`). */
function numeric(input: string): Step {
  let len = 0;
  let value = 0n;
  while (isDigit(input[len])) {
    if (value === 0n && len > 0) return null; // leading zero
    value = value * 10n + BigInt(input.charCodeAt(len) - 48);
    if (value > U64_MAX) return null;
    len += 1;
  }
  return len > 0 ? { rest: input.slice(len) } : null;
}

/** Dot-separated `[0-9A-Za-z-]+` identifiers (`identifier`); empty is an error for callers. */
function identifier(input: string, prerelease: boolean): { ident: string; rest: string } | null {
  let accumulated = 0;
  let segment = 0;
  let nonDigit = false;
  for (;;) {
    const c = input[accumulated + segment];
    if (isIdentChar(c)) {
      if (!isDigit(c)) nonDigit = true;
      segment += 1;
      continue;
    }
    if (segment === 0) {
      if (accumulated === 0 && c !== ".") return { ident: "", rest: input };
      return null; // empty segment
    }
    if (prerelease && segment > 1 && !nonDigit && input[accumulated] === "0") return null;
    accumulated += segment;
    if (c === ".") {
      accumulated += 1;
      segment = 0;
      nonDigit = false;
    } else {
      return { ident: input.slice(0, accumulated), rest: input.slice(accumulated) };
    }
  }
}

function wildcard(input: string): string | null {
  const c = input[0];
  return c === "*" || c === "x" || c === "X" ? input.slice(1) : null;
}

/** `semver::Version::parse(text).is_ok()` */
export function isValidVersion(text: string): boolean {
  if (text === "") return false;
  let step = numeric(text);
  for (let part = 0; part < 2; part += 1) {
    if (step === null || !step.rest.startsWith(".")) return false;
    step = numeric(step.rest.slice(1));
  }
  if (step === null) return false;
  let rest = step.rest;
  if (rest.startsWith("-")) {
    const pre = identifier(rest.slice(1), true);
    if (pre === null || pre.ident === "") return false;
    rest = pre.rest;
  }
  if (rest.startsWith("+")) {
    const build = identifier(rest.slice(1), false);
    if (build === null || build.ident === "") return false;
    rest = build.rest;
  }
  return rest === "";
}

/** One comparator; returns the unparsed remainder (`comparator`). */
function comparator(input: string): Step {
  // The operator (or its absence) changes meaning, never validity.
  let text = input;
  if (text.startsWith(">=") || text.startsWith("<=")) text = text.slice(2);
  else if (/^[=><~^]/.test(text)) text = text.slice(1);
  text = trimSpaces(text);

  const major = numeric(text);
  if (major === null) return null;
  text = major.rest;
  let hasWildcard = false;
  let hasPatch = false;

  if (text.startsWith(".")) {
    text = text.slice(1);
    const star = wildcard(text);
    if (star !== null) {
      hasWildcard = true;
      text = star;
    } else {
      const minor = numeric(text);
      if (minor === null) return null;
      text = minor.rest;
    }
  }
  if (text.startsWith(".")) {
    text = text.slice(1);
    const star = wildcard(text);
    if (star !== null) {
      text = star;
    } else if (hasWildcard) {
      return null;
    } else {
      const patch = numeric(text);
      if (patch === null) return null;
      text = patch.rest;
      hasPatch = true;
    }
  }
  if (hasPatch && text.startsWith("-")) {
    const pre = identifier(text.slice(1), true);
    if (pre === null || pre.ident === "") return null;
    text = pre.rest;
  }
  if (hasPatch && text.startsWith("+")) {
    const build = identifier(text.slice(1), false);
    if (build === null || build.ident === "") return null;
    text = build.rest;
  }
  return { rest: trimSpaces(text) };
}

/** `semver::VersionReq::parse(text).is_ok()` */
export function isValidVersionRequirement(input: string): boolean {
  let text = trimSpaces(input);
  const star = wildcard(text);
  if (star !== null) return trimSpaces(star) === "";
  for (let depth = 0; ; depth += 1) {
    const step = comparator(text);
    if (step === null) return false;
    if (step.rest === "") return true;
    if (!step.rest.startsWith(",")) return false;
    if (depth + 1 === MAX_COMPARATORS) return false;
    text = trimSpaces(step.rest.slice(1));
  }
}
