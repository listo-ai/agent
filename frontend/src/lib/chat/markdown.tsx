import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism'
import { CopyButton } from './copy-button'

interface Props {
  content: string
  /** Compact mode uses smaller text/spacing (for embedded panels). */
  compact?: boolean | undefined
}

export function Markdown({ content, compact }: Props) {
  const textSize = compact ? 'text-xs' : 'text-sm'
  const codeSize = compact ? '11px' : '12px'
  const spacing = compact
    ? { my: 'my-1.5', mt: 'mt-2', ml: 'ml-3', p: '10px 12px' }
    : { my: 'my-2', mt: 'mt-3', ml: 'ml-4', p: '12px 16px' }

  return (
    <div className={`${textSize} leading-relaxed max-w-none`}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1({ children }) {
            return (
              <h1
                className={`${compact ? 'text-sm' : 'text-lg'} font-semibold ${
                  compact ? 'mt-3 mb-1.5' : 'mt-4 mb-2'
                } text-foreground`}
              >
                {children}
              </h1>
            )
          },
          h2({ children }) {
            return (
              <h2
                className={`${compact ? 'text-xs' : 'text-base'} font-semibold ${
                  compact ? 'mt-2.5 mb-1' : 'mt-3 mb-1.5'
                } text-foreground`}
              >
                {children}
              </h2>
            )
          },
          h3({ children }) {
            return (
              <h3
                className={`${compact ? 'text-xs' : 'text-sm'} font-semibold mt-2 mb-1 text-foreground`}
              >
                {children}
              </h3>
            )
          },
          p({ children }) {
            return (
              <p className={`${spacing.my} text-foreground/90 leading-relaxed`}>{children}</p>
            )
          },
          strong({ children }) {
            return <strong className="font-semibold text-foreground">{children}</strong>
          },
          ul({ children }) {
            return (
              <ul
                className={`${spacing.my} ${spacing.ml} space-y-0.5 list-disc text-foreground/90`}
              >
                {children}
              </ul>
            )
          },
          ol({ children }) {
            return (
              <ol
                className={`${spacing.my} ${spacing.ml} space-y-0.5 list-decimal text-foreground/90`}
              >
                {children}
              </ol>
            )
          },
          li({ children }) {
            return <li className="leading-relaxed">{children}</li>
          },
          blockquote({ children }) {
            return (
              <blockquote
                className={`border-l-2 border-primary/30 ${compact ? 'pl-2' : 'pl-3'} ${
                  spacing.my
                } text-muted-foreground italic`}
              >
                {children}
              </blockquote>
            )
          },
          hr() {
            return <hr className={`${spacing.my} border-border`} />
          },
          a({ href, children }) {
            return (
              <a
                href={href}
                target="_blank"
                rel="noopener"
                className="text-primary underline underline-offset-2 hover:opacity-70"
              >
                {children}
              </a>
            )
          },
          table({ children }) {
            return (
              <div className={`overflow-x-auto ${spacing.my} border border-border`}>
                <table className={`w-full ${compact ? 'text-[11px]' : 'text-xs'}`}>{children}</table>
              </div>
            )
          },
          thead({ children }) {
            return <thead className="bg-muted/50">{children}</thead>
          },
          th({ children }) {
            return (
              <th
                className={`${
                  compact ? 'px-2 py-1' : 'px-3 py-2'
                } text-left font-medium text-foreground border-b border-border`}
              >
                {children}
              </th>
            )
          },
          td({ children }) {
            return (
              <td
                className={`${
                  compact ? 'px-2 py-1' : 'px-3 py-2'
                } border-b border-border text-foreground/80`}
              >
                {children}
              </td>
            )
          },
          code({ className, children, ...props }) {
            const match = /language-(\w+)/.exec(className || '')
            const text = String(children).replace(/\n$/, '')

            if (match || text.includes('\n')) {
              return (
                <div className="relative group/code">
                  <div className="absolute top-1.5 right-1.5 z-10 opacity-0 group-hover/code:opacity-100 transition-opacity">
                    <CopyButton
                      text={text}
                      className="bg-black/40 backdrop-blur-sm border border-white/10 rounded px-1.5 py-0.5"
                    />
                  </div>
                  <SyntaxHighlighter
                    style={oneDark}
                    language={match?.[1] || 'text'}
                    PreTag="div"
                    customStyle={{
                      margin: 0,
                      borderRadius: 0,
                      fontSize: codeSize,
                      lineHeight: '1.5',
                      padding: spacing.p,
                    }}
                  >
                    {text}
                  </SyntaxHighlighter>
                </div>
              )
            }

            return (
              <code
                className={`px-1 py-0.5 bg-muted text-[${codeSize}] font-mono text-foreground/80`}
                {...props}
              >
                {children}
              </code>
            )
          },
          pre({ children }) {
            return <div className={`${spacing.my} border border-border overflow-hidden`}>{children}</div>
          },
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
}
