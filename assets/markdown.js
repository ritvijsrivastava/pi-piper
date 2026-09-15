// Pi Piper's small, dependency-free Markdown renderer.
//
// The mobile client receives assistant content from a live session, so raw
// HTML is always escaped and links are restricted to safe URL schemes before
// any markup is inserted into the transcript.
(() => {
  "use strict";

  const TOKEN_PREFIX = "\uE000PIPERTOKEN";
  const TOKEN_SUFFIX = "\uE001";

  const KEYWORDS = {
    javascript:
      "as|async|await|break|case|catch|class|const|continue|debugger|default|delete|do|else|export|extends|finally|for|from|function|if|import|in|instanceof|let|new|of|return|static|super|switch|this|throw|try|typeof|var|void|while|with|yield",
    typescript:
      "as|async|await|break|case|catch|class|const|continue|debugger|default|delete|do|else|export|extends|finally|for|from|function|if|implements|import|in|instanceof|interface|keyof|let|namespace|never|new|of|private|public|readonly|return|static|string|super|switch|this|throw|try|type|typeof|unknown|var|void|while|with|yield",
    python:
      "and|as|assert|async|await|break|case|class|continue|def|del|elif|else|except|False|finally|for|from|global|if|import|in|is|lambda|match|None|nonlocal|not|or|pass|raise|return|True|try|while|with|yield",
    rust:
      "as|async|await|break|const|continue|crate|dyn|else|enum|extern|false|fn|for|if|impl|in|let|loop|match|mod|move|mut|pub|ref|return|self|Self|static|struct|super|trait|true|type|unsafe|use|where|while",
    json: "true|false|null",
    bash: "case|do|done|elif|else|esac|fi|for|function|if|in|select|then|until|while",
    shell: "case|do|done|elif|else|esac|fi|for|function|if|in|select|then|until|while",
  };

  function escapeHtml(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function escapeAttribute(value) {
    return escapeHtml(value).replace(/`/g, "&#96;");
  }

  function safeUrl(value) {
    const url = value.trim();
    if (!url) return null;
    if (/^(?:javascript|vbscript|data|file):/i.test(url)) return null;
    if (/^(?:https?:|mailto:|tel:)/i.test(url)) return url;
    if (/^(?:\/|\.\.?\/|#)/.test(url)) return url;
    return null;
  }

  function createToken(tokens, html) {
    const token = `${TOKEN_PREFIX}${tokens.length}${TOKEN_SUFFIX}`;
    tokens.push(html);
    return token;
  }

  function restoreTokens(value, tokens) {
    return value.replace(new RegExp(`${TOKEN_PREFIX}(\\d+)${TOKEN_SUFFIX}`, "g"), (_, index) => tokens[index]);
  }

  function renderInline(source, depth = 0) {
    if (!source) return "";
    const tokens = [];
    let value = String(source);

    // Protect code spans and links before escaping/formatting the remaining
    // text. Link labels are rendered recursively with a depth guard.
    value = value.replace(/`([^`\n]+)`/g, (_, code) => createToken(tokens, `<code>${escapeHtml(code)}</code>`));
    value = value.replace(/\[([^\]\n]+)\]\((\S+?)(?:\s+["'][^)]*["'])?\)/g, (_, label, url) => {
      const target = safeUrl(url);
      if (!target) return `[${label}](${url})`;
      const renderedLabel = depth < 2 ? renderInline(label, depth + 1) : escapeHtml(label);
      return createToken(
        tokens,
        `<a href="${escapeAttribute(target)}" target="_blank" rel="noopener noreferrer">${renderedLabel}</a>`,
      );
    });

    value = escapeHtml(value);
    value = value.replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>");
    value = value.replace(/__([^_\n]+)__/g, "<strong>$1</strong>");
    value = value.replace(/~~([^~\n]+)~~/g, "<del>$1</del>");
    value = value.replace(/(^|[^*])\*([^*\n]+)\*/g, "$1<em>$2</em>");
    value = value.replace(/(^|[^_])_([^_\n]+)_/g, "$1<em>$2</em>");
    value = value.replace(/ {2}\n/g, "<br>");
    value = value.replace(/\n/g, " ");
    return restoreTokens(value, tokens);
  }

  function normalizeLanguage(info) {
    const language = (info || "").trim().split(/[\s,]/, 1)[0].toLowerCase();
    const aliases = {
      cjs: "javascript",
      js: "javascript",
      jsx: "javascript",
      mjs: "javascript",
      py: "python",
      rb: "ruby",
      rs: "rust",
      sh: "bash",
      shell: "bash",
      ts: "typescript",
      tsx: "typescript",
      yml: "yaml",
    };
    return aliases[language] || language.replace(/[^a-z0-9+#-]/g, "").slice(0, 24);
  }

  function tokenClass(match, language) {
    if (/^(?:\/\/|#|--)/.test(match) || /^\/\*/.test(match)) return "comment";
    if (/^["'`]/.test(match)) return "string";
    if (/^(?:0x[\da-f]+|\d+(?:\.\d+)?)/i.test(match)) return "number";
    const keywordPattern = KEYWORDS[language];
    if (keywordPattern && new RegExp(`^(?:${keywordPattern})$`).test(match)) return "keyword";
    return null;
  }

  function highlightCode(code, language) {
    const normalized = normalizeLanguage(language);
    if (!KEYWORDS[normalized] && !["css", "html", "xml", "yaml", "markdown", "md"].includes(normalized)) {
      return escapeHtml(code);
    }

    const pattern = /\/\*[\s\S]*?\*\/|\/\/[^\n]*|#[^\n]*|--[^\n]*|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`|\b(?:0x[\da-f]+|\d+(?:\.\d+)?)\b|\b[A-Za-z_$][\w$]*\b/gi;
    let output = "";
    let lastIndex = 0;
    for (const match of code.matchAll(pattern)) {
      const index = match.index ?? 0;
      output += escapeHtml(code.slice(lastIndex, index));
      const kind = tokenClass(match[0], normalized);
      output += kind
        ? `<span class="code-token-${kind}">${escapeHtml(match[0])}</span>`
        : escapeHtml(match[0]);
      lastIndex = index + match[0].length;
    }
    return output + escapeHtml(code.slice(lastIndex));
  }

  function renderCodeBlock(code, language) {
    const normalized = normalizeLanguage(language);
    const languageAttribute = normalized ? ` data-language="${escapeAttribute(normalized)}"` : "";
    return `<pre class="markdown-code"${languageAttribute}><code>${highlightCode(code, normalized)}</code></pre>`;
  }

  function renderMarkdown(markdown) {
    const lines = String(markdown || "").replace(/\r\n?/g, "\n").split("\n");
    const blocks = [];
    let paragraph = [];
    let listType = null;
    let listItems = [];
    let fence = null;
    let fenceLines = [];

    function flushParagraph() {
      if (paragraph.length === 0) return;
      blocks.push(`<p>${renderInline(paragraph.join("\n").trim())}</p>`);
      paragraph = [];
    }

    function flushList() {
      if (!listType) return;
      const tag = listType === "ordered" ? "ol" : "ul";
      blocks.push(`<${tag}>${listItems.map((item) => `<li>${renderInline(item)}</li>`).join("")}</${tag}>`);
      listType = null;
      listItems = [];
    }

    for (const line of lines) {
      if (fence) {
        if (new RegExp(`^\\s{0,3}${fence.marker}{3,}\\s*$`).test(line)) {
          blocks.push(renderCodeBlock(fenceLines.join("\n"), fence.language));
          fence = null;
          fenceLines = [];
        } else {
          const indentation = line.match(/^ */)?.[0].length || 0;
          fenceLines.push(line.slice(Math.min(fence.indent, indentation)));
        }
        continue;
      }

      const fenceMatch = /^( *)(`{3,}|~{3,})\s*(.*)$/.exec(line);
      if (fenceMatch && fenceMatch[1].length <= 3) {
        flushParagraph();
        flushList();
        fence = { marker: fenceMatch[2][0], language: fenceMatch[3], indent: fenceMatch[1].length };
        fenceLines = [];
        continue;
      }

      if (!line.trim()) {
        flushParagraph();
        flushList();
        continue;
      }

      const heading = /^\s{0,3}(#{1,3})\s+(.+?)\s*#*\s*$/.exec(line);
      if (heading) {
        flushParagraph();
        flushList();
        const level = heading[1].length;
        blocks.push(`<h${level}>${renderInline(heading[2])}</h${level}>`);
        continue;
      }

      const unordered = /^\s{0,3}[-*+]\s+(.+)$/.exec(line);
      const ordered = /^\s{0,3}\d+[.)]\s+(.+)$/.exec(line);
      if (unordered || ordered) {
        flushParagraph();
        const nextType = unordered ? "unordered" : "ordered";
        if (listType && listType !== nextType) flushList();
        listType = nextType;
        listItems.push((unordered || ordered)[1]);
        continue;
      }

      if (listType) flushList();

      const quote = /^\s{0,3}>\s?(.*)$/.exec(line);
      if (quote) {
        flushParagraph();
        blocks.push(`<blockquote>${renderInline(quote[1])}</blockquote>`);
        continue;
      }

      if (/^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/.test(line)) {
        flushParagraph();
        blocks.push("<hr>");
        continue;
      }

      paragraph.push(line);
    }

    if (fence) blocks.push(renderCodeBlock(fenceLines.join("\n"), fence.language));
    flushParagraph();
    flushList();
    return blocks.join("");
  }

  window.piPiperMarkdown = { renderMarkdown };
})();
