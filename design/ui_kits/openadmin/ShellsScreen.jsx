/* ShellsScreen.jsx — screen 2: open shells as tabs.
   A tab opened from a single host is named after that host. A tab opened with
   several hosts marked is named "Group: <first host>" and stacks one pane per
   host vertically, each with its own titled frame and a live prompt. */

const LINE_COLOR = { p:C.green, c:C.bright, d:C.fg, ok:C.green, warn:C.yellow, err:C.red };
const PANE_TOTAL = 22;   // rows available to the pane stack

function TabBar({ tabs, active, onSelect, onClose }){
  if (!tabs.length) return null;
  return (
    <div className="oa-tabbar">
      {tabs.map(t => (
        <span key={t.id} className={'oa-shtab' + (t.id===active?' oa-shtab-on':'')}
              onClick={()=>onSelect(t.id)} title={t.hosts.join(', ')}>
          <span className="oa-shtab-dot" style={{color: t.group ? 'var(--yellow-mark)' : 'var(--green)'}}>●</span>
          <span className="oa-shtab-lab">{t.title}</span>
          {t.group && <span className="oa-shtab-ct">{t.hosts.length}</span>}
          <span className="oa-shtab-x" title="close shell"
                onClick={(e)=>{ e.stopPropagation(); onClose(t.id); }}>×</span>
        </span>
      ))}
    </div>
  );
}

/* one terminal pane: titled frame + scrollback + prompt */
function ShellPane({ host, lines, rows, focused, onFocus }){
  const body = [];
  // flatten [kind,text] pairs into rows, merging a prompt with its command
  let i = 0;
  while (i < lines.length){
    const [k, txt] = lines[i];
    if (k==='p' && lines[i+1] && lines[i+1][0]==='c'){
      body.push([{ t:txt, color:LINE_COLOR.p }, { t:lines[i+1][1], color:LINE_COLOR.c }]);
      i += 2;
    } else if (k==='p'){
      body.push([{ t:txt, color:LINE_COLOR.p }, { t:'█', color:C.orange, cls: focused?'tui-cursor':'' }]);
      i++;
    } else {
      body.push([{ t:txt, color:LINE_COLOR[k]||C.fg }]);
      i++;
    }
  }
  const shown = body.slice(-(rows));
  while (shown.length < rows) shown.push([]);
  return (
    <div onClick={onFocus} className="oa-pane">
      <TopBorder w={COLS} title={host} focus={focused}
        right={focused ? 'active' : 'click to focus'} />
      {shown.map((inner,i)=>(
        <BodyRow key={i} w={COLS} inner={inner} focus={focused} />
      ))}
    </div>
  );
}

function EmptyShells({ onGo }){
  const rows = [
    [], [],
    [{ t:'  No open shells.', color:C.muted }],
    [],
    [{ t:'  Go to ', color:C.faint }, { t:'Hosts', color:C.orange },
     { t:' and press ', color:C.faint }, { t:'F5', color:C.orangeB },
     { t:' on a host to open one.', color:C.faint }],
    [],
    [{ t:'  Mark several hosts with ', color:C.faint }, { t:'Insert', color:C.mark },
     { t:' first and F5 opens a single', color:C.faint }],
    [{ t:'  grouped tab with one stacked pane per host.', color:C.faint }],
  ];
  return (
    <>
      <TopBorder w={COLS} title="Shells" focus={true} />
      {rows.map((r,i)=><BodyRow key={i} w={COLS} inner={r} focus={true} />)}
      {Array.from({length:14}).map((_,i)=><BodyRow key={'p'+i} w={COLS} inner={[]} focus={true} />)}
      <BotBorder w={COLS} focus={true} />
    </>
  );
}

function ShellsScreen({ tabs, active, paneFocus, onSelect, onClose, onPaneFocus }){
  const tab = tabs.find(t => t.id===active);
  if (!tab) return <EmptyShells />;
  const n = tab.hosts.length;
  // split the available rows between stacked panes (2 rows of chrome per pane)
  const per = Math.max(3, Math.floor((PANE_TOTAL - n*1) / n) - 1);
  return (
    <div className="oa-shells">
      <TabBar tabs={tabs} active={active} onSelect={onSelect} onClose={onClose} />
      {tab.hosts.map((h, i) => (
        <ShellPane key={h} host={h} lines={tab.lines[h] || []} rows={per}
          focused={ n===1 || paneFocus===i } onFocus={()=>onPaneFocus(i)} />
      ))}
      <BotBorder w={COLS} focus={true} />
    </div>
  );
}

Object.assign(window, { ShellsScreen, TabBar, ShellPane, EmptyShells });
