import { HqlError, type Span } from "./ast";

export type TokenKind =
  | "word" // bare identifier or keyword; the parser decides which
  | "string"
  | "number"
  | "operator"
  | "punct"
  | "eof";

export interface Token {
  kind: TokenKind;
  /** The raw text, with string quotes and escapes already resolved. */
  text: string;
  /** Uppercased `text`, so keyword comparisons stay case-insensitive. */
  upper: string;
  /** Set for `number` tokens; bigint when the literal exceeds Number.MAX_SAFE_INTEGER. */
  numeric?: number | bigint;
  /** True for `"quoted"` identifiers, which are never treated as keywords. */
  quoted?: boolean;
  span: Span;
}

const OPERATORS = [">=", "<=", "!=", "<>", "=", ">", "<"];
const PUNCTUATION = new Set(["(", ")", ",", "*", ".", ":", ";"]);

const isDigit = (c: string) => c >= "0" && c <= "9";
const isIdentStart = (c: string) => /[A-Za-z_$]/.test(c);
const isIdentPart = (c: string) => /[A-Za-z0-9_$]/.test(c);

/**
 * Turns HelixSQL source into tokens.
 *
 * Comments (`--` to end of line, and `/* … *\/`) are dropped. Identifiers may be
 * quoted with double quotes or backticks so that labels or properties which
 * collide with keywords stay reachable.
 */
export function tokenize(source: string): Token[] {
  const tokens: Token[] = [];
  let index = 0;
  let line = 1;
  let lineStart = 0;

  const spanFrom = (start: number, startLine: number, startLineStart: number): Span => ({
    start,
    end: index,
    line: startLine,
    column: start - startLineStart + 1,
  });

  const advanceNewlines = (from: number, to: number) => {
    for (let i = from; i < to; i++) {
      if (source[i] === "\n") {
        line++;
        lineStart = i + 1;
      }
    }
  };

  while (index < source.length) {
    const char = source[index];

    if (char === "\n") {
      index++;
      line++;
      lineStart = index;
      continue;
    }
    if (char === " " || char === "\t" || char === "\r") {
      index++;
      continue;
    }

    // Comments.
    if (char === "-" && source[index + 1] === "-") {
      while (index < source.length && source[index] !== "\n") index++;
      continue;
    }
    if (char === "/" && source[index + 1] === "*") {
      const start = index;
      index += 2;
      while (index < source.length && !(source[index] === "*" && source[index + 1] === "/")) index++;
      if (index >= source.length) {
        advanceNewlines(start, index);
        throw new HqlError("unterminated block comment", spanFrom(start, line, lineStart));
      }
      advanceNewlines(start, index);
      index += 2;
      continue;
    }

    const startIndex = index;
    const startLine = line;
    const startLineStart = lineStart;

    // Single-quoted strings, with '' as the escape for a literal quote.
    if (char === "'") {
      index++;
      let value = "";
      for (;;) {
        if (index >= source.length) {
          throw new HqlError(
            "unterminated string literal",
            spanFrom(startIndex, startLine, startLineStart),
            "add a closing '",
          );
        }
        if (source[index] === "'") {
          if (source[index + 1] === "'") {
            value += "'";
            index += 2;
            continue;
          }
          index++;
          break;
        }
        if (source[index] === "\n") {
          line++;
          lineStart = index + 1;
        }
        value += source[index++];
      }
      tokens.push({
        kind: "string",
        text: value,
        upper: value.toUpperCase(),
        span: spanFrom(startIndex, startLine, startLineStart),
      });
      continue;
    }

    // Quoted identifiers — always a `word`, never reinterpreted as a keyword.
    if (char === '"' || char === "`") {
      const quote = char;
      index++;
      let value = "";
      for (;;) {
        if (index >= source.length || source[index] === "\n") {
          throw new HqlError(
            "unterminated quoted identifier",
            spanFrom(startIndex, startLine, startLineStart),
            `add a closing ${quote}`,
          );
        }
        if (source[index] === quote) {
          index++;
          break;
        }
        value += source[index++];
      }
      tokens.push({
        kind: "word",
        text: value,
        // Quoting is how you escape a keyword, so blank out the keyword form.
        upper: "",
        quoted: true,
        span: spanFrom(startIndex, startLine, startLineStart),
      });
      continue;
    }

    if (isDigit(char) || (char === "-" && isDigit(source[index + 1] ?? ""))) {
      if (char === "-") index++;
      while (index < source.length && isDigit(source[index])) index++;
      let isFloat = false;
      if (source[index] === "." && isDigit(source[index + 1] ?? "")) {
        isFloat = true;
        index++;
        while (index < source.length && isDigit(source[index])) index++;
      }
      if (source[index] === "e" || source[index] === "E") {
        const save = index;
        index++;
        if (source[index] === "+" || source[index] === "-") index++;
        if (isDigit(source[index] ?? "")) {
          isFloat = true;
          while (index < source.length && isDigit(source[index])) index++;
        } else {
          index = save;
        }
      }
      const text = source.slice(startIndex, index);
      // Integers beyond 2^53 must stay exact: HelixDB stores i64.
      const numeric = isFloat
        ? Number(text)
        : Number.isSafeInteger(Number(text))
          ? Number(text)
          : BigInt(text);
      tokens.push({
        kind: "number",
        text,
        upper: text,
        numeric,
        span: spanFrom(startIndex, startLine, startLineStart),
      });
      continue;
    }

    if (isIdentStart(char)) {
      while (index < source.length && isIdentPart(source[index])) index++;
      const text = source.slice(startIndex, index);
      tokens.push({
        kind: "word",
        text,
        upper: text.toUpperCase(),
        span: spanFrom(startIndex, startLine, startLineStart),
      });
      continue;
    }

    const operator = OPERATORS.find((op) => source.startsWith(op, index));
    if (operator) {
      index += operator.length;
      tokens.push({
        kind: "operator",
        text: operator,
        upper: operator,
        span: spanFrom(startIndex, startLine, startLineStart),
      });
      continue;
    }

    if (PUNCTUATION.has(char)) {
      index++;
      tokens.push({
        kind: "punct",
        text: char,
        upper: char,
        span: spanFrom(startIndex, startLine, startLineStart),
      });
      continue;
    }

    index++;
    throw new HqlError(
      `unexpected character ${JSON.stringify(char)}`,
      spanFrom(startIndex, startLine, startLineStart),
    );
  }

  tokens.push({
    kind: "eof",
    text: "",
    upper: "",
    span: { start: source.length, end: source.length, line, column: source.length - lineStart + 1 },
  });
  return tokens;
}
