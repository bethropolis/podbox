import React from 'react';
import { marked } from 'marked';
import { ExternalLink } from 'lucide-react';
import { withBase } from '../../base';
import { cleanMarkdownLink, resolveAssetUrl } from './links';

export function renderInline(text: string): React.ReactNode[] {
  // Parse inline formatting: code, bold, link, text
  const inlineTokens = marked.Lexer.lexInline(text);

  return inlineTokens.map((token, index) => {
    if (token.type === 'codespan') {
      return (
        <code
          key={index}
          className="px-1.5 py-0.5 rounded-[2px] bg-[var(--bg-surface0)] text-[var(--accent-mauve)] text-[12.5px] border border-[var(--border)] font-mono"
        >
          {token.text}
        </code>
      );
    }
    if (token.type === 'strong') {
      return (
        <strong key={index} className="font-bold text-[var(--text-primary)]">
          {renderInline(token.text)}
        </strong>
      );
    }
    if (token.type === 'em') {
      return (
        <em key={index} className="italic text-[var(--accent-peach)]">
          {renderInline(token.text)}
        </em>
      );
    }
    if (token.type === 'link') {
      const { isInternal, url } = cleanMarkdownLink(token.href);
      if (isInternal) {
        return (
          <a
            key={index}
            href={withBase(url)}
            className="text-[var(--accent-blue)] underline underline-offset-4 decoration-[var(--accent-blue)]/50 hover:decoration-[var(--accent-blue)] transition-colors cursor-pointer"
          >
            {renderInline(token.text)}
          </a>
        );
      }
      return (
        <a
          key={index}
          href={url}
          target="_blank"
          rel="noopener noreferrer"
          className="text-[var(--accent-blue)] inline-flex items-center gap-0.5 underline underline-offset-4 decoration-[var(--accent-blue)]/50 hover:decoration-[var(--accent-blue)]"
        >
          {renderInline(token.text)}
          <ExternalLink className="w-3 h-3 ml-0.5 opacity-70" />
        </a>
      );
    }
    if (token.type === 'image') {
      const resolvedSrc = resolveAssetUrl(token.href);
      return (
        <img
          key={index}
          src={resolvedSrc}
          alt={(token as any).title || (token as any).text || ''}
          className="my-3 max-w-full rounded-[3px] border border-[var(--border)]"
        />
      );
    }
    const rawText = 'text' in token ? (token as any).text : ('raw' in token ? (token as any).raw : '');
    return <span key={index}>{rawText}</span>;
  });
}
