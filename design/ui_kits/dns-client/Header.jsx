/* Header.jsx — top band: solid brand block + wordmark on the left, zone & account
   on the right, closed by a full-width rule. A TUI can't draw logo art, so the
   mark is an honest solid orange rectangle sized to the two-line wordmark. */

function Header({ zone, count, proxiedCount }){
  return (
    <div className="tui-header">
      <div className="hdr-row">
        <div className="hdr-left">
          <div className="hdr-mark"></div>
          <div className="hdr-word">
            <div className="hdr-name">cfdns</div>
            <div className="hdr-tag">Cloudflare DNS Console</div>
          </div>
        </div>
        <div className="hdr-right">
          <div className="hdr-zone">zone&nbsp;<b>{zone}</b>&nbsp;<span className="hdr-car">▾</span></div>
          <div className="hdr-acct">ops@{zone} · Free plan</div>
          <div className="hdr-count">{count} records · <span className="hdr-prx">{proxiedCount} proxied</span></div>
        </div>
      </div>
      <Row w={COLS} segs={[{ fill:true, ch:'─', color:C.line }]} />
    </div>
  );
}

window.Header = Header;
