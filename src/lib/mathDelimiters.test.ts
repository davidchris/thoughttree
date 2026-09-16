import { describe, it, expect } from 'vitest';
import { normalizeMathDelimiters } from './mathDelimiters';

describe('normalizeMathDelimiters', () => {
  it('converts \\[...\\] display math to $$...$$', () => {
    const input = 'Start:\n\\[ x_1+x_2=M,\\qquad x_1,x_2\\geq 0. \\]\nEnd.';
    expect(normalizeMathDelimiters(input)).toBe(
      'Start:\n$$ x_1+x_2=M,\\qquad x_1,x_2\\geq 0. $$\nEnd.'
    );
  });
});

describe('normalizeMathDelimiters inline', () => {
  it('converts \\(...\\) inline math to $...$', () => {
    expect(normalizeMathDelimiters('where \\(x_1 \\geq 0\\) holds')).toBe('where $x_1 \\geq 0$ holds');
  });
});

describe('normalizeMathDelimiters leaves code alone', () => {
  it('does not touch fenced code blocks', () => {
    const input = 'text\n```tex\n\\[ a+b \\]\n```\nafter \\[ c \\]';
    expect(normalizeMathDelimiters(input)).toBe('text\n```tex\n\\[ a+b \\]\n```\nafter $$ c $$');
  });

  it('does not touch inline code', () => {
    const input = 'use `\\(x\\)` for \\(x\\)';
    expect(normalizeMathDelimiters(input)).toBe('use `\\(x\\)` for $x$');
  });
});
