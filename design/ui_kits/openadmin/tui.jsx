/* tui.jsx — character-grid primitives for OpenAdmin (shared vocabulary with the
   cfdns kit, widened to 120 columns for the hosts table). Rows are
   <div class="tui-row"> with white-space:pre so box-drawing glyphs connect;
   coloring is inline <span> which does not disturb the grid. */

const COLS = 120;

const C = {
  fg:'var(--fg)', bright:'var(--fg-bright)', muted:'var(--fg-muted)',
  faint:'var(--fg-faint)', disabled:'var(--fg-disabled)',
  orange:'var(--orange)', orangeB:'var(--orange-bright)', orangeD:'var(--orange-dim)',
  ink:'var(--orange-ink)', green:'var(--green)', red:'var(--red)', blue:'var(--blue)',
  yellow:'var(--yellow)', mark:'var(--yellow-mark)', magenta:'var(--magenta)',
  line:'var(--line)', inset:'var(--bg-inset)',
};

const SETS = {
  single: ['┌','┐','└','┘','─','│','├','┤'],
  double: ['╔','╗','╚','╝','═','║','╠','╣'],
};

function pad(s, w, align){
  s = s==null ? '' : String(s);
  if (w==null) return s;
  if (s.length > w) return s.length>1 ? s.slice(0, Math.max(0,w-1))+'…' : s.slice(0,w);
  const gap = w - s.length;
  if (align==='right')  return ' '.repeat(gap) + s;
  if (align==='center') return ' '.repeat(gap>>1) + s + ' '.repeat(gap-(gap>>1));
  return s + ' '.repeat(gap);
}

function buildSpans(segs, w){
  let fixed = 0, fills = 0;
  for (const s of segs){
    if (s.fill) fills++;
    else fixed += (s.w!=null ? s.w : (s.t==null ? 0 : String(s.t).length));
  }
  const rem = Math.max(0, (w!=null ? w : fixed) - fixed);
  const per = fills ? Math.floor(rem/fills) : 0;
  let extra = rem - per*fills, fi = 0;
  return segs.map((s, i) => {
    let t;
    if (s.fill){ const ww = per + (fi===fills-1 ? extra : 0); fi++; t = (s.ch||' ').repeat(ww); }
    else if (s.w!=null) t = pad(s.t, s.w, s.align);
    else t = s.t==null ? '' : String(s.t);
    const st = {};
    if (s.color) st.color = s.color;
    if (s.bg)    st.background = s.bg;
    if (s.bold)  st.fontWeight = 500;
    if (s.ul)    st.textDecoration = 'underline';
    return React.createElement('span', { key:i, className:s.cls||'', style:st }, t);
  });
}

function Row({ segs, w=COLS, style, className, onClick, onDoubleClick, onMouseEnter, onMouseLeave, title }){
  return (
    <div className={'tui-row ' + (className||'')} style={style} title={title}
         onClick={onClick} onDoubleClick={onDoubleClick}
         onMouseEnter={onMouseEnter} onMouseLeave={onMouseLeave}>
      {buildSpans(segs, w)}
    </div>
  );
}

const bcOf = (focus, double) => focus ? C.orange : (double ? C.orangeD : C.line);

function TopBorder({ w=COLS, title, focus, double, right }){
  const [tl,tr,,,H] = SETS[double?'double':'single'];
  const bc = bcOf(focus, double);
  const segs = [{ t: tl + H, color: bc }];
  if (title){
    segs.push({ t:' ', color:bc },
              { t:title, color: focus?C.bright:C.muted, bold:true },
              { t:' ', color:bc });
  }
  segs.push({ fill:true, ch:H, color:bc });
  if (right) segs.push({ t:' '+right+' ', color:C.faint });
  segs.push({ t: H + tr, color: bc });
  return <Row w={w} segs={segs} />;
}
function SepBorder({ w=COLS, focus, double }){
  const set = SETS[double?'double':'single'];
  const bc = bcOf(focus, double);
  return <Row w={w} segs={[{ t:set[6], color:bc },{ fill:true, ch:set[4], color:bc },{ t:set[7], color:bc }]} />;
}
function BotBorder({ w=COLS, focus, double }){
  const [,,bl,br,H] = SETS[double?'double':'single'];
  const bc = bcOf(focus, double);
  return <Row w={w} segs={[{ t:bl, color:bc },{ fill:true, ch:H, color:bc },{ t:br, color:bc }]} />;
}

/* A content row inside a frame: │ <pad> ...inner... <fill> │
   `bg` tints the whole inner region (reverse-video); `ink` overrides text color. */
function BodyRow({ inner, w=COLS, focus, double, bg, ink, onClick, onDoubleClick, onMouseEnter, onMouseLeave, className, title }){
  const V = SETS[double?'double':'single'][5];
  const bc = bcOf(focus, double);
  const lead = { t:' ' }, fill = { fill:true };
  let content = inner;
  if (bg){
    content = inner.map(s => ({ ...s, bg, color: s.keepColor ? s.color : (ink || s.color) }));
    lead.bg = bg; fill.bg = bg;
  }
  return <Row w={w} segs={[{ t:V, color:bc }, lead, ...content, fill, { t:V, color:bc }]}
    onClick={onClick} onDoubleClick={onDoubleClick}
    onMouseEnter={onMouseEnter} onMouseLeave={onMouseLeave}
    className={className} title={title} />;
}

/* Center a fixed-width screen in the viewport, scaled to fit. */
function Screen({ children }){
  const ref = React.useRef(null);
  React.useEffect(()=>{
    const el = ref.current; if(!el) return;
    const fit = ()=>{
      const nw = el.offsetWidth, nh = el.offsetHeight;
      if(!nw||!nh) return;
      let s = Math.min(window.innerWidth/nw, window.innerHeight/nh) * 0.96;
      el.style.transform = `translate(-50%,-50%) scale(${Math.min(s,1.3)})`;
    };
    // always measure AFTER layout settles — reading inside the observer
    // callback can catch a stale box and produce a wildly wrong scale
    let raf = 0;
    const schedule = ()=>{ cancelAnimationFrame(raf); raf = requestAnimationFrame(fit); };
    schedule();
    window.addEventListener('resize', schedule);
    const ro = new ResizeObserver(schedule);
    ro.observe(el);
    if (document.fonts && document.fonts.ready) document.fonts.ready.then(schedule);
    // low-frequency safety net: re-assert the transform if anything external
    // clears it (print/export passes, screenshot tooling, devtools edits)
    const heal = setInterval(fit, 1000);
    return ()=>{ cancelAnimationFrame(raf); clearInterval(heal);
      window.removeEventListener('resize', schedule); ro.disconnect(); };
  }, []);
  return <div className="tui-stage"><div className="tui-screen" ref={ref}>{children}</div></div>;
}

/* text field / select / cursor helpers shared by dialogs */
function wellSegs(value, ph, width, focused){
  const txt = value || (!focused ? ph : '');
  const used = 1 + txt.length + (focused ? 1 : 0);
  const segs = [{ t:' ', bg:C.inset }];
  if (txt) segs.push({ t:txt, bg:C.inset, color: value ? C.fg : C.faint });
  if (focused) segs.push({ t:'█', bg:C.inset, color:C.orange, cls:'tui-cursor' });
  segs.push({ t:' '.repeat(Math.max(0, width-used)), bg:C.inset });
  return segs;
}
function selectSegs(value, width, focused, disabled){
  const used = 1 + String(value).length;
  return [
    { t:' ', bg:C.inset },
    { t:value, bg:C.inset, color: disabled ? C.disabled : C.fg },
    { t:' '.repeat(Math.max(0, width-used-2)), bg:C.inset },
    { t:'▾ ', bg:C.inset, color: disabled ? C.disabled : (focused ? C.orange : C.muted) },
  ];
}
const LABELW = 14;
const labelSeg = (label, focused) => ({ t:label, w:LABELW, align:'right', color: focused ? C.orange : C.muted });
const fieldInner = (label, segs, focused) => [ labelSeg(label, focused), { t:'  ' }, ...segs ];

function Spinner(){
  const frames = '⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏';
  const [i, setI] = React.useState(0);
  React.useEffect(()=>{
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const id = setInterval(()=>setI(x=>(x+1)%frames.length), 80);
    return ()=>clearInterval(id);
  },[]);
  return <span style={{color:'var(--orange)'}}>{frames[i]}</span>;
}

Object.assign(window, { COLS, C, pad, buildSpans, Row, TopBorder, SepBorder, BotBorder,
  BodyRow, Screen, wellSegs, selectSegs, fieldInner, labelSeg, LABELW, Spinner });
