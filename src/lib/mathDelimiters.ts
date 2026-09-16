/**
 * Convert LaTeX-style math delimiters to the dollar-sign delimiters that
 * remark-math understands. Models like GPT emit `\[...\]` and `\(...\)`,
 * which plain markdown eats as backslash escapes.
 *
 * Fenced code blocks and inline code spans are left untouched.
 */
export function normalizeMathDelimiters(markdown: string): string {
  // Split on fenced blocks (```...```) and inline code (`...`) so we only
  // rewrite prose segments. Odd-indexed parts are the captured code segments.
  const parts = markdown.split(/(```[\s\S]*?```|`[^`\n]*`)/);
  return parts
    .map((part, i) => (i % 2 === 1 ? part : convertDelimiters(part)))
    .join('');
}

function convertDelimiters(text: string): string {
  return text
    .replace(/\\\[([\s\S]*?)\\\]/g, (_m, body: string) => `$$${body}$$`)
    .replace(/\\\(([\s\S]*?)\\\)/g, (_m, body: string) => `$${body}$`);
}
