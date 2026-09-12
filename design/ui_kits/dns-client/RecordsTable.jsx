/* RecordsTable.jsx — the central scrollable DNS records grid.
   Columns (inner width 101): [marker 2][TYPE 6][NAME 20][CONTENT 58][TTL 9][PRX 6] */

const COL = { mark:2, type:6, name:20, content:58, ttl:9, prx:6 };
const VISIBLE = 15;

function proxySeg(rec){
  const meta = TYPE_META[rec.type];
  if (!meta || !meta.proxiable) return { t:'·', w:COL.prx, align:'center', color:C.disabled };
  return rec.proxied
    ? { t:'▲', w:COL.prx, align:'center', color:C.orange }
    : { t:'○', w:COL.prx, align:'center', color:C.muted };
}

function contentText(rec){
  if (rec.priority != null) return rec.priority + '  ' + rec.content;
  return rec.content;
}

function RecordsTable({ records, selected, onSelect, focused, height=VISIBLE }){
  // scroll window
  let start = 0;
  if (records.length > height){
    start = Math.min(Math.max(0, selected - (height>>1)), records.length - height);
  }
  const window_ = records.slice(start, start + height);
  const more = records.length - height;

  const headSegs = [
    { t:'', w:COL.mark },
    { t:'TYPE',    w:COL.type,    cls:'tui-colhead' },
    { t:'NAME',    w:COL.name,    cls:'tui-colhead' },
    { t:'CONTENT', w:COL.content, cls:'tui-colhead' },
    { t:'TTL',     w:COL.ttl,     cls:'tui-colhead' },
    { t:'PRX',     w:COL.prx,     cls:'tui-colhead', align:'center' },
  ];

  const rightInfo = records.length + (more>0 ? ` · ${start+1}\u2013${start+height}` : '');

  return (
    <div className="tui-table">
      <TopBorder w={COLS} title="Records" focus={focused} right={rightInfo} />
      <BodyRow w={COLS} inner={headSegs} focus={focused} />
      <SepBorder w={COLS} focus={focused} />
      {window_.map((rec, i) => {
        const idx = start + i;
        const isSel = idx === selected;
        const inner = [
          { t: isSel ? '▸' : ' ', w:COL.mark, color:C.orange },
          { t: rec.type,          w:COL.type,    color: TYPE_COLOR[rec.type] || C.fg },
          { t: rec.name,          w:COL.name,    color: rec.name==='@' ? C.muted : C.fg },
          { t: contentText(rec),  w:COL.content, color: C.fg },
          { t: ttlLabel(rec.ttl), w:COL.ttl,     color: C.muted },
          proxySeg(rec),
        ];
        return (
          <BodyRow key={rec.id} w={COLS} inner={inner} focus={focused}
            bg={ isSel ? (focused ? C.orange : 'var(--bg-sel)') : null }
            ink={ isSel && focused ? C.ink : null }
            onClick={() => onSelect(idx)} className="tui-clickrow" />
        );
      })}
      {/* pad to fixed height so the panel never jumps */}
      {Array.from({ length: Math.max(0, height - window_.length) }).map((_, i) => (
        <BodyRow key={'pad'+i} w={COLS} inner={[]} focus={focused} />
      ))}
      <BotBorder w={COLS} focus={focused} />
    </div>
  );
}

window.RecordsTable = RecordsTable;
