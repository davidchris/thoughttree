/**
 * Returns the viewport-relative pixel coordinates of the caret in a textarea
 * using the mirror-div technique.
 */

const MIRROR_PROPERTIES = [
  'direction', 'boxSizing',
  'width',
  'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft',
  'borderTopWidth', 'borderRightWidth', 'borderBottomWidth', 'borderLeftWidth',
  'borderTopStyle', 'borderRightStyle', 'borderBottomStyle', 'borderLeftStyle',
  'fontFamily', 'fontSize', 'fontWeight', 'fontStyle',
  'letterSpacing', 'lineHeight', 'textTransform',
  'wordSpacing', 'wordWrap', 'overflowWrap', 'tabSize',
  'textIndent',
] as const;

export function getCaretCoordinates(
  textarea: HTMLTextAreaElement,
  caretPosition: number,
): { top: number; left: number; height: number } {
  const div = document.createElement('div');
  const style = div.style;
  const computed = window.getComputedStyle(textarea);

  // Position off-screen but still measurable
  style.position = 'absolute';
  style.visibility = 'hidden';
  style.overflow = 'hidden';
  style.top = '0';
  style.left = '-9999px';
  style.whiteSpace = 'pre-wrap';
  style.wordWrap = 'break-word';

  for (const prop of MIRROR_PROPERTIES) {
    (style as unknown as Record<string, string>)[prop] = (computed as unknown as Record<string, string>)[prop];
  }

  document.body.appendChild(div);

  const textBefore = textarea.value.substring(0, caretPosition);
  div.appendChild(document.createTextNode(textBefore));

  const marker = document.createElement('span');
  marker.textContent = '\u200b'; // zero-width space for measurable height
  div.appendChild(marker);

  const markerTop = marker.offsetTop - textarea.scrollTop;
  const markerLeft = marker.offsetLeft - textarea.scrollLeft;
  const markerHeight = marker.offsetHeight;

  document.body.removeChild(div);

  const rect = textarea.getBoundingClientRect();

  return {
    top: rect.top + markerTop,
    left: rect.left + markerLeft,
    height: markerHeight,
  };
}
