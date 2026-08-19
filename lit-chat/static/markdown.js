// Minimal markdown renderer that BUILDS DOM NODES — it never assigns
// innerHTML, so model output (attacker-influenceable content) cannot inject
// markup regardless of what it contains. Supported: paragraphs, headings,
// fenced code blocks, inline code, bold, italic, links (http/https only),
// unordered/ordered lists, blockquotes.

function inline(text) {
  // Returns an array of nodes for one line of inline markdown.
  const nodes = [];
  let i = 0;
  let buf = '';
  const flush = () => {
    if (buf) { nodes.push(document.createTextNode(buf)); buf = ''; }
  };
  while (i < text.length) {
    // inline code
    if (text[i] === '`') {
      const end = text.indexOf('`', i + 1);
      if (end > i) {
        flush();
        const code = document.createElement('code');
        code.textContent = text.slice(i + 1, end);
        nodes.push(code);
        i = end + 1;
        continue;
      }
    }
    // bold
    if (text.startsWith('**', i)) {
      const end = text.indexOf('**', i + 2);
      if (end > i) {
        flush();
        const b = document.createElement('strong');
        inline(text.slice(i + 2, end)).forEach((n) => b.appendChild(n));
        nodes.push(b);
        i = end + 2;
        continue;
      }
    }
    // italic
    if (text[i] === '*' && text[i + 1] !== '*') {
      const end = text.indexOf('*', i + 1);
      if (end > i) {
        flush();
        const em = document.createElement('em');
        inline(text.slice(i + 1, end)).forEach((n) => em.appendChild(n));
        nodes.push(em);
        i = end + 1;
        continue;
      }
    }
    // link [text](url) — http(s) only, everything else renders as text
    if (text[i] === '[') {
      const closeBracket = text.indexOf(']', i);
      if (closeBracket > i && text[closeBracket + 1] === '(') {
        const closeParen = text.indexOf(')', closeBracket + 2);
        if (closeParen > closeBracket) {
          const label = text.slice(i + 1, closeBracket);
          const url = text.slice(closeBracket + 2, closeParen).trim();
          if (/^https?:\/\//i.test(url)) {
            flush();
            const a = document.createElement('a');
            a.href = url;
            a.target = '_blank';
            a.rel = 'noopener noreferrer';
            a.textContent = label;
            nodes.push(a);
            i = closeParen + 1;
            continue;
          }
        }
      }
    }
    buf += text[i];
    i += 1;
  }
  flush();
  return nodes;
}

export function renderMarkdown(text) {
  const frag = document.createDocumentFragment();
  const lines = String(text).split('\n');
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    // fenced code block
    const fence = line.match(/^```(\w*)\s*$/);
    if (fence) {
      const code = [];
      i += 1;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) {
        code.push(lines[i]);
        i += 1;
      }
      i += 1; // skip closing fence (or EOF)
      const pre = document.createElement('pre');
      const codeEl = document.createElement('code');
      if (fence[1]) codeEl.dataset.lang = fence[1];
      codeEl.textContent = code.join('\n');
      pre.appendChild(codeEl);
      frag.appendChild(pre);
      continue;
    }

    // blank line
    if (!line.trim()) { i += 1; continue; }

    // heading
    const heading = line.match(/^(#{1,4})\s+(.*)$/);
    if (heading) {
      const h = document.createElement(`h${Math.min(heading[1].length + 2, 6)}`);
      inline(heading[2]).forEach((n) => h.appendChild(n));
      frag.appendChild(h);
      i += 1;
      continue;
    }

    // blockquote
    if (line.startsWith('> ')) {
      const bq = document.createElement('blockquote');
      while (i < lines.length && lines[i].startsWith('> ')) {
        const p = document.createElement('p');
        inline(lines[i].slice(2)).forEach((n) => p.appendChild(n));
        bq.appendChild(p);
        i += 1;
      }
      frag.appendChild(bq);
      continue;
    }

    // unordered list
    if (/^[-*]\s+/.test(line)) {
      const ul = document.createElement('ul');
      while (i < lines.length && /^[-*]\s+/.test(lines[i])) {
        const li = document.createElement('li');
        inline(lines[i].replace(/^[-*]\s+/, '')).forEach((n) => li.appendChild(n));
        ul.appendChild(li);
        i += 1;
      }
      frag.appendChild(ul);
      continue;
    }

    // ordered list
    if (/^\d+\.\s+/.test(line)) {
      const ol = document.createElement('ol');
      while (i < lines.length && /^\d+\.\s+/.test(lines[i])) {
        const li = document.createElement('li');
        inline(lines[i].replace(/^\d+\.\s+/, '')).forEach((n) => li.appendChild(n));
        ol.appendChild(li);
        i += 1;
      }
      frag.appendChild(ol);
      continue;
    }

    // paragraph: join consecutive non-empty, non-special lines
    const para = [line];
    i += 1;
    while (
      i < lines.length && lines[i].trim() &&
      !/^(```|#{1,4}\s|> |[-*]\s|\d+\.\s)/.test(lines[i])
    ) {
      para.push(lines[i]);
      i += 1;
    }
    const p = document.createElement('p');
    inline(para.join(' ')).forEach((n) => p.appendChild(n));
    frag.appendChild(p);
  }
  return frag;
}
