/* AppChrome.jsx — OpenAdmin header (brand block + screen tabs), status bar, F-key bar.
   Screen tabs and F-keys are mouse-hoverable and clickable. */

const SCREENS = [
  { id:'hosts',  label:'Hosts'  },
  { id:'shells', label:'Shells' },
  { id:'chat',   label:'Chat'   },
];

function Header({ screen, onScreen, counts }){
  return (
    <div className="oa-header">
      <div className="oa-hrow">
        <div className="oa-left">
          <div className="oa-mark"></div>
          <div className="oa-word">
            <div className="oa-name">OpenAdmin</div>
            <div className="oa-tag">remote hosts · shells · agent</div>
          </div>
        </div>
        <div className="oa-tabs">
          {SCREENS.map((s, i) => (
            <span key={s.id}
              className={'oa-tab' + (screen===s.id ? ' oa-tab-on' : '')}
              onClick={()=>onScreen(s.id)}
              title={`Alt+${i+1} · switch to ${s.label}`}>
              <span className="oa-tabnum">{i+1}</span>
              <span className="oa-tablab">{s.label}</span>
              {counts[s.id] != null && <span className="oa-tabct">{counts[s.id]}</span>}
            </span>
          ))}
        </div>
      </div>
      <Row w={COLS} segs={[{ fill:true, ch:'─', color:C.line }]} />
    </div>
  );
}

function StatusBar({ left, hint, message, messageType, busy }){
  const mc = { ok:'oa-ok', err:'oa-err', warn:'oa-warn', info:'oa-info' }[messageType] || '';
  return (
    <div className="oa-statusbar">
      <span className="oa-sb-left">{left}</span>
      {hint && <span className="oa-sb-hint">{hint}</span>}
      <span className={'oa-sb-msg ' + mc}>
        {busy && <><Spinner />&nbsp;</>}{message}
      </span>
    </div>
  );
}

function FunctionBar({ items }){
  return (
    <div className="oa-fnbar">
      {items.map((it, i) => (
        <span key={i}
          className={'oa-fk' + (it.active?' oa-fk-on':'') + (it.disabled?' oa-fk-off':'')}
          onClick={it.disabled ? null : it.onClick} title={it.title||''}>
          <span className="oa-fk-cap">{it.key}</span>
          <span className={'oa-fk-lab' + (it.danger?' oa-fk-danger':'')}>{it.label}</span>
        </span>
      ))}
    </div>
  );
}

Object.assign(window, { Header, StatusBar, FunctionBar, SCREENS });
