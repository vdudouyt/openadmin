const SETS = {
  single: { tl:'┌', tr:'┐', bl:'└', br:'┘', h:'─', v:'│', lj:'├', rj:'┤' },
  double: { tl:'╔', tr:'╗', bl:'╚', br:'╝', h:'═', v:'║', lj:'╠', rj:'╣' },
};

const ROW_STYLE = {
  whiteSpace: 'pre', fontFamily: 'var(--font-mono)',
  lineHeight: 1.55, fontVariantLigatures: 'none', fontFeatureSettings: '"calt" 0',
};

/**
 * TuiPanel — a titled box-drawing frame, the universal container of this design
 * system. Draws on a monospace character grid: the title is inset into the top
 * rule, content is padded one cell in, and corners are box glyphs (radius is
 * always 0 — a terminal has no rounded pixels).
 *
 * Border color encodes focus: dim by default, Cloudflare orange when `focus`,
 * and `double` switches to ╔═╗ for blocking modals.
 */
export function TuiPanel({
  title, right, rows = [], width = 64, focus = false, double = false,
  minRows = 0, fontSize = 15, style, children,
}) {
  const S = double ? SETS.double : SETS.single;
  const bc = focus ? 'var(--orange)' : (double ? 'var(--orange-dim)' : 'var(--line)');
  const rowStyle = { ...ROW_STYLE, fontSize };

  const body = rows.slice();
  while (body.length < minRows) body.push('');

  const pad = (s, w) => {
    s = s == null ? '' : String(s);
    return s.length > w ? s.slice(0, w) : s + ' '.repeat(w - s.length);
  };

  const lead = S.tl + S.h + (title ? ' ' : '');
  const rightStr = right ? ` ${right} ` : '';
  const dashes = Math.max(0,
    width - lead.length - (title ? title.length + 1 : 0) - rightStr.length - 1);

  return (
    <div style={{ background:'var(--bg-base)', color:'var(--fg)', display:'inline-block', ...style }}>
      <div style={rowStyle}>
        <span style={{ color: bc }}>{lead}</span>
        {title && <span style={{ color: focus ? 'var(--fg-bright)' : 'var(--fg-muted)', fontWeight:500 }}>{title}</span>}
        <span style={{ color: bc }}>{(title ? ' ' : '') + S.h.repeat(dashes)}</span>
        {right && <span style={{ color:'var(--fg-faint)' }}>{rightStr}</span>}
        <span style={{ color: bc }}>{S.tr}</span>
      </div>
      {body.map((r, i) => (
        <div key={i} style={rowStyle}>
          <span style={{ color: bc }}>{S.v}</span>
          <span>{' ' + pad(r, width - 3)}</span>
          <span style={{ color: bc }}>{S.v}</span>
        </div>
      ))}
      {children}
      <div style={rowStyle}>
        <span style={{ color: bc }}>{S.bl + S.h.repeat(width - 2) + S.br}</span>
      </div>
    </div>
  );
}
