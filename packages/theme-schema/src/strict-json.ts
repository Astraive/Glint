// Strict JSON reader mirroring crates/glint-schema/src/strict_json.rs, which
// is serde_json plus a duplicate-key refusal. `JSON.parse` cannot be used: it
// silently keeps the last duplicate, accepts unpaired surrogate escapes, has
// no nesting limit and loses the integer/float distinction that decides
// whether `1.0` is a valid `schemaVersion`.

import { reject } from "./codes.ts";

/** serde_json refuses the 128th nested container. */
export const MAX_NESTING = 127;

const U64_MAX = 18446744073709551615n;

export type JsonValue =
  | { readonly type: "null" }
  | { readonly type: "boolean"; readonly value: boolean }
  | {
      readonly type: "number";
      readonly value: number;
      /** Set only for a non-negative integer literal that fits in u64 (serde_json `as_u64`). */
      readonly u64: bigint | null;
      readonly raw: string;
    }
  | { readonly type: "string"; readonly value: string }
  | { readonly type: "array"; readonly items: readonly JsonValue[] }
  | { readonly type: "object"; readonly entries: ReadonlyMap<string, JsonValue> };

export type JsonObject = Extract<JsonValue, { type: "object" }>;

const NUMBER = /-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/y;
const HEX4 = /^[0-9a-fA-F]{4}$/;

class Reader {
  private pos = 0;
  private depth = 0;
  private readonly text: string;

  constructor(text: string) {
    this.text = text;
  }

  private fail(message: string): never {
    return reject("GLINT_MANIFEST_INVALID_JSON", "", `${message} at offset ${this.pos}`);
  }

  private skipWhitespace(): void {
    while (this.pos < this.text.length) {
      const c = this.text.charCodeAt(this.pos);
      if (c === 0x20 || c === 0x09 || c === 0x0a || c === 0x0d) this.pos += 1;
      else break;
    }
  }

  document(): JsonValue {
    this.skipWhitespace();
    const value = this.value();
    this.skipWhitespace();
    if (this.pos !== this.text.length) this.fail("trailing characters");
    return value;
  }

  private value(): JsonValue {
    if (this.pos >= this.text.length) this.fail("EOF while parsing a value");
    const c = this.text[this.pos];
    switch (c) {
      case "{":
        return this.object();
      case "[":
        return this.array();
      case '"':
        return { type: "string", value: this.string() };
      case "t":
        return this.literal("true", { type: "boolean", value: true });
      case "f":
        return this.literal("false", { type: "boolean", value: false });
      case "n":
        return this.literal("null", { type: "null" });
      default:
        return this.number();
    }
  }

  private literal(word: string, value: JsonValue): JsonValue {
    if (!this.text.startsWith(word, this.pos)) this.fail("expected value");
    this.pos += word.length;
    return value;
  }

  private number(): JsonValue {
    NUMBER.lastIndex = this.pos;
    const match = NUMBER.exec(this.text);
    if (match === null) this.fail("expected value");
    const raw = match[0];
    this.pos += raw.length;
    const value = Number(raw);
    if (!Number.isFinite(value)) this.fail("number out of range");
    let u64: bigint | null = null;
    if (/^(?:0|[1-9][0-9]*)$/.test(raw)) {
      const big = BigInt(raw);
      if (big <= U64_MAX) u64 = big;
    }
    return { type: "number", value, u64, raw };
  }

  private enter(): void {
    this.depth += 1;
    if (this.depth > MAX_NESTING) this.fail("recursion limit exceeded");
  }

  private array(): JsonValue {
    this.enter();
    this.pos += 1; // [
    const items: JsonValue[] = [];
    this.skipWhitespace();
    if (this.text[this.pos] === "]") {
      this.pos += 1;
    } else {
      for (;;) {
        this.skipWhitespace();
        items.push(this.value());
        this.skipWhitespace();
        const c = this.text[this.pos];
        this.pos += 1;
        if (c === "]") break;
        if (c !== ",") this.fail("expected `,` or `]`");
      }
    }
    this.depth -= 1;
    return { type: "array", items };
  }

  private object(): JsonValue {
    this.enter();
    this.pos += 1; // {
    const entries = new Map<string, JsonValue>();
    this.skipWhitespace();
    if (this.text[this.pos] === "}") {
      this.pos += 1;
    } else {
      for (;;) {
        this.skipWhitespace();
        if (this.text[this.pos] !== '"') this.fail("key must be a string");
        const key = this.string();
        // Like the Rust visitor: refused as soon as the repeated key is read.
        if (entries.has(key)) {
          reject("GLINT_MANIFEST_DUPLICATE_KEY", "", `duplicate key \`${key}\``);
        }
        this.skipWhitespace();
        if (this.text[this.pos] !== ":") this.fail("expected `:`");
        this.pos += 1;
        this.skipWhitespace();
        entries.set(key, this.value());
        this.skipWhitespace();
        const c = this.text[this.pos];
        this.pos += 1;
        if (c === "}") break;
        if (c !== ",") this.fail("expected `,` or `}`");
      }
    }
    this.depth -= 1;
    return { type: "object", entries };
  }

  private hex4(): number {
    const digits = this.text.slice(this.pos, this.pos + 4);
    if (!HEX4.test(digits)) this.fail("invalid \\u escape");
    this.pos += 4;
    return Number.parseInt(digits, 16);
  }

  private string(): string {
    this.pos += 1; // opening quote
    let out = "";
    for (;;) {
      if (this.pos >= this.text.length) this.fail("EOF while parsing a string");
      const code = this.text.codePointAt(this.pos) ?? 0;
      if (code === 0x22) {
        this.pos += 1;
        return out;
      }
      if (code < 0x20) this.fail("control character in string");
      if (code >= 0xd800 && code <= 0xdfff) this.fail("unpaired surrogate in input");
      if (code !== 0x5c) {
        out += String.fromCodePoint(code);
        this.pos += code > 0xffff ? 2 : 1;
        continue;
      }
      this.pos += 1; // backslash
      const escape = this.text[this.pos];
      this.pos += 1;
      switch (escape) {
        case '"':
          out += '"';
          break;
        case "\\":
          out += "\\";
          break;
        case "/":
          out += "/";
          break;
        case "b":
          out += "\b";
          break;
        case "f":
          out += "\f";
          break;
        case "n":
          out += "\n";
          break;
        case "r":
          out += "\r";
          break;
        case "t":
          out += "\t";
          break;
        case "u": {
          const unit = this.hex4();
          if (unit >= 0xdc00 && unit <= 0xdfff) this.fail("lone trailing surrogate");
          if (unit >= 0xd800 && unit <= 0xdbff) {
            if (this.text.slice(this.pos, this.pos + 2) !== "\\u") this.fail("lone leading surrogate");
            this.pos += 2;
            const low = this.hex4();
            if (low < 0xdc00 || low > 0xdfff) this.fail("lone leading surrogate");
            out += String.fromCharCode(unit, low);
          } else {
            out += String.fromCharCode(unit);
          }
          break;
        }
        default:
          this.fail("invalid escape");
      }
    }
  }
}

/** Parse `text`, refusing duplicate keys anywhere in the document. */
export function parseStrictJson(text: string): JsonValue {
  return new Reader(text).document();
}

/** Convert to plain data. Keys such as `__proto__` stay own data properties. */
export function toPlain(value: JsonValue): unknown {
  switch (value.type) {
    case "null":
      return null;
    case "boolean":
    case "number":
    case "string":
      return value.value;
    case "array":
      return value.items.map(toPlain);
    case "object":
      return Object.fromEntries([...value.entries].map(([k, v]) => [k, toPlain(v)]));
  }
}
