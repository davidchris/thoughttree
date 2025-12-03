import { useRef, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { vscDarkPlus } from 'react-syntax-highlighter/dist/esm/styles/prism';

interface MarkdownContentProps {
  content: string;
  className?: string;
}

export function MarkdownContent({ content, className = '' }: MarkdownContentProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  // Intercept copy to preserve raw markdown
  const handleCopy = useCallback((e: React.ClipboardEvent) => {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed) return;

    // Get the selected text range
    const range = selection.getRangeAt(0);

    // Check if selection is within our container
    if (containerRef.current?.contains(range.commonAncestorContainer)) {
      e.preventDefault();

      // If entire content is selected (or close to it), use raw markdown
      // Otherwise, let the browser handle partial selection
      const selectedText = selection.toString();
      const plainTextContent = content;

      // Heuristic: if selection is >80% of content length, use raw markdown
      if (selectedText.length > plainTextContent.length * 0.8) {
        e.clipboardData.setData('text/plain', plainTextContent);
      } else {
        // For partial selections, still preserve markdown-like formatting
        e.clipboardData.setData('text/plain', selectedText);
      }
    }
  }, [content]);

  return (
    <div
      ref={containerRef}
      className={`markdown-content ${className}`}
      onCopy={handleCopy}
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          // Code blocks with syntax highlighting
          code({ node, className, children, ...props }) {
            const match = /language-(\w+)/.exec(className || '');
            const isInline = !match && !className;

            if (isInline) {
              return (
                <code className="inline-code" {...props}>
                  {children}
                </code>
              );
            }

            return (
              <SyntaxHighlighter
                style={vscDarkPlus}
                language={match ? match[1] : 'text'}
                PreTag="div"
                customStyle={{
                  margin: '0.5em 0',
                  borderRadius: '4px',
                  fontSize: '12px',
                }}
              >
                {String(children).replace(/\n$/, '')}
              </SyntaxHighlighter>
            );
          },
          // Links open in new tab
          a({ href, children }) {
            return (
              <a href={href} target="_blank" rel="noopener noreferrer">
                {children}
              </a>
            );
          },
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
