/* app.jsx — OpenAdmin: state, keyboard routing, mouse affordances. */

function App(){
  const [hosts, setHosts]   = React.useState(HOSTS);
  const [cursor, setCursor] = React.useState(0);
  const [marks, setMarks]   = React.useState(new Set());
  const [hovered, setHover] = React.useState(-1);
  const [screen, setScreen] = React.useState('hosts');
  const [modal, setModal]   = React.useState(null);       // null|add|edit|delete|key|help
  const [form, setForm]     = React.useState(null);
  const [ffocus, setFfocus] = React.useState('name');
  const [tabs, setTabs]     = React.useState([]);
  const [activeTab, setTab] = React.useState(null);
  const [paneFocus, setPane]= React.useState(0);
  const [msgs, setMsgs]     = React.useState(CHAT);
  const [draft, setDraft]   = React.useState('');
  const [busy, setBusy]     = React.useState(false);
  const [msg, setMsg]       = React.useState({ text:'Alt+1/2/3 switch screens · Insert marks hosts · F1 help', type:'' });
  const tmr = React.useRef();

  const flash = (text, type='ok') => {
    setMsg({ text, type });
    clearTimeout(tmr.current);
    tmr.current = setTimeout(()=>setMsg({ text:'Ready.', type:'' }), 3400);
  };

  const host = hosts[cursor] || null;
  const markedHosts = hosts.filter(h => marks.has(h.id));
  const targets = markedHosts.length ? markedHosts : (host ? [host] : []);
  const clamp = i => setCursor(Math.max(0, Math.min(i, hosts.length-1)));

  /* ---------------- host actions ---------------- */
  const openAdd = () => {
    setForm({ name:'', type:'SSH', addr:'', port:22, mount:'', mountAuto:true,
              login:'', pass:'', key:false });
    setFfocus('name'); setModal('add');
  };
  const openEdit = () => {
    if (!host) return;
    setForm({ ...host, mountAuto: host.mountAuto !== false });
    setFfocus('name'); setModal('edit');
  };
  /* name drives the mount point until the user edits the mount field */
  const setFormName = (name) => setForm(f => ({
    ...f, name, mount: f.mountAuto ? mountFor(name) : f.mount,
  }));
  const setFormMount = (mount) => setForm(f => ({
    ...f, mount, mountAuto: mount.trim()==='' ? true : false,
  }));
  const cycleType = (d) => setForm(f => {
    const i = HOST_TYPES.indexOf(f.type);
    const t = HOST_TYPES[(i+d+HOST_TYPES.length)%HOST_TYPES.length];
    return { ...f, type:t, port: DEFAULT_PORT[t] };
  });
  const saveForm = () => {
    if (!form.name.trim() || !form.addr.trim()){ flash('Host name and address are required.','err'); return; }
    const rec = { ...form, id: form.id || nid('h'),
      name: form.name.trim(), addr: form.addr.trim(),
      port: parseInt(form.port,10) || DEFAULT_PORT[form.type],
      mount: (form.mount||'').trim() || mountFor(form.name),
      mounted: form.mounted||false, proxy: form.proxy||false };
    if (modal==='edit'){ setHosts(hs=>hs.map(h=>h.id===rec.id?rec:h)); flash(`Saved ${rec.name}.`); }
    else { setHosts(hs=>[...hs, rec]); setCursor(hosts.length); flash(`Added ${rec.name}.`); }
    setModal(null);
  };
  const doDelete = () => {
    const ids = new Set(targets.map(t=>t.id));
    setHosts(hs=>hs.filter(h=>!ids.has(h.id)));
    setMarks(new Set());
    flash(`Deleted ${ids.size} host${ids.size>1?'s':''}.`);
    setModal(null); clamp(cursor);
  };
  const toggleMount = () => {
    if (!targets.length) return;
    const ids = new Set(targets.map(t=>t.id));
    const anyUnmounted = targets.some(t=>!t.mounted);
    setHosts(hs=>hs.map(h=>ids.has(h.id)?{...h, mounted:anyUnmounted}:h));
    flash(`${anyUnmounted?'Mounted':'Unmounted'} ${ids.size} host${ids.size>1?'s':''}.`);
  };
  const toggleProxy = () => {
    if (!host) return;
    setHosts(hs=>hs.map(h=>h.id===host.id?{...h, proxy:!h.proxy}:h));
    flash(host.proxy ? `${host.name} no longer used as proxy.` : `Routing through ${host.name}.`);
  };
  const genKey = (h) => {
    const t = h || host; if (!t) return;
    setHosts(hs=>hs.map(x=>x.id===t.id?{...x, key:true}:x));
    setModal('key');
  };
  const toggleMark = () => {
    if (!host) return;
    setMarks(m => { const n = new Set(m); n.has(host.id) ? n.delete(host.id) : n.add(host.id); return n; });
    clamp(cursor+1);
  };

  /* ---------------- shells ---------------- */
  const openShell = () => {
    const sel = markedHosts.length ? markedHosts : (host ? [host] : []);
    if (!sel.length) return;
    const group = sel.length > 1;
    const names = sel.map(h=>h.name);
    const lines = {};
    names.forEach(n => { lines[n] = SHELL_OUTPUT[n] || GENERIC_OUTPUT(n); });
    const tab = { id:nid('t'), title: group ? `Group: ${names[0]}` : names[0],
                  hosts:names, group, lines };
    setTabs(ts=>[...ts, tab]); setTab(tab.id); setPane(0);
    setScreen('shells'); setMarks(new Set());
    flash(group ? `Opened ${names.length} shells in one tab.` : `Shell open on ${names[0]}.`);
  };
  const closeTab = (id) => {
    setTabs(ts => { const n = ts.filter(t=>t.id!==id);
      if (activeTab===id) setTab(n.length ? n[n.length-1].id : null);
      return n; });
  };

  /* ---------------- chat ---------------- */
  const sendChat = () => {
    if (!draft.trim()) return;
    const text = draft.trim();
    setMsgs(m=>[...m, { role:'user', text }]); setDraft(''); setBusy(true);
    setTimeout(()=>{
      setMsgs(m=>[...m,
        { role:'tool', name:'ssh', arg:'web-01 · scp web-02:/srv/api/.env /tmp/env.ref', status:'ok', out:['/tmp/env.ref  1.2 KB  100%'] },
        { role:'assistant', text:'Pulled the reference env from web-02 and diffed it — `DATABASE_URL` is the only missing key. Restoring it and restarting `api.service` now.' },
      ]);
      setBusy(false);
    }, 1400);
  };

  /* ---------------- keyboard ---------------- */
  React.useEffect(()=>{
    const onKey = (e) => {
      const k = e.key;
      if (e.altKey && ['1','2','3'].includes(k)){ e.preventDefault(); setScreen(SCREENS[+k-1].id); return; }
      if (k==='F9'){ e.preventDefault(); setScreen(s=>SCREENS[(SCREENS.findIndex(x=>x.id===s)+1)%3].id); return; }
      if (k==='F1'){ e.preventDefault(); setModal(m=>m==='help'?null:'help'); return; }
      if (k==='F10'){ e.preventDefault(); flash('Quit — close the tab. (demo)','warn'); return; }

      if (modal==='help' || modal==='key'){ if(k==='Escape'){e.preventDefault();setModal(null);} return; }
      if (modal==='delete'){
        if (k==='Escape'||k==='n'||k==='N'){ e.preventDefault(); setModal(null); }
        else if (k==='Enter'||k==='y'||k==='Y'){ e.preventDefault(); doDelete(); }
        return;
      }
      if (modal==='add'||modal==='edit'){
        if (k==='Escape'){ e.preventDefault(); setModal(null); return; }
        if (k==='Enter'){ e.preventDefault(); saveForm(); return; }
        if (k==='Tab'){ e.preventDefault();
          const i=FORM_FIELDS.indexOf(ffocus);
          setFfocus(FORM_FIELDS[(i+(e.shiftKey?-1:1)+FORM_FIELDS.length)%FORM_FIELDS.length]); return; }
        if (ffocus==='type'){ if(k==='ArrowRight'){e.preventDefault();cycleType(1);} else if(k==='ArrowLeft'){e.preventDefault();cycleType(-1);} return; }
        if (k==='Backspace'){ e.preventDefault();
          if (ffocus==='name') setFormName(String(form.name).slice(0,-1));
          else if (ffocus==='mount') setFormMount(String(form.mount).slice(0,-1));
          else setForm(f=>({...f,[ffocus]:String(f[ffocus]??'').slice(0,-1)}));
          return; }
        if (k.length===1 && !e.metaKey && !e.ctrlKey){
          if (ffocus==='port' && !/\d/.test(k)) return;
          e.preventDefault();
          if (ffocus==='name') setFormName(String(form.name)+k);
          else if (ffocus==='mount') setFormMount(String(form.mount)+k);
          else setForm(f=>({...f,[ffocus]:String(f[ffocus]??'')+k}));
        }
        return;
      }

      if (screen==='hosts'){
        if (k==='ArrowDown'){ e.preventDefault(); clamp(cursor+1); }
        else if (k==='ArrowUp'){ e.preventDefault(); clamp(cursor-1); }
        else if (k==='Home'){ e.preventDefault(); clamp(0); }
        else if (k==='End'){ e.preventDefault(); clamp(hosts.length-1); }
        else if (k==='Insert'||k===' '){ e.preventDefault(); toggleMark(); }
        else if (k==='*'){ e.preventDefault(); setMarks(m=>new Set(hosts.filter(h=>!m.has(h.id)).map(h=>h.id))); }
        else if ((e.ctrlKey||e.metaKey) && k==='a'){ e.preventDefault(); setMarks(new Set(hosts.map(h=>h.id))); }
        else if (k==='Escape'){ e.preventDefault(); setMarks(new Set()); }
        else if (k==='Enter'||k==='F3'){ e.preventDefault(); openEdit(); }
        else if (k==='F2'){ e.preventDefault(); openAdd(); }
        else if (k==='F4'){ e.preventDefault(); toggleMount(); }
        else if (k==='F5'){ e.preventDefault(); openShell(); }
        else if (k==='F6'){ e.preventDefault(); toggleProxy(); }
        else if (k==='F7'){ e.preventDefault(); genKey(); }
        else if (k==='F8'||k==='Delete'){ e.preventDefault(); if(targets.length) setModal('delete'); }
        return;
      }
      if (screen==='shells'){
        if (k==='Tab' && tabs.length){ e.preventDefault();
          const i = tabs.findIndex(t=>t.id===activeTab);
          setTab(tabs[(i+(e.shiftKey?-1:1)+tabs.length)%tabs.length].id); }
        else if (k==='F4' && activeTab){ e.preventDefault(); closeTab(activeTab); flash('Shell closed.'); }
        return;
      }
      if (screen==='chat'){
        if (k==='Enter' && !e.shiftKey){ e.preventDefault(); sendChat(); return; }
        if (k==='Backspace'){ e.preventDefault(); setDraft(d=>d.slice(0,-1)); return; }
        if (k.length===1 && !e.metaKey && !e.ctrlKey){ e.preventDefault(); setDraft(d=>d+k); }
        return;
      }
    };
    window.addEventListener('keydown', onKey);
    return ()=>window.removeEventListener('keydown', onKey);
  });

  /* ---------------- chrome ---------------- */
  const nm = marks.size;
  const fkeys = () => {
    if (modal==='add'||modal==='edit') return [
      { key:'Esc', label:'Cancel', onClick:()=>setModal(null) },
      { key:'↹', label:'Next field' },
      { key:'↵', label: modal==='edit'?'Save':'Add', onClick:saveForm, active:true },
    ];
    if (modal==='delete') return [
      { key:'N', label:'Cancel', onClick:()=>setModal(null) },
      { key:'Y', label:'Delete', danger:true, onClick:doDelete, active:true },
    ];
    if (modal) return [{ key:'Esc', label:'Close', onClick:()=>setModal(null), active:true }];
    if (screen==='hosts') return [
      { key:'F1', label:'Help', onClick:()=>setModal('help') },
      { key:'F2', label:'Add', onClick:openAdd },
      { key:'F3', label:'Edit', onClick:openEdit },
      { key:'F4', label: targets.some(t=>!t.mounted)?'Mount':'Unmount', onClick:toggleMount },
      { key:'F5', label: nm>1?`Shell ×${nm}`:'Shell', onClick:openShell, active:nm>1 },
      { key:'F6', label:'Proxy', onClick:toggleProxy },
      { key:'F7', label:'GenKey', onClick:()=>genKey() },
      { key:'F8', label:'Delete', danger:true, onClick:()=>targets.length&&setModal('delete') },
      { key:'Ins', label: nm?`Marked ${nm}`:'Mark', onClick:toggleMark, active:!!nm },
      { key:'F10', label:'Quit', onClick:()=>flash('Quit — close the tab. (demo)','warn') },
    ];
    if (screen==='shells') return [
      { key:'F1', label:'Help', onClick:()=>setModal('help') },
      { key:'↹', label:'Next tab' },
      { key:'F4', label:'Close tab', onClick:()=>activeTab&&closeTab(activeTab), disabled:!activeTab },
      { key:'F5', label:'New shell', onClick:()=>setScreen('hosts') },
      { key:'F9', label:'Screen', onClick:()=>setScreen('chat') },
      { key:'F10', label:'Quit', onClick:()=>flash('Quit — close the tab. (demo)','warn') },
    ];
    return [
      { key:'F1', label:'Help', onClick:()=>setModal('help') },
      { key:'↵', label:'Send', onClick:sendChat, active:true },
      { key:'^R', label:'Run command' },
      { key:'@', label:'Add context' },
      { key:'F9', label:'Screen', onClick:()=>setScreen('hosts') },
      { key:'F10', label:'Quit', onClick:()=>flash('Quit — close the tab. (demo)','warn') },
    ];
  };

  /* hover-driven hint — a real use of terminal mouse reporting */
  const hoverHost = hovered>=0 ? hosts[hovered] : null;
  const hint = screen==='hosts' && hoverHost
    ? <>{hoverHost.login}@{hoverHost.addr}:{hoverHost.port} · {hoverHost.mounted?'mounted at '+hoverHost.mount:'not mounted'} · double-click for a shell</>
    : null;
  const left = screen==='hosts'
    ? <>{hosts.length} hosts · {hosts.filter(h=>h.mounted).length} mounted{nm ? <> · <b className="oa-marked">{nm} marked</b></> : null}</>
    : screen==='shells'
      ? <>{tabs.length} shell{tabs.length===1?'':'s'} open</>
      : <>agent · claude-sonnet-4.5</>;

  return (
    <Screen>
      <Header screen={screen} onScreen={setScreen}
        counts={{ hosts:hosts.length, shells:tabs.length, chat:null }} />
      <div className={modal ? 'oa-dim' : ''}>
        {screen==='hosts' &&
          <HostsScreen hosts={hosts} cursor={cursor} marks={marks} hovered={hovered}
            onCursor={clamp} onHover={setHover} onToggleMark={toggleMark}
            onGenKey={genKey} onOpen={(i)=>{ clamp(i); setTimeout(openShell,0); }} />}
        {screen==='shells' &&
          <ShellsScreen tabs={tabs} active={activeTab} paneFocus={paneFocus}
            onSelect={setTab} onClose={closeTab} onPaneFocus={setPane} />}
        {screen==='chat' &&
          <ChatScreen msgs={msgs} draft={draft} busy={busy} focused={true}
            model="claude-sonnet-4.5" />}
      </div>
      <StatusBar left={left} hint={hint} message={msg.text} messageType={msg.type} busy={false} />
      <FunctionBar items={fkeys()} />

      {(modal==='add'||modal==='edit') &&
        <HostDialog mode={modal} form={form} focus={ffocus} onFocus={setFfocus}
          onCycleType={cycleType} onSave={saveForm} onCancel={()=>setModal(null)}
          onGenKey={()=>{ setForm(f=>({...f,key:true})); setModal('key'); }} />}
      {modal==='delete' && targets.length>0 &&
        <DeleteDialog targets={targets} onConfirm={doDelete} onCancel={()=>setModal(null)} />}
      {modal==='key' && (host||form) &&
        <KeyDialog host={form && (modal==='key'&&form.name) ? form : host}
          onCancel={()=>setModal(null)}
          onCopy={()=>{ setModal(null); flash('Public key copied to clipboard.'); }} />}
      {modal==='help' && <HelpDialog onCancel={()=>setModal(null)} />}
    </Screen>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
