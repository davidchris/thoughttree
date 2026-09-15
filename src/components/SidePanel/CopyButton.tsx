import { useState } from 'react';
import { logger } from '../../lib/logger';

interface CopyButtonProps {
  content: string;
}

/** Copies text via the async clipboard API, falling back to execCommand. */
async function copyText(content: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(content);
    return true;
  } catch (error) {
    logger.error('Failed to copy content:', error);
  }

  // Fallback for older browsers
  const textarea = document.createElement('textarea');
  textarea.value = content;
  textarea.style.position = 'fixed';
  textarea.style.opacity = '0';
  document.body.appendChild(textarea);
  textarea.select();
  let copied = false;
  try {
    document.execCommand('copy');
    copied = true;
  } catch (err) {
    logger.error('Fallback copy failed:', err);
  }
  document.body.removeChild(textarea);
  return copied;
}

/** Copy-to-clipboard button with a transient "Copied!" confirmation. */
export function CopyButton({ content }: CopyButtonProps) {
  const [copySuccess, setCopySuccess] = useState(false);

  const handleCopy = async () => {
    if (await copyText(content)) {
      setCopySuccess(true);
      setTimeout(() => setCopySuccess(false), 2000);
    }
  };

  return (
    <button
      className="side-panel-copy-button"
      onClick={() => void handleCopy()}
      title="Copy as markdown"
    >
      {copySuccess ? 'Copied!' : 'Copy'}
    </button>
  );
}
