import { useRef, useCallback, useEffect, useState, useId } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { vscDarkPlus } from 'react-syntax-highlighter/dist/esm/styles/prism';
import mermaid from 'mermaid';
import 'katex/dist/katex.min.css';

// Initialize mermaid once
mermaid.initialize({
  startOnLoad: false,
  theme: 'base',
  // Mermaid renders into an SVG that does not resolve CSS variables.
  // Values mirror design/tokens.css: teal-black neutrals, one mint accent.
  themeVariables: {
    background: '#113b43', // --tt-surface-2
    // Force all node types to the same dark surface
    primaryColor: '#0a3038', // --tt-surface
    secondaryColor: '#0a3038',
    tertiaryColor: '#0a3038',
    quaternaryColor: '#0a3038',
    // Text colors - all light
    primaryTextColor: '#e6f4ee', // --tt-text
    secondaryTextColor: '#e6f4ee',
    tertiaryTextColor: '#e6f4ee',
    // Borders
    primaryBorderColor: '#c0facc', // --tt-accent
    secondaryBorderColor: '#c0facc',
    tertiaryBorderColor: '#c0facc',
    lineColor: '#4f7a76', // --tt-edge
    // Node styling
    mainBkg: '#0a3038',
    nodeBkg: '#0a3038',
    nodeBorder: '#c0facc',
    nodeTextColor: '#e6f4ee',
    // Default/base colors
    defaultLinkColor: '#4f7a76',
    // Clusters
    clusterBkg: '#04252c', // --tt-bg
    clusterBorder: '#1a464e', // --tt-line
    // Labels
    titleColor: '#e6f4ee',
    edgeLabelBackground: '#113b43',
    // Notes
    noteBkgColor: '#0a3038',
    noteTextColor: '#e6f4ee',
    noteBorderColor: '#1a464e',
    // Flowchart specific
    fillType0: '#0a3038',
    fillType1: '#0a3038',
    fillType2: '#0a3038',
    fillType3: '#0a3038',
    fillType4: '#0a3038',
    fillType5: '#0a3038',
    fillType6: '#0a3038',
    fillType7: '#0a3038',
  },
  flowchart: {
    curve: 'basis',
    padding: 15,
  },
});

interface MermaidDiagramProps {
  code: string;
}

function MermaidDiagram({ code }: MermaidDiagramProps) {
  const [svg, setSvg] = useState<string>('');
  const [error, setError] = useState<string | null>(null);
  const uniqueId = useId().replace(/:/g, '-');

  useEffect(() => {
    let cancelled = false;

    async function render() {
      try {
        const { svg: renderedSvg } = await mermaid.render(`mermaid-${uniqueId}`, code);
        if (!cancelled) {
          setSvg(renderedSvg);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : 'Failed to render diagram');
          setSvg('');
        }
      }
    }

    render();
    return () => { cancelled = true; };
  }, [code, uniqueId]);

  if (error) {
    return (
      <div className="mermaid-error">
        <div className="mermaid-error-label">Mermaid Error</div>
        <pre>{error}</pre>
        <pre className="mermaid-source">{code}</pre>
      </div>
    );
  }

  if (!svg) {
    return <div className="mermaid-loading">Rendering diagram...</div>;
  }

  return (
    <div
      className="mermaid-diagram"
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}

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
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={{
          // Code blocks with syntax highlighting or mermaid rendering
          code({ node, className, children, ...props }) {
            const match = /language-(\w+)/.exec(className || '');
            const language = match ? match[1] : null;
            const isInline = !match && !className;

            if (isInline) {
              return (
                <code className="inline-code" {...props}>
                  {children}
                </code>
              );
            }

            // Render mermaid diagrams
            if (language === 'mermaid') {
              const code = String(children).replace(/\n$/, '');
              return <MermaidDiagram code={code} />;
            }

            return (
              <SyntaxHighlighter
                style={vscDarkPlus}
                language={language || 'text'}
                PreTag="div"
                customStyle={{
                  margin: '0.5em 0',
                  borderRadius: 'var(--tt-radius)',
                  fontSize: 'var(--tt-size-xs)',
                  background: 'var(--tt-surface-2)',
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