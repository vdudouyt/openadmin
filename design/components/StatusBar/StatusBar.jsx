/**
 * StatusBar — the single context row that sits directly above the FunctionBar.
 * Left: where you are. Middle: an optional hover/context hint (terminals report
 * mouse motion, so hovering a row can explain it here). Right: transient status
 * — green for success, red for errors, yellow for warnings.
 */
const TONE = {
  ok:'var(--green)', err:'var(--red)', warn:'var(--yellow)',
  info:'var(--blue)', '':'var(--fg-muted)',
};

export function StatusBar({ left, hint, message, tone = '', fontSize = 13, style }) {
  return (
    <div style={{
      display:'flex', alignItems:'center', gap:'2ch',
      background:'var(--statusbar-bg)', padding:'3px 1ch', fontSize,
      fontFamily:'var(--font-mono)', color:'var(--fg-muted)',
      overflow:'hidden', whiteSpace:'nowrap', ...style,
    }}>
      <span>{left}</span>
      {hint && <span style={{ color:'var(--fg-faint)' }}>{hint}</span>}
      <span style={{ marginLeft:'auto', textAlign:'right', color: TONE[tone] || TONE[''] }}>
        {message}
      </span>
    </div>
  );
}
