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
  themeVariables: {
    // Catppuccin Mocha palette - ALL backgrounds dark
    background: '#1e1e2e',
    // Force all node types to dark background
    primaryColor: '#313244',
    secondaryColor: '#313244',
    tertiaryColor: '#313244',
    quaternaryColor: '#313244',
    // Text colors - all light
    primaryTextColor: '#cdd6f4',
    secondaryTextColor: '#cdd6f4',
    tertiaryTextColor: '#cdd6f4',
    // Borders
    primaryBorderColor: '#89b4fa',
    secondaryBorderColor: '#89b4fa',
    tertiaryBorderColor: '#89b4fa',
    lineColor: '#6c7086',
    // Node styling
    mainBkg: '#313244',
    nodeBkg: '#313244',
    nodeBorder: '#89b4fa',
    nodeTextColor: '#cdd6f4',
    // Default/base colors
    defaultLinkColor: '#6c7086',
    // Clusters
    clusterBkg: '#181825',
    clusterBorder: '#45475a',
    // Labels
    titleColor: '#cdd6f4',
    edgeLabelBackground: '#1e1e2e',
    // Notes
    noteBkgColor: '#313244',
    noteTextColor: '#cdd6f4',
    noteBorderColor: '#45475a',
    // Flowchart specific
    fillType0: '#313244',
    fillType1: '#313244',
    fillType2: '#313244',
    fillType3: '#313244',
    fillType4: '#313244',
    fillType5: '#313244',
    fillType6: '#313244',
    fillType7: '#313244',
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
