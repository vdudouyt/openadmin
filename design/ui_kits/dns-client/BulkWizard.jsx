/* BulkWizard.jsx — the signature feature. Step 1 configures a type, proxy state,
   a seed hostname containing a number (e.g. s1000.mydomain.com) and a list of IPs.
   Step 2 previews the expansion  s1000 => ip1 · s1001 => ip2 · …  before adding. */

const DWB = 74;

/* split the seed hostname around its first digit run, preserving zero-pad width */
function parseSeed(host){
  const m = /^(\D*)(\d+)(.*)$/.exec(host || '');
  if (!m) return null;
  return { prefix:m[1], num:parseInt(m[2],10), width:m[2].length, suffix:m[3] };
}
function ipLines(ips){
  return (ips || '').split('\n').map(s => s.trim()).filter(Boolean);
}
function expand(seed, ips){
  const p = parseSeed(seed);
  const list = ipLines(ips);
  if (!p) return [];
  return list.map((ip, i) => ({
    name: p.prefix + String(p.num + i).padStart(p.width, '0') + p.suffix,
    content: ip,
  }));
}

function BulkWizard(props){
  const { st, meta, focus, onFocusField, onCycleType, onToggleProxy,
          onBack, onNext, onConfirm, onCancel } = props;
  const f = id => focus === id;
  const proxiable = meta.proxiable;

  if (st.step === 1){
    const lines = ipLines(st.ips);
    const TA_ROWS = 6;
    const rows = [];
    rows.push([]);
    rows.push(fieldInner('Type', selectSegs(st.type, 16, f('type')), f('type')));
    rows.push([]);
    if (proxiable){
      rows.push(radioInner('Proxy', st.proxied, f('proxy')));
      rows.push([]);
    }
    rows.push(fieldInner('Seed host', wellSegs(st.first, 's1000.'+ZONE, 50, f('first')), f('first')));
    rows.push([{ t:' '.repeat(LABELW+2), }, { t:'the number is incremented per IP below', color:C.faint }]);
    rows.push([]);
    rows.push([{ t:'IP list', w:LABELW, align:'right', color: f('ips')?C.orange:C.muted }, { t:'  ' },
               { t: f('ips') ? 'one IP per line  ▼' : 'one IP per line', color:C.faint }]);
    // textarea region
    for (let i=0;i<TA_ROWS;i++){
      const line = lines[i] || '';
      const isCur = f('ips') && i === Math.min(lines.length, TA_ROWS-1) && lines.length < TA_ROWS;
      const lineCur = f('ips') && i === lines.length;
      const txt = line || '';
      const used = 1 + txt.length + (lineCur?1:0);
      const seg = [{ t:'  ' }, { t:' ', bg:INSET },
        ...(txt ? [{ t:txt, bg:INSET, color:C.fg }] : []),
        ...(lineCur ? [{ t:'█', bg:INSET, color:C.orange, cls:'tui-cursor' }] : []),
        { t:' '.repeat(Math.max(0, 52-used)), bg:INSET }];
      rows.push(seg);
    }
    rows.push([{ t:'  ' }, { t: `${lines.length} IP${lines.length===1?'':'s'} entered`, color:C.muted }]);
    rows.push([]);
    rows.push([{ t:'Tab next field   Enter in IP list = new line   Esc cancel', color:C.faint }]);
    rows.push([]);
    rows.push([{ fill:true }, { t:' Next ▸ ', bg:C.orange, color:C.ink, bold:true }, { t:'   ' }, { t:'[ Cancel ]', color:C.fg }, { t:' ' }]);
    rows.push([]);

    return (
      <DialogOverlay onBackdrop={onCancel}>
        <TopBorder w={DWB} title="Bulk Add · 1 of 2 — Configure" focus={true} />
        {rows.map((inner,i)=>(
          <BodyRow key={i} w={DWB} inner={inner} focus={true}
            onClick={ i===2&&false ? null : (i===1 ? ()=>onCycleType(1) : (proxiable && i===3 ? onToggleProxy : (i===rows.length-2 ? onNext : null))) }
            className={ (i===1)||(proxiable&&i===3)||(i===rows.length-2) ? 'tui-clickrow':'' } />
        ))}
        <BotBorder w={DWB} focus={true} />
      </DialogOverlay>
    );
  }

  // step 2 — preview
  const pairs = expand(st.first, st.ips);
  const SHOW = 9;
  const shown = pairs.slice(0, SHOW);
  const extra = pairs.length - SHOW;
  const rows = [];
  rows.push([]);
  rows.push([{ t:'Will create ', color:C.fg }, { t:String(pairs.length), color:C.orange, bold:true },
             { t:` ${st.type} record${pairs.length===1?'':'s'}`, color:C.fg },
             ...(proxiable ? [{ t:'   ' }, { t: st.proxied?'▲ proxied':'○ dns-only', color: st.proxied?C.orange:C.muted }] : []) ]);
  rows.push([]);
  shown.forEach(p => {
    rows.push([
      { t:p.name, w:34, color:C.fg },
      { t:'=> ', color:C.faint },
      { t:p.content, color:C.blue },
    ]);
  });
  if (extra > 0) rows.push([{ t:`  … and ${extra} more`, color:C.faint }]);
  if (pairs.length === 0) rows.push([{ t:'  Nothing to expand — check the seed host and IP list.', color:C.yellow }]);
  rows.push([]);
  rows.push([{ fill:true },
             { t:'[ ◂ Back ]', color:C.fg }, { t:'   ' },
             { t:` Add ${pairs.length} Record${pairs.length===1?'':'s'} `, bg: pairs.length?C.orange:C.disabled, color:C.ink, bold:true },
             { t:'   ' }, { t:'[ Cancel ]', color:C.fg }, { t:' ' }]);
  rows.push([]);

  const lastIdx = rows.length - 2;
  return (
    <DialogOverlay onBackdrop={onCancel}>
      <TopBorder w={DWB} title="Bulk Add · 2 of 2 — Preview" focus={true} />
      {rows.map((inner,i)=>(
        <BodyRow key={i} w={DWB} inner={inner} focus={true}
          onClick={ i===lastIdx ? (pairs.length?onConfirm:null) : null }
          className={ i===lastIdx && pairs.length ? 'tui-clickrow':'' } />
      ))}
      <BotBorder w={DWB} focus={true} />
    </DialogOverlay>
  );
}

Object.assign(window, { BulkWizard, parseSeed, expand, ipLines });
