/* Chrome.jsx — StatusBar (context + transient message) and FunctionBar
   (context-sensitive F-key actions). Both are full-width bands. */

function StatusBar({ zone, left, message, messageType, spinner }){
  const mc = { ok:'tui-ok', err:'tui-err', warn:'tui-warn', info:'tui-info' }[messageType] || '';
  return (
    <div className="tui-statusbar">
      <span className="sb-zone">zone <b>{zone}</b></span>
      <span className="sb-left">{left}</span>
      <span className="sb-legend"><span className="sb-prx">▲</span> proxied&nbsp;&nbsp;<span className="sb-dns">○</span> dns-only</span>
      <span className={'sb-msg ' + mc}>
        {spinner && <Spinner />}
        {message}
      </span>
    </div>
  );
}

function Spinner(){
  const frames = '⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏';
  const [i, setI] = React.useState(0);
  React.useEffect(() => {
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (reduce) return;
    const id = setInterval(() => setI(x => (x+1) % frames.length), 80);
    return () => clearInterval(id);
  }, []);
  return <span className="sb-spin">{frames[i]}&nbsp;</span>;
}

function FunctionBar({ items }){
  return (
    <div className="tui-fnbar">
      {items.map((it, i) => (
        <span key={i} className={'fk' + (it.active ? ' fk-active' : '')}
              onClick={it.onClick}>
          <span className="fk-cap">{it.key}</span>
          <span className={'fk-lab' + (it.danger ? ' fk-danger' : '')}>{it.label}</span>
        </span>
      ))}
    </div>
  );
}

Object.assign(window, { StatusBar, FunctionBar, Spinner });
