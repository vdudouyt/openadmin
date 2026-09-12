/* Dialogs.jsx — modal overlays drawn as char-grid panels:
   shared field/select/radio/button builders, AddEditDialog, DeleteDialog, HelpOverlay.
   Text editing & navigation are driven by the app's global key handler; rows are
   also clickable for mouse use. */

const INSET = 'var(--bg-inset)';
const DW = 66;                       // dialog width in cells
const LABELW = 13;

function DialogOverlay({ children, onBackdrop }){
  return (
    <div className="tui-overlay" onClick={onBackdrop}>
      <div className="tui-dialog" onClick={e => e.stopPropagation()}>{children}</div>
    </div>
  );
}

/* ---- field builders (return inner-seg arrays for a BodyRow) ---- */
function wellSegs(value, ph, width, focused){
  const showPh = !value && !focused;
  const txt = value || (showPh ? ph : '');
  const used = 1 + txt.length + (focused ? 1 : 0);
  const trail = Math.max(0, width - used);
  const segs = [{ t:' ', bg:INSET }];
  if (txt) segs.push({ t:txt, bg:INSET, color: value ? C.fg : C.faint });
  if (focused) segs.push({ t:'█', bg:INSET, color:C.orange, cls:'tui-cursor' });
  segs.push({ t:' '.repeat(trail), bg:INSET });
  return segs;
}
function selectSegs(value, width, focused, disabled){
  const txt = value, used = 1 + txt.length, trail = Math.max(0, width - used - 2);
  return [
    { t:' ', bg:INSET },
    { t:txt, bg:INSET, color: disabled ? C.disabled : C.fg },
    { t:' '.repeat(trail), bg:INSET },
    { t:'▾ ', bg:INSET, color: disabled ? C.disabled : (focused ? C.orange : C.muted) },
  ];
}
function labelSeg(label, focused){
  return { t:label, w:LABELW, align:'right', color: focused ? C.orange : C.muted };
}
function fieldInner(label, segs, focused){
  return [ labelSeg(label, focused), { t:'  ' }, ...segs ];
}
function radioInner(label, proxied, focused){
  return [
    labelSeg(label, focused), { t:'  ' },
    { t: proxied ? '(•)' : '( )', color: proxied ? C.orange : C.muted },
    { t:' ▲ Proxied', color: proxied ? C.orange : C.fg },
    { t:'     ' },
    { t: !proxied ? '(•)' : '( )', color: !proxied ? C.orange : C.muted },
    { t:' ○ DNS only', color: !proxied ? C.bright : C.fg },
  ];
}
/* default action rendered reverse-video; returns inner segs, right-aligned */
function buttonInner(primaryLabel){
  return [
    { fill:true },
    { t:` ${primaryLabel} `, bg:C.orange, color:C.ink, bold:true },
    { t:'   ' },
    { t:'[ Cancel ]', color:C.fg },
    { t:'  ' },
  ];
}

/* ============================ Add / Edit ============================ */
function AddEditDialog({ mode, form, focus, meta, onFocusField, onCycleType, onToggleProxy, onCycleTTL, onSave, onCancel }){
  const title = mode==='edit' ? 'Edit DNS Record' : 'Add DNS Record';
  const f = (id) => focus===id;
  const proxiable = meta.proxiable;
  const ttlDisabled = proxiable && form.proxied;

  const rows = [];
  const push = (inner, onClick) => rows.push({ inner, onClick });

  push([]);
  push(fieldInner('Type', selectSegs(form.type, 16, f('type')), f('type')), () => onCycleType(1));
  push([]);
  push(fieldInner('Name', wellSegs(form.name, 'subdomain or @', 40, f('name')), f('name')), () => onFocusField('name'));
  push([]);
  push(fieldInner(meta.label[0].toUpperCase()+meta.label.slice(1),
        wellSegs(form.content, meta.ph, 44, f('content')), f('content')), () => onFocusField('content'));
  if (meta.priority){
    push([]);
    push(fieldInner('Priority', wellSegs(String(form.priority ?? ''), '10', 10, f('priority')), f('priority')), () => onFocusField('priority'));
  }
  push([]);
  push(fieldInner('TTL', selectSegs(ttlDisabled ? 'Auto (proxied)' : ttlLabel(form.ttl), 18, f('ttl'), ttlDisabled), f('ttl')),
       () => { if (!ttlDisabled) onCycleTTL(1); });
  if (proxiable){
    push([]);
    push(radioInner('Proxy', form.proxied, f('proxy')), () => onToggleProxy());
  }
  push([]);
  push([{ t:'Tab next  ←/→ change  Enter save  Esc cancel', color:C.faint }]);
  push([]);
  push(buttonInner(mode==='edit' ? 'Save' : 'Add Record'));
  push([]);

  return (
    <DialogOverlay onBackdrop={onCancel}>
      <TopBorder w={DW} title={title} focus={true} />
      {rows.map((r,i) => (
        <BodyRow key={i} w={DW} inner={r.inner} focus={true}
          onClick={i===rows.length-2 ? onSave : r.onClick} className={r.onClick||i===rows.length-2 ? 'tui-clickrow':''} />
      ))}
      <BotBorder w={DW} focus={true} />
    </DialogOverlay>
  );
}

/* ============================ Delete confirm ============================ */
function DeleteDialog({ rec, onConfirm, onCancel }){
  const DWX = 56;
  const rows = [
    [],
    [{ t:'Delete ', color:C.fg }, { t:rec.type+' ', color: TYPE_COLOR[rec.type]||C.fg }, { t:'"'+rec.name+'"', color:C.fg }, { t:' ?', color:C.fg }],
    [{ t:rec.content, color:C.muted }],
    [],
    [{ t:'This record cannot be recovered.', color:C.muted }],
    [],
    [{ fill:true }, { t:' Delete ', bg:C.red, color:'#1a0000', bold:true }, { t:'   ' }, { t:'[ Cancel ]', color:C.fg }, { t:' ' }],
    [],
  ];
  return (
    <DialogOverlay onBackdrop={onCancel}>
      <TopBorder w={DWX} title="Confirm Delete" double={true} />
      {rows.map((inner,i)=>(
        <BodyRow key={i} w={DWX} inner={inner} double={true}
          onClick={i===6 ? onConfirm : null} className={i===6?'tui-clickrow':''} />
      ))}
      <BotBorder w={DWX} double={true} />
    </DialogOverlay>
  );
}

/* ============================ Help ============================ */
function HelpOverlay({ onCancel }){
  const DWH = 60;
  const k = (key, desc) => [{ t:key, w:10, color:C.orangeB }, { t:desc, color:C.fg }];
  const rows = [
    [],
    [{ t:'NAVIGATION', cls:'tui-colhead' }],
    k('↑ ↓','move selection'),
    k('Home End','jump to first / last'),
    k('Enter','edit selected record'),
    [],
    [{ t:'ACTIONS', cls:'tui-colhead' }],
    k('F2','add a new record'),
    k('F3','edit selected'),
    k('F4','toggle proxy (A · AAAA · CNAME)'),
    k('F5','bulk add — sequential hostnames'),
    k('F6','filter records'),
    k('F8','delete selected'),
    k('F10 / q','quit'),
    [],
    [{ t:'Esc closes any dialog · Tab moves between fields', color:C.faint }],
    [],
  ];
  return (
    <DialogOverlay onBackdrop={onCancel}>
      <TopBorder w={DWH} title="Help · Key Bindings" focus={true} />
      {rows.map((inner,i)=> <BodyRow key={i} w={DWH} inner={inner} focus={true} />)}
      <BotBorder w={DWH} focus={true} />
    </DialogOverlay>
  );
}

Object.assign(window, { DialogOverlay, AddEditDialog, DeleteDialog, HelpOverlay,
  wellSegs, selectSegs, fieldInner, radioInner, INSET, DW, LABELW });
