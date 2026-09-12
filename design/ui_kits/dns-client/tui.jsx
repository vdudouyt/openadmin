/* tui.jsx — core primitives for rendering the cfdns terminal UI in React.
   Everything is drawn on a fixed character grid (COLS × ROWS) that scales to fit
   the viewport. Rows are <div class="tui-row"> with white-space:pre so box-drawing
   glyphs in fixed columns connect vertically. Coloring via inline <span>.        */

const COLS = 104;
const ROWS = 34;

const C = {
  fg:'var(--fg)', bright:'var(--fg-bright)', muted:'var(--fg-muted)',
  faint:'var(--fg-faint)', disabled:'var(--fg-disabled)',
  orange:'var(--orange)', orangeB:'var(--orange-bright)', orangeD:'var(--orange-dim)',
  ink:'var(--orange-ink)', green:'var(--green)', red:'var(--red)', blue:'var(--blue)',
  yellow:'var(--yellow)', magenta:'var(--magenta)', line:'var(--line)',
};

const SETS = {
  single: ['┌','┐','└','┘','─','│','├','┤'],
  double: ['╔','╗','╚','╝','═','║','╠','╣'],
};

function pad(s, w, align){
  s = s==null ? '' : String(s);
  if (w==null) return s;
  if (s.length > w) return s.slice(0, w);
  const gap = w - s.length;
  if (align==='right')  return ' '.repeat(gap) + s;
  if (align==='center') return ' '.repeat(gap>>1) + s + ' '.repeat(gap-(gap>>1));
  return s + ' '.repeat(gap);
}

/* Turn an array of segment descriptors into React <span>s, expanding any {fill}
   segment to consume the remaining width up to total `w`.
   seg = { t, w, align, color, bg, bold, cls, fill, ch }  */
function buildSpans(segs, w){
  let fixed = 0, fills = 0;
  for (const s of segs){
    if (s.fill) fills++;
    else fixed += (s.w!=null ? s.w : (s.t==null ? 0 : String(s.t).length));
  }
  let rem = Math.max(0, (w!=null ? w : fixed) - fixed);
  let per = fills ? Math.floor(rem/fills) : 0, extra = rem - per*fills, fi = 0;
  return segs.map((s, i) => {
    let t;
    if (s.fill){ const ww = per + (fi===fills-1 ? extra : 0); fi++; t = (s.ch||' ').repeat(ww); }
    else if (s.w!=null) t = pad(s.t, s.w, s.align);
    else t = s.t==null ? '' : String(s.t);
    const st = {};
    if (s.color) st.color = s.color;
    if (s.bg)    st.background = s.bg;
    if (s.bold)  st.fontWeight = 500;
    return React.createElement('span', { key:i, className:s.cls||'', style:st }, t);
  });
}

function Row({ segs, w=COLS, style, className, onClick, onMouseEnter }){
  return (
    <div className={'tui-row ' + (className||'')} style={style}
         onClick={onClick} onMouseEnter={onMouseEnter}>
      {buildSpans(segs, w)}
    </div>
  );
}

const bcOf = (focus, double) => focus ? C.orange : (double ? C.orangeD : C.line);

function TopBorder({ w=COLS, title, focus, double, right }){
  const [tl,tr,,,H,,,] = SETS[double?'double':'single'];
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
  const lj = set[6], rj = set[7], H = set[4];
  const bc = bcOf(focus, double);
  return <Row w={w} segs={[{ t:lj, color:bc },{ fill:true, ch:H, color:bc },{ t:rj, color:bc }]} />;
}
function BotBorder({ w=COLS, focus, double }){
  const [,,bl,br,H,,,] = SETS[double?'double':'single'];
  const bc = bcOf(focus, double);
  return <Row w={w} segs={[{ t:bl, color:bc },{ fill:true, ch:H, color:bc },{ t:br, color:bc }]} />;
}

/* A content row inside a frame: │ <pad> ...inner... <fill> │
   When `bg` is set the whole inner region is tinted (reverse-video selection);
   `ink` overrides text color on that fill. Borders keep their own color. */
function BodyRow({ inner, w=COLS, focus, double, bg, ink, onClick, onMouseEnter, className }){
  const V = SETS[double?'double':'single'][5];
  const bc = bcOf(focus, double);
  const lead = { t:' ' }, fill = { fill:true };
  let content = inner;
  if (bg){
    content = inner.map(s => ({ ...s, bg, color: ink || s.color }));
    lead.bg = bg; fill.bg = bg;
  }
  const segs = [{ t:V, color:bc }, lead, ...content, fill, { t:V, color:bc }];
  return <Row w={w} segs={segs} onClick={onClick} onMouseEnter={onMouseEnter} className={className} />;
}

/* Simple titled box for dialogs: pass `rows` = array of inner-seg-arrays. */
function Panel({ title, w, focus, double, rows, fillTo }){
  const body = rows.slice();
  if (fillTo) while (body.length < fillTo) body.push([]);
  return (
    <>
      <TopBorder w={w} title={title} focus={focus} double={double} />
      {body.map((inner,i)=>(
        <BodyRow key={i} inner={inner} w={w} focus={focus} double={double} />
      ))}
      <BotBorder w={w} focus={focus} double={double} />
    </>
  );
}

/* Center a fixed COLS×ROWS screen in the viewport, scaled to fit (letterboxed). */
function useScale(ref){
  React.useEffect(()=>{
    const el = ref.current; if(!el) return;
    const fit = ()=>{
      const nw = el.offsetWidth, nh = el.offsetHeight;
      if(!nw||!nh) return;
      const margin = 0.96;            // keep the frame off the very edge
      let s = Math.min(window.innerWidth/nw, window.innerHeight/nh) * margin;
      s = Math.min(s, 1.3);           // don't blow the text up on huge screens
      el.style.transform = `translate(-50%,-50%) scale(${s})`;
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
}

function Screen({ children }){
  const ref = React.useRef(null);
  useScale(ref);
  return (
    <div className="tui-stage">
      <div className="tui-screen" ref={ref}>{children}</div>
    </div>
  );
}

Object.assign(window, {
  COLS, ROWS, C, pad, buildSpans, Row, Panel,
  TopBorder, SepBorder, BotBorder, BodyRow, Screen, useScale,
});
