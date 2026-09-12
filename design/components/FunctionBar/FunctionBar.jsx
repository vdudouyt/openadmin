/**
 * FunctionBar — the always-visible bottom action strip. Every action in a TUI has
 * an F-key; this renders the key caps and their labels, full width.
 *
 * Caps are Cloudflare orange, the active item inverts to a solid fill, and
 * destructive actions tint their label red. Items are clickable for mouse use.
 */
export function FunctionBar({ items = [], fontSize = 13, style }) {
  return (
    <div style={{
      display:'flex', background:'var(--statusbar-bg)', fontSize,
      fontFamily:'var(--font-mono)', overflow:'hidden', whiteSpace:'nowrap',
      borderTop:'1px solid var(--bg-base)', ...style,
    }}>
      {items.map((it, i) => (
        <span key={i}
          onClick={it.disabled ? undefined : it.onClick}
          title={it.title}
          style={{
            display:'flex', alignItems:'center', padding:'4px 0',
            cursor: it.disabled ? 'default' : 'pointer',
            opacity: it.disabled ? 0.4 : 1, userSelect:'none',
          }}>
          <span style={{
            padding:'0 1px 0 1.2ch', fontWeight:500,
            color: it.active ? 'var(--orange-ink)' : 'var(--orange-bright)',
            background: it.active ? 'var(--orange)' : 'transparent',
          }}>{it.keyLabel}</span>
          <span style={{
            padding:'0 1.2ch 0 .6ch',
            color: it.danger ? 'var(--red)' : 'var(--fg)',
          }}>{it.label}</span>
        </span>
      ))}
    </div>
  );
}
