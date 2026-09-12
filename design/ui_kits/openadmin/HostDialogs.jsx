/* HostDialogs.jsx — add/edit host, delete confirm, and SSH public key output.
   The Add/Edit form derives MOUNT POINT from NAME (/net/<name>) until the user
   edits the mount field themselves, after which it is left alone. */

const DW = 72;

function DialogOverlay({ children, onBackdrop }){
  return (
    <div className="oa-overlay" onClick={onBackdrop}>
      <div className="oa-dialog" onClick={e=>e.stopPropagation()}>{children}</div>
    </div>
  );
}

const FORM_FIELDS = ['name','type','addr','port','mount','login','pass'];

function HostDialog({ mode, form, focus, onFocus, onCycleType, onSave, onCancel, onGenKey }){
  const f = id => focus===id;
  const rows = [];
  const R = (inner, onClick) => rows.push({ inner, onClick });

  R([]);
  R(fieldInner('Host name', wellSegs(form.name, 'nickname, e.g. web-01', 34, f('name')), f('name')), ()=>onFocus('name'));
  R([{ t:' '.repeat(LABELW+2) }, { t:'a human label — the mount point follows it', color:C.faint }]);
  R(fieldInner('Type', selectSegs(form.type, 12, f('type')), f('type')), ()=>onCycleType(1));
  R(fieldInner('Address', wellSegs(form.addr, 'host or IP', 34, f('addr')), f('addr')), ()=>onFocus('addr'));
  R(fieldInner('Port', wellSegs(String(form.port??''), '22', 8, f('port')), f('port')), ()=>onFocus('port'));
  R(fieldInner('Mount point', wellSegs(form.mount, '/net/<name>', 34, f('mount')), f('mount')), ()=>onFocus('mount'));
  R([{ t:' '.repeat(LABELW+2) },
     { t: form.mountAuto ? 'auto from host name — type here to override' : 'overridden · clear to restore auto',
       color: form.mountAuto ? C.faint : C.yellow }]);
  R(fieldInner('Login', wellSegs(form.login, 'user', 24, f('login')), f('login')), ()=>onFocus('login'));
  R(fieldInner('Password', wellSegs(form.pass ? '•'.repeat(form.pass.length) : '', 'optional with a key', 24, f('pass')), f('pass')), ()=>onFocus('pass'));
  R([]);
  R([ labelSeg('SSH key', false), { t:'  ' },
      ...(form.key
        ? [{ t:'✓ ', color:C.green }, { t:'key installed', color:C.fg }, { t:'    ' },
           { t:'[ Show public key ]', color:C.orangeB, cls:'oa-btn' }]
        : [{ t:'[ Generate SSH key ]', color:C.orangeB, cls:'oa-btn' }, { t:'   ' },
           { t:'prints the public key', color:C.faint }]) ],
     onGenKey);
  R([]);
  R([{ t:'Tab next field   ←/→ change type   Enter save   Esc cancel', color:C.faint }]);
  R([{ fill:true }, { t:` ${mode==='edit'?'Save':'Add Host'} `, bg:C.orange, color:C.ink, bold:true },
     { t:'   ' }, { t:'[ Cancel ]', color:C.fg }, { t:' ' }]);
  R([]);

  const saveIdx = rows.length-2;
  return (
    <DialogOverlay onBackdrop={onCancel}>
      <TopBorder w={DW} title={mode==='edit' ? 'Edit Host' : 'Add Host'} focus={true} />
      {rows.map((r,i)=>(
        <BodyRow key={i} w={DW} inner={r.inner} focus={true}
          onClick={ i===saveIdx ? onSave : r.onClick }
          className={ (r.onClick||i===saveIdx) ? 'oa-clickrow' : '' } />
      ))}
      <BotBorder w={DW} focus={true} />
    </DialogOverlay>
  );
}

function DeleteDialog({ targets, onConfirm, onCancel }){
  const W = 58;
  const many = targets.length > 1;
  const rows = [
    [],
    [{ t:'Delete ', color:C.fg },
     { t: many ? `${targets.length} hosts` : `"${targets[0].name}"`, color: many?C.mark:C.fg, bold:true },
     { t:' ?', color:C.fg }],
    ...(many
      ? targets.slice(0,4).map(t=>[{ t:'  '+t.name, color:C.muted }])
        .concat(targets.length>4 ? [[{ t:`  … and ${targets.length-4} more`, color:C.faint }]] : [])
      : [[{ t:`  ${targets[0].login}@${targets[0].addr}:${targets[0].port}`, color:C.muted }]]),
    [],
    [{ t:'Saved credentials and keys are removed too.', color:C.muted }],
    [],
    [{ fill:true }, { t:' Delete ', bg:C.red, color:'#1a0000', bold:true },
     { t:'   ' }, { t:'[ Cancel ]', color:C.fg }, { t:' ' }],
    [],
  ];
  const ci = rows.length-2;
  return (
    <DialogOverlay onBackdrop={onCancel}>
      <TopBorder w={W} title="Confirm Delete" double={true} />
      {rows.map((inner,i)=>(
        <BodyRow key={i} w={W} inner={inner} double={true}
          onClick={i===ci?onConfirm:null} className={i===ci?'oa-clickrow':''} />
      ))}
      <BotBorder w={W} double={true} />
    </DialogOverlay>
  );
}

/* SSH public key output — the key is shown wrapped so it can be copied out. */
function KeyDialog({ host, onCancel, onCopy }){
  const W = 86;
  const inner = W - 4;
  const chunks = [];
  for (let i=0;i<PUBKEY.length;i+=inner) chunks.push(PUBKEY.slice(i, i+inner));
  const rows = [
    [],
    [{ t:'Public key for ', color:C.fg }, { t:host.name, color:C.orange, bold:true },
     { t:'  ·  ed25519', color:C.muted }],
    [],
    [{ t:'Add this line to ', color:C.muted }, { t:'~/.ssh/authorized_keys', color:C.blue },
     { t:' on the remote host:', color:C.muted }],
    [],
    ...chunks.map(c => [{ t:' ' }, { t:c, w:inner, bg:C.inset, color:C.green }, { t:' ', bg:C.inset }]),
    [],
    [{ t:'Private key stored at ', color:C.muted }, { t:'~/.config/openadmin/keys/'+host.name, color:C.faint }],
    [],
    [{ fill:true }, { t:' Copy ', bg:C.orange, color:C.ink, bold:true },
     { t:'   ' }, { t:'[ Close ]', color:C.fg }, { t:' ' }],
    [],
  ];
  const ci = rows.length-2;
  return (
    <DialogOverlay onBackdrop={onCancel}>
      <TopBorder w={W} title="SSH Public Key" focus={true} />
      {rows.map((r,i)=>(
        <BodyRow key={i} w={W} inner={r} focus={true}
          onClick={i===ci?onCopy:null} className={i===ci?'oa-clickrow':''} />
      ))}
      <BotBorder w={W} focus={true} />
    </DialogOverlay>
  );
}

function HelpDialog({ onCancel }){
  const W = 66;
  const k = (key, desc) => [{ t:key, w:14, color:C.orangeB }, { t:desc, color:C.fg }];
  const rows = [
    [],
    [{ t:'SCREENS', cls:'tui-colhead' }],
    k('Alt+1/2/3','Hosts · Shells · Chat'),
    k('F9','cycle screen'),
    [],
    [{ t:'HOSTS', cls:'tui-colhead' }],
    k('↑ ↓','move cursor'),
    k('Insert','mark / unmark host (multi-select)'),
    k('* ','invert marks     Ctrl+A select all'),
    k('F2 / F3','add · edit'),
    k('F4','mount / unmount'),
    k('F5','open shell (marked hosts → group tab)'),
    k('F6','use as proxy'),
    k('F7','generate SSH key'),
    k('F8','delete'),
    [],
    [{ t:'MOUSE', cls:'tui-colhead' }],
    k('hover','row highlights, action hint in status bar'),
    k('click','move cursor    double-click opens a shell'),
    [],
    [{ t:'Esc closes any dialog · Tab moves between fields', color:C.faint }],
    [],
  ];
  return (
    <DialogOverlay onBackdrop={onCancel}>
      <TopBorder w={W} title="Help · Key Bindings" focus={true} />
      {rows.map((r,i)=><BodyRow key={i} w={W} inner={r} focus={true} />)}
      <BotBorder w={W} focus={true} />
    </DialogOverlay>
  );
}

Object.assign(window, { HostDialog, DeleteDialog, KeyDialog, HelpDialog, DialogOverlay, FORM_FIELDS, DW });
