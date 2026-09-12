/* app.jsx — cfdns application: state, global keyboard handling, and wiring. */

const TTL_CYCLE = [1, 60, 300, 1800, 3600, 86400];

function useMessage(){
  const [msg, setMsg] = React.useState({ text:'↑↓ select · Enter edit · F2 add · F5 bulk · F1 help', type:'' });
  const t = React.useRef();
  const flash = (text, type='ok') => {
    setMsg({ text, type });
    clearTimeout(t.current);
    t.current = setTimeout(() => setMsg({ text:'Ready.', type:'' }), 3200);
  };
  return [msg, flash];
}

function visibleFormFields(meta){
  const v = ['type','name','content'];
  if (meta.priority) v.push('priority');
  v.push('ttl');
  if (meta.proxiable) v.push('proxy');
  return v;
}
function visibleBulkFields(meta){
  const v = ['type'];
  if (meta.proxiable) v.push('proxy');
  v.push('first','ips');
  return v;
}

function App(){
  const [records, setRecords] = React.useState(SAMPLE_RECORDS);
  const [selected, setSelected] = React.useState(0);
  const [mode, setMode] = React.useState('list');     // list|add|edit|delete|bulk|help|filter
  const [form, setForm] = React.useState(null);
  const [formFocus, setFormFocus] = React.useState('name');
  const [bulk, setBulk] = React.useState(null);
  const [filter, setFilter] = React.useState('');
  const [msg, flash] = useMessage();

  const filtered = React.useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return records;
    return records.filter(r =>
      (r.type+' '+r.name+' '+r.content).toLowerCase().includes(q));
  }, [records, filter]);

  const sel = filtered[Math.min(selected, filtered.length-1)] || null;
  const proxiedCount = records.filter(r => r.proxied).length;

  const clampSel = (i) => setSelected(Math.max(0, Math.min(i, filtered.length-1)));

  /* ---------------- actions ---------------- */
  const openAdd = () => {
    setForm({ type:'A', name:'', content:'', priority:10, ttl:1, proxied:true });
    setFormFocus('name'); setMode('add');
  };
  const openEdit = () => {
    if (!sel) return;
    setForm({ ...sel, priority: sel.priority ?? 10 });
    setFormFocus('name'); setMode('edit');
  };
  const cycleType = (dir) => {
    setForm(f => {
      const idx = RECORD_TYPES.findIndex(t => t.type===f.type);
      const nt = RECORD_TYPES[(idx + dir + RECORD_TYPES.length) % RECORD_TYPES.length];
      return { ...f, type:nt.type, proxied: nt.proxiable ? f.proxied : false };
    });
  };
  const cycleTTL = (dir) => setForm(f => {
    const i = TTL_CYCLE.indexOf(f.ttl); const ni = (i + dir + TTL_CYCLE.length) % TTL_CYCLE.length;
    return { ...f, ttl: TTL_CYCLE[ni] };
  });
  const toggleFormProxy = () => setForm(f => ({ ...f, proxied: !f.proxied, ttl: !f.proxied ? 1 : f.ttl }));
  const saveForm = () => {
    if (!form.name.trim() || !form.content.trim()){ flash('Name and content are required.', 'err'); return; }
    const meta = TYPE_META[form.type];
    const rec = { id: form.id || rid(), type:form.type, name:form.name.trim(), content:form.content.trim(),
      ttl: meta.proxiable && form.proxied ? 1 : form.ttl,
      proxied: meta.proxiable ? form.proxied : false };
    if (meta.priority) rec.priority = parseInt(form.priority,10) || 0;
    if (mode==='edit'){
      setRecords(rs => rs.map(r => r.id===rec.id ? rec : r));
      flash(`Saved ${rec.type} ${rec.name}.`, 'ok');
    } else {
      setRecords(rs => [...rs, rec]);
      flash(`Added ${rec.type} ${rec.name}.`, 'ok');
    }
    setMode('list');
  };
  const toggleProxy = () => {
    if (!sel) return;
    const meta = TYPE_META[sel.type];
    if (!meta.proxiable){ flash(`${sel.type} records cannot be proxied.`, 'warn'); return; }
    setRecords(rs => rs.map(r => r.id===sel.id ? { ...r, proxied:!r.proxied, ttl: !r.proxied?1:r.ttl } : r));
    flash(`${sel.name}: ${sel.proxied?'DNS only':'proxied'}.`, 'ok');
  };
  const confirmDelete = () => {
    if (!sel) return;
    setRecords(rs => rs.filter(r => r.id!==sel.id));
    flash(`Deleted ${sel.type} ${sel.name}.`, 'ok');
    setMode('list'); clampSel(selected);
  };
  const openBulk = () => {
    setBulk({ step:1, type:'A', proxied:true, first:'s1000.'+ZONE, ips:'', focus:'first' });
    setMode('bulk');
  };
  const bulkCycleType = (dir) => setBulk(b => {
    const idx = RECORD_TYPES.findIndex(t => t.type===b.type);
    const nt = RECORD_TYPES[(idx+dir+RECORD_TYPES.length)%RECORD_TYPES.length];
    return { ...b, type:nt.type, proxied: nt.proxiable ? b.proxied : false };
  });
  const bulkConfirm = () => {
    const pairs = expand(bulk.first, bulk.ips);
    if (!pairs.length){ flash('Nothing to add.', 'warn'); return; }
    const meta = TYPE_META[bulk.type];
    const recs = pairs.map(p => ({ id:rid(), type:bulk.type, name:p.name, content:p.content,
      ttl: meta.proxiable && bulk.proxied ? 1 : 1, proxied: meta.proxiable ? bulk.proxied : false }));
    setRecords(rs => [...rs, ...recs]);
    flash(`Added ${recs.length} ${bulk.type} records.`, 'ok');
    setMode('list');
  };

  /* ---------------- keyboard ---------------- */
  React.useEffect(() => {
    const onKey = (e) => {
      const k = e.key;
      // global
      if (k==='F1'){ e.preventDefault(); setMode(m => m==='help'?'list':'help'); return; }
      if (k==='F10'){ e.preventDefault(); flash('Quit — close the tab. (demo)', 'warn'); return; }

      if (mode==='list'){
        if (k==='ArrowDown'){ e.preventDefault(); clampSel(selected+1); }
        else if (k==='ArrowUp'){ e.preventDefault(); clampSel(selected-1); }
        else if (k==='Home'){ e.preventDefault(); clampSel(0); }
        else if (k==='End'){ e.preventDefault(); clampSel(filtered.length-1); }
        else if (k==='Enter'){ e.preventDefault(); openEdit(); }
        else if (k==='F2'){ e.preventDefault(); openAdd(); }
        else if (k==='F3'){ e.preventDefault(); openEdit(); }
        else if (k==='F4'){ e.preventDefault(); toggleProxy(); }
        else if (k==='F5'){ e.preventDefault(); openBulk(); }
        else if (k==='F6' || k==='/'){ e.preventDefault(); setMode('filter'); }
        else if (k==='F8' || k==='Delete'){ e.preventDefault(); if (sel) setMode('delete'); }
        return;
      }
      if (mode==='add' || mode==='edit'){
        const meta = TYPE_META[form.type];
        const vf = visibleFormFields(meta);
        if (k==='Escape'){ e.preventDefault(); setMode('list'); return; }
        if (k==='Enter'){ e.preventDefault(); saveForm(); return; }
        if (k==='Tab'){ e.preventDefault(); const i=vf.indexOf(formFocus); setFormFocus(vf[(i+(e.shiftKey?-1:1)+vf.length)%vf.length]); return; }
        if (k==='ArrowDown'){ e.preventDefault(); const i=vf.indexOf(formFocus); setFormFocus(vf[(i+1)%vf.length]); return; }
        if (k==='ArrowUp'){ e.preventDefault(); const i=vf.indexOf(formFocus); setFormFocus(vf[(i-1+vf.length)%vf.length]); return; }
        if (formFocus==='type'){ if(k==='ArrowRight'){e.preventDefault();cycleType(1);} else if(k==='ArrowLeft'){e.preventDefault();cycleType(-1);} return; }
        if (formFocus==='ttl'){ if(!(meta.proxiable&&form.proxied)){ if(k==='ArrowRight'){e.preventDefault();cycleTTL(1);} else if(k==='ArrowLeft'){e.preventDefault();cycleTTL(-1);} } return; }
        if (formFocus==='proxy'){ if(k==='ArrowRight'||k==='ArrowLeft'||k===' '){e.preventDefault();toggleFormProxy();} return; }
        // text fields: name, content, priority
        if (k==='Backspace'){ e.preventDefault(); setForm(f=>({ ...f, [formFocus]: String(f[formFocus]??'').slice(0,-1) })); return; }
        if (k.length===1 && !e.metaKey && !e.ctrlKey){
          if (formFocus==='priority' && !/\d/.test(k)) return;
          e.preventDefault(); setForm(f=>({ ...f, [formFocus]: String(f[formFocus]??'')+k }));
        }
        return;
      }
      if (mode==='delete'){
        if (k==='Escape'||k==='n'||k==='N'){ e.preventDefault(); setMode('list'); }
        else if (k==='Enter'||k==='y'||k==='Y'){ e.preventDefault(); confirmDelete(); }
        return;
      }
      if (mode==='help'){ if (k==='Escape'||k==='F1'){ e.preventDefault(); setMode('list'); } return; }
      if (mode==='filter'){
        if (k==='Escape'||k==='Enter'){ e.preventDefault(); setMode('list'); clampSel(0); return; }
        if (k==='Backspace'){ e.preventDefault(); setFilter(s=>s.slice(0,-1)); return; }
        if (k.length===1 && !e.metaKey && !e.ctrlKey){ e.preventDefault(); setFilter(s=>s+k); }
        return;
      }
      if (mode==='bulk'){
        const meta = TYPE_META[bulk.type];
        const vf = visibleBulkFields(meta);
        if (k==='Escape'){ e.preventDefault(); if(bulk.step===2) setBulk(b=>({...b,step:1})); else setMode('list'); return; }
        if (bulk.step===1){
          if (k==='Tab'){ e.preventDefault(); const i=vf.indexOf(bulk.focus); setBulk(b=>({...b,focus:vf[(i+(e.shiftKey?-1:1)+vf.length)%vf.length]})); return; }
          if (k==='Enter' && bulk.focus!=='ips'){ e.preventDefault(); setBulk(b=>({...b,step:2})); return; }
          if (bulk.focus==='type'){ if(k==='ArrowRight'){e.preventDefault();bulkCycleType(1);} else if(k==='ArrowLeft'){e.preventDefault();bulkCycleType(-1);} return; }
          if (bulk.focus==='proxy'){ if(k==='ArrowRight'||k==='ArrowLeft'||k===' '){e.preventDefault();setBulk(b=>({...b,proxied:!b.proxied}));} return; }
          if (bulk.focus==='ips'){
            if (k==='Enter'){ e.preventDefault(); setBulk(b=>({...b,ips:b.ips+'\n'})); return; }
            if (k==='Backspace'){ e.preventDefault(); setBulk(b=>({...b,ips:b.ips.slice(0,-1)})); return; }
            if (k.length===1 && !e.metaKey && !e.ctrlKey){ e.preventDefault(); setBulk(b=>({...b,ips:b.ips+k})); }
            return;
          }
          // first (seed) field
          if (k==='Backspace'){ e.preventDefault(); setBulk(b=>({...b,first:b.first.slice(0,-1)})); return; }
          if (k.length===1 && !e.metaKey && !e.ctrlKey){ e.preventDefault(); setBulk(b=>({...b,first:b.first+k})); }
          return;
        } else {
          if (k==='Enter'){ e.preventDefault(); bulkConfirm(); }
          else if (k==='ArrowLeft'){ e.preventDefault(); setBulk(b=>({...b,step:1})); }
          return;
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  /* ---------------- function bar ---------------- */
  const fkItems = () => {
    if (mode==='list') return [
      { key:'F1', label:'Help', onClick:()=>setMode('help') },
      { key:'F2', label:'Add', onClick:openAdd },
      { key:'F3', label:'Edit', onClick:openEdit },
      { key:'F4', label:'Proxy', onClick:toggleProxy },
      { key:'F5', label:'Bulk', onClick:openBulk, active:true },
      { key:'F6', label:'Filter', onClick:()=>setMode('filter') },
      { key:'F8', label:'Delete', danger:true, onClick:()=>{ if(sel) setMode('delete'); } },
      { key:'F10', label:'Quit', onClick:()=>flash('Quit — close the tab. (demo)','warn') },
    ];
    if (mode==='add'||mode==='edit') return [
      { key:'Esc', label:'Cancel', onClick:()=>setMode('list') },
      { key:'↹', label:'Next field' },
      { key:'↵', label: mode==='edit'?'Save':'Add', onClick:saveForm, active:true },
    ];
    if (mode==='delete') return [
      { key:'N', label:'Cancel', onClick:()=>setMode('list') },
      { key:'Y', label:'Delete', danger:true, onClick:confirmDelete, active:true },
    ];
    if (mode==='bulk' && bulk.step===1) return [
      { key:'Esc', label:'Cancel', onClick:()=>setMode('list') },
      { key:'↹', label:'Next field' },
      { key:'↵', label:'Next ▸', onClick:()=>setBulk(b=>({...b,step:2})), active:true },
    ];
    if (mode==='bulk' && bulk.step===2) return [
      { key:'Esc', label:'Back', onClick:()=>setBulk(b=>({...b,step:1})) },
      { key:'↵', label:`Add ${expand(bulk.first,bulk.ips).length}`, onClick:bulkConfirm, active:true },
    ];
    return [ { key:'Esc', label:'Close', onClick:()=>setMode('list'), active:true } ];
  };

  const statusLeft = mode==='filter'
    ? <span>filter: <span style={{color:'var(--fg)'}}>{filter}<span className="tui-cursor" style={{color:'var(--orange)'}}>█</span></span>&nbsp;&nbsp;<span style={{color:'var(--fg-faint)'}}>{filtered.length} match</span></span>
    : (filter ? <span>filter: <span style={{color:'var(--fg)'}}>{filter}</span> · {filtered.length} of {records.length}</span>
              : '');

  const meta = form ? TYPE_META[form.type] : null;
  const bmeta = bulk ? TYPE_META[bulk.type] : null;
  const dim = mode!=='list' && mode!=='filter';

  return (
    <Screen>
      <Header zone={ZONE} count={records.length} proxiedCount={proxiedCount} />
      <div className={dim ? 'tui-dimmed' : ''}>
        <RecordsTable records={filtered} selected={selected} onSelect={(i)=>{ clampSel(i); }}
          focused={mode==='list'||mode==='filter'} />
      </div>
      <StatusBar zone={ZONE} left={statusLeft} message={msg.text} messageType={msg.type}
        spinner={false} />
      <FunctionBar items={fkItems()} />

      {(mode==='add'||mode==='edit') &&
        <AddEditDialog mode={mode} form={form} focus={formFocus} meta={meta}
          onFocusField={setFormFocus} onCycleType={cycleType} onToggleProxy={toggleFormProxy}
          onCycleTTL={cycleTTL} onSave={saveForm} onCancel={()=>setMode('list')} />}
      {mode==='delete' && sel &&
        <DeleteDialog rec={sel} onConfirm={confirmDelete} onCancel={()=>setMode('list')} />}
      {mode==='bulk' &&
        <BulkWizard st={bulk} meta={bmeta} focus={bulk.focus}
          onFocusField={(id)=>setBulk(b=>({...b,focus:id}))} onCycleType={bulkCycleType}
          onToggleProxy={()=>setBulk(b=>({...b,proxied:!b.proxied}))}
          onBack={()=>setBulk(b=>({...b,step:1}))} onNext={()=>setBulk(b=>({...b,step:2}))}
          onConfirm={bulkConfirm} onCancel={()=>setMode('list')} />}
      {mode==='help' && <HelpOverlay onCancel={()=>setMode('list')} />}
    </Screen>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
