/* HostsScreen.jsx — screen 1: known hosts + CRUD.
   Columns: NAME TYPE ADDR PORT MOUNT-POINT LOGIN PASSWORD KEY MNT PRX
   Marks (Insert) render in --yellow-mark so they stay legible on the orange
   cursor bar; the mark glyph ● keeps the state readable even when cursored. */

const HCOL = { mark:2, name:14, type:6, addr:22, port:6, gap:2, mount:20, login:11, pass:11, key:7, mnt:5, prx:5 };
const HROWS = 12;

function maskPass(p){ return p ? '•'.repeat(Math.min(8, p.length)) : '—'; }

function HostsScreen({ hosts, cursor, marks, hovered, onCursor, onHover, onToggleMark, onGenKey, onOpen }){
  let start = 0;
  if (hosts.length > HROWS) start = Math.min(Math.max(0, cursor-(HROWS>>1)), hosts.length-HROWS);
  const win = hosts.slice(start, start+HROWS);

  const head = [
    { t:'', w:HCOL.mark },
    { t:'NAME',        w:HCOL.name,  cls:'tui-colhead' },
    { t:'TYPE',        w:HCOL.type,  cls:'tui-colhead' },
    { t:'ADDR',        w:HCOL.addr,  cls:'tui-colhead' },
    { t:'PORT',        w:HCOL.port,  cls:'tui-colhead', align:'right' },
    { t:'',            w:HCOL.gap },
    { t:'MOUNT POINT', w:HCOL.mount, cls:'tui-colhead' },
    { t:'LOGIN',       w:HCOL.login, cls:'tui-colhead' },
    { t:'PASSWORD',    w:HCOL.pass,  cls:'tui-colhead' },
    { t:'KEY',         w:HCOL.key,   cls:'tui-colhead', align:'center' },
    { t:'MNT',         w:HCOL.mnt,   cls:'tui-colhead', align:'center' },
    { t:'PRX',         w:HCOL.prx,   cls:'tui-colhead', align:'center' },
  ];

  const nMarked = marks.size;
  const right = nMarked ? `${nMarked} marked of ${hosts.length}` : `${hosts.length} hosts`;

  return (
    <div className="oa-hosts">
      <TopBorder w={COLS} title="Known Hosts" focus={true} right={right} />
      <BodyRow w={COLS} inner={head} focus={true} />
      <SepBorder w={COLS} focus={true} />
      {win.map((h, i) => {
        const idx = start+i;
        const cur = idx===cursor, marked = marks.has(h.id), hov = idx===hovered;
        const markCol = C.mark;
        const inner = [
          { t: marked ? '●' : (cur ? '▸' : ' '), w:HCOL.mark,
            color: marked ? markCol : C.orange, keepColor:marked, bold:marked },
          { t:h.name,  w:HCOL.name,  color: marked ? markCol : C.fg, keepColor:marked, bold:marked },
          { t:h.type,  w:HCOL.type,  color: h.type==='SSH' ? C.blue : C.magenta },
          { t:h.addr,  w:HCOL.addr,  color:C.fg },
          { t:String(h.port), w:HCOL.port, align:'right', color:C.muted },
          { t:'', w:HCOL.gap },
          { t:h.mount, w:HCOL.mount, color: h.mountAuto===false ? C.fg : C.muted },
          { t:h.login, w:HCOL.login, color:C.fg },
          { t:maskPass(h.pass), w:HCOL.pass, color: h.pass ? C.muted : C.disabled },
          { t: h.key ? '✓' : '[gen]', w:HCOL.key, align:'center',
            color: h.key ? C.green : C.orangeB, keepColor:!h.key,
            cls: h.key ? '' : 'oa-genkey' },
          { t: h.mounted ? '●' : '○', w:HCOL.mnt, align:'center',
            color: h.mounted ? C.green : C.faint, keepColor:h.mounted },
          { t: h.proxy ? '●' : '○', w:HCOL.prx, align:'center',
            color: h.proxy ? C.orange : C.faint, keepColor:h.proxy },
        ];
        return (
          <BodyRow key={h.id} w={COLS} inner={inner} focus={true}
            bg={ cur ? C.orange : (marked ? 'var(--bg-sel)' : (hov ? 'var(--bg-hover)' : null)) }
            ink={ cur ? C.ink : null }
            onClick={(e)=>{ onCursor(idx); }}
            onDoubleClick={()=>onOpen(idx)}
            onMouseEnter={()=>onHover(idx)} onMouseLeave={()=>onHover(-1)}
            className="oa-row" />
        );
      })}
      {Array.from({length: Math.max(0, HROWS-win.length)}).map((_,i)=>(
        <BodyRow key={'p'+i} w={COLS} inner={[]} focus={true} />
      ))}
      <BotBorder w={COLS} focus={true} />
    </div>
  );
}

window.HostsScreen = HostsScreen;
window.HCOL = HCOL;
window.maskPass = maskPass;
