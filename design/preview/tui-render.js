/* tui-render.js — tiny helpers to draw aligned, colored terminal UI in HTML.
   Renders each "row" as a <div class="tui-row"> (white-space: pre) so box-drawing
   glyphs in fixed columns line up vertically across rows. Coloring is done with
   inline <span class>, which does not disturb the character grid. */
(function (g) {
  function esc(s){ return String(s).replace(/[&<>]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;'}[c])); }

  // pad/truncate a string to an exact cell width
  function pad(s, w, align){
    s = String(s);
    if (w == null) return s;
    if (s.length > w) return s.slice(0, w);
    const gap = w - s.length;
    if (align === 'right')  return ' '.repeat(gap) + s;
    if (align === 'center') return ' '.repeat(gap>>1) + s + ' '.repeat(gap - (gap>>1));
    return s + ' '.repeat(gap);
  }

  // one colored segment → HTML; optionally padded to width w
  function seg(text, cls, w, align){
    const t = (w != null) ? pad(text, w, align) : String(text);
    return cls ? `<span class="${cls}">${esc(t)}</span>` : esc(t);
  }

  // assemble a row from segments (strings already produced by seg())
  function line(...segs){ return `<div class="tui-row">${segs.join('')}</div>`; }

  // a horizontal rule of given inner width, with corner/junction chars
  function rule(inner, left, right, fill, cls){
    return line(seg((left||'├') + (fill||'─').repeat(inner) + (right||'┤'), cls||'tui-line'));
  }

  g.esc = esc; g.pad = pad; g.seg = seg; g.line = line; g.rule = rule;
})(window);
