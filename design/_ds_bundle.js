/* @ds-bundle: {"format":4,"namespace":"CfdnsCloudflareDNSConsoleTUIDesignSystem_650d76","components":[{"name":"FunctionBar","sourcePath":"components/FunctionBar/FunctionBar.jsx"},{"name":"StatusBar","sourcePath":"components/StatusBar/StatusBar.jsx"},{"name":"TuiPanel","sourcePath":"components/TuiPanel/TuiPanel.jsx"}],"sourceHashes":{"components/FunctionBar/FunctionBar.jsx":"5bf66028bbc2","components/StatusBar/StatusBar.jsx":"986baf44c6ca","components/TuiPanel/TuiPanel.jsx":"7d64b4f31378","preview/tui-render.js":"0cdf99a1caf9","ui_kits/dns-client/BulkWizard.jsx":"f261a33cd635","ui_kits/dns-client/Chrome.jsx":"d80fa1545a6d","ui_kits/dns-client/Dialogs.jsx":"76875fbb3d2f","ui_kits/dns-client/Header.jsx":"ad4025d46292","ui_kits/dns-client/RecordsTable.jsx":"6d3e986137f3","ui_kits/dns-client/app.jsx":"a3f9161213cf","ui_kits/dns-client/data.js":"d3d7f232633f","ui_kits/dns-client/tui.jsx":"69b545d01d21","ui_kits/openadmin/AppChrome.jsx":"ff6534baf917","ui_kits/openadmin/ChatScreen.jsx":"78194d0b03d8","ui_kits/openadmin/HostDialogs.jsx":"fea124fe6907","ui_kits/openadmin/HostsScreen.jsx":"9969ce5acb6e","ui_kits/openadmin/ShellsScreen.jsx":"8154b7fa2721","ui_kits/openadmin/app.jsx":"c3246d542de0","ui_kits/openadmin/data.js":"5c27a0d0061a","ui_kits/openadmin/tui.jsx":"fdfeb9e0ee08"},"inlinedExternals":[],"unexposedExports":[]} */

(() => {

const __ds_ns = (window.CfdnsCloudflareDNSConsoleTUIDesignSystem_650d76 = window.CfdnsCloudflareDNSConsoleTUIDesignSystem_650d76 || {});

const __ds_scope = {};

(__ds_ns.__errors = __ds_ns.__errors || []);

// components/FunctionBar/FunctionBar.jsx
try { (() => {
/**
 * FunctionBar — the always-visible bottom action strip. Every action in a TUI has
 * an F-key; this renders the key caps and their labels, full width.
 *
 * Caps are Cloudflare orange, the active item inverts to a solid fill, and
 * destructive actions tint their label red. Items are clickable for mouse use.
 */
function FunctionBar({
  items = [],
  fontSize = 13,
  style
}) {
  return /*#__PURE__*/React.createElement("div", {
    style: {
      display: 'flex',
      background: 'var(--statusbar-bg)',
      fontSize,
      fontFamily: 'var(--font-mono)',
      overflow: 'hidden',
      whiteSpace: 'nowrap',
      borderTop: '1px solid var(--bg-base)',
      ...style
    }
  }, items.map((it, i) => /*#__PURE__*/React.createElement("span", {
    key: i,
    onClick: it.disabled ? undefined : it.onClick,
    title: it.title,
    style: {
      display: 'flex',
      alignItems: 'center',
      padding: '4px 0',
      cursor: it.disabled ? 'default' : 'pointer',
      opacity: it.disabled ? 0.4 : 1,
      userSelect: 'none'
    }
  }, /*#__PURE__*/React.createElement("span", {
    style: {
      padding: '0 1px 0 1.2ch',
      fontWeight: 500,
      color: it.active ? 'var(--orange-ink)' : 'var(--orange-bright)',
      background: it.active ? 'var(--orange)' : 'transparent'
    }
  }, it.keyLabel), /*#__PURE__*/React.createElement("span", {
    style: {
      padding: '0 1.2ch 0 .6ch',
      color: it.danger ? 'var(--red)' : 'var(--fg)'
    }
  }, it.label))));
}
Object.assign(__ds_scope, { FunctionBar });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/FunctionBar/FunctionBar.jsx", error: String((e && e.message) || e) }); }

// components/StatusBar/StatusBar.jsx
try { (() => {
/**
 * StatusBar — the single context row that sits directly above the FunctionBar.
 * Left: where you are. Middle: an optional hover/context hint (terminals report
 * mouse motion, so hovering a row can explain it here). Right: transient status
 * — green for success, red for errors, yellow for warnings.
 */
const TONE = {
  ok: 'var(--green)',
  err: 'var(--red)',
  warn: 'var(--yellow)',
  info: 'var(--blue)',
  '': 'var(--fg-muted)'
};
function StatusBar({
  left,
  hint,
  message,
  tone = '',
  fontSize = 13,
  style
}) {
  return /*#__PURE__*/React.createElement("div", {
    style: {
      display: 'flex',
      alignItems: 'center',
      gap: '2ch',
      background: 'var(--statusbar-bg)',
      padding: '3px 1ch',
      fontSize,
      fontFamily: 'var(--font-mono)',
      color: 'var(--fg-muted)',
      overflow: 'hidden',
      whiteSpace: 'nowrap',
      ...style
    }
  }, /*#__PURE__*/React.createElement("span", null, left), hint && /*#__PURE__*/React.createElement("span", {
    style: {
      color: 'var(--fg-faint)'
    }
  }, hint), /*#__PURE__*/React.createElement("span", {
    style: {
      marginLeft: 'auto',
      textAlign: 'right',
      color: TONE[tone] || TONE['']
    }
  }, message));
}
Object.assign(__ds_scope, { StatusBar });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/StatusBar/StatusBar.jsx", error: String((e && e.message) || e) }); }

// components/TuiPanel/TuiPanel.jsx
try { (() => {
const SETS = {
  single: {
    tl: '┌',
    tr: '┐',
    bl: '└',
    br: '┘',
    h: '─',
    v: '│',
    lj: '├',
    rj: '┤'
  },
  double: {
    tl: '╔',
    tr: '╗',
    bl: '╚',
    br: '╝',
    h: '═',
    v: '║',
    lj: '╠',
    rj: '╣'
  }
};
const ROW_STYLE = {
  whiteSpace: 'pre',
  fontFamily: 'var(--font-mono)',
  lineHeight: 1.55,
  fontVariantLigatures: 'none',
  fontFeatureSettings: '"calt" 0'
};

/**
 * TuiPanel — a titled box-drawing frame, the universal container of this design
 * system. Draws on a monospace character grid: the title is inset into the top
 * rule, content is padded one cell in, and corners are box glyphs (radius is
 * always 0 — a terminal has no rounded pixels).
 *
 * Border color encodes focus: dim by default, Cloudflare orange when `focus`,
 * and `double` switches to ╔═╗ for blocking modals.
 */
function TuiPanel({
  title,
  right,
  rows = [],
  width = 64,
  focus = false,
  double = false,
  minRows = 0,
  fontSize = 15,
  style,
  children
}) {
  const S = double ? SETS.double : SETS.single;
  const bc = focus ? 'var(--orange)' : double ? 'var(--orange-dim)' : 'var(--line)';
  const rowStyle = {
    ...ROW_STYLE,
    fontSize
  };
  const body = rows.slice();
  while (body.length < minRows) body.push('');
  const pad = (s, w) => {
    s = s == null ? '' : String(s);
    return s.length > w ? s.slice(0, w) : s + ' '.repeat(w - s.length);
  };
  const lead = S.tl + S.h + (title ? ' ' : '');
  const rightStr = right ? ` ${right} ` : '';
  const dashes = Math.max(0, width - lead.length - (title ? title.length + 1 : 0) - rightStr.length - 1);
  return /*#__PURE__*/React.createElement("div", {
    style: {
      background: 'var(--bg-base)',
      color: 'var(--fg)',
      display: 'inline-block',
      ...style
    }
  }, /*#__PURE__*/React.createElement("div", {
    style: rowStyle
  }, /*#__PURE__*/React.createElement("span", {
    style: {
      color: bc
    }
  }, lead), title && /*#__PURE__*/React.createElement("span", {
    style: {
      color: focus ? 'var(--fg-bright)' : 'var(--fg-muted)',
      fontWeight: 500
    }
  }, title), /*#__PURE__*/React.createElement("span", {
    style: {
      color: bc
    }
  }, (title ? ' ' : '') + S.h.repeat(dashes)), right && /*#__PURE__*/React.createElement("span", {
    style: {
      color: 'var(--fg-faint)'
    }
  }, rightStr), /*#__PURE__*/React.createElement("span", {
    style: {
      color: bc
    }
  }, S.tr)), body.map((r, i) => /*#__PURE__*/React.createElement("div", {
    key: i,
    style: rowStyle
  }, /*#__PURE__*/React.createElement("span", {
    style: {
      color: bc
    }
  }, S.v), /*#__PURE__*/React.createElement("span", null, ' ' + pad(r, width - 3)), /*#__PURE__*/React.createElement("span", {
    style: {
      color: bc
    }
  }, S.v))), children, /*#__PURE__*/React.createElement("div", {
    style: rowStyle
  }, /*#__PURE__*/React.createElement("span", {
    style: {
      color: bc
    }
  }, S.bl + S.h.repeat(width - 2) + S.br)));
}
Object.assign(__ds_scope, { TuiPanel });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/TuiPanel/TuiPanel.jsx", error: String((e && e.message) || e) }); }

// preview/tui-render.js
try { (() => {
/* tui-render.js — tiny helpers to draw aligned, colored terminal UI in HTML.
   Renders each "row" as a <div class="tui-row"> (white-space: pre) so box-drawing
   glyphs in fixed columns line up vertically across rows. Coloring is done with
   inline <span class>, which does not disturb the character grid. */
(function (g) {
  function esc(s) {
    return String(s).replace(/[&<>]/g, c => ({
      '&': '&amp;',
      '<': '&lt;',
      '>': '&gt;'
    })[c]);
  }

  // pad/truncate a string to an exact cell width
  function pad(s, w, align) {
    s = String(s);
    if (w == null) return s;
    if (s.length > w) return s.slice(0, w);
    const gap = w - s.length;
    if (align === 'right') return ' '.repeat(gap) + s;
    if (align === 'center') return ' '.repeat(gap >> 1) + s + ' '.repeat(gap - (gap >> 1));
    return s + ' '.repeat(gap);
  }

  // one colored segment → HTML; optionally padded to width w
  function seg(text, cls, w, align) {
    const t = w != null ? pad(text, w, align) : String(text);
    return cls ? `<span class="${cls}">${esc(t)}</span>` : esc(t);
  }

  // assemble a row from segments (strings already produced by seg())
  function line(...segs) {
    return `<div class="tui-row">${segs.join('')}</div>`;
  }

  // a horizontal rule of given inner width, with corner/junction chars
  function rule(inner, left, right, fill, cls) {
    return line(seg((left || '├') + (fill || '─').repeat(inner) + (right || '┤'), cls || 'tui-line'));
  }
  g.esc = esc;
  g.pad = pad;
  g.seg = seg;
  g.line = line;
  g.rule = rule;
})(window);
})(); } catch (e) { __ds_ns.__errors.push({ path: "preview/tui-render.js", error: String((e && e.message) || e) }); }

// ui_kits/dns-client/BulkWizard.jsx
try { (() => {
/* BulkWizard.jsx — the signature feature. Step 1 configures a type, proxy state,
   a seed hostname containing a number (e.g. s1000.mydomain.com) and a list of IPs.
   Step 2 previews the expansion  s1000 => ip1 · s1001 => ip2 · …  before adding. */

const DWB = 74;

/* split the seed hostname around its first digit run, preserving zero-pad width */
function parseSeed(host) {
  const m = /^(\D*)(\d+)(.*)$/.exec(host || '');
  if (!m) return null;
  return {
    prefix: m[1],
    num: parseInt(m[2], 10),
    width: m[2].length,
    suffix: m[3]
  };
}
function ipLines(ips) {
  return (ips || '').split('\n').map(s => s.trim()).filter(Boolean);
}
function expand(seed, ips) {
  const p = parseSeed(seed);
  const list = ipLines(ips);
  if (!p) return [];
  return list.map((ip, i) => ({
    name: p.prefix + String(p.num + i).padStart(p.width, '0') + p.suffix,
    content: ip
  }));
}
function BulkWizard(props) {
  const {
    st,
    meta,
    focus,
    onFocusField,
    onCycleType,
    onToggleProxy,
    onBack,
    onNext,
    onConfirm,
    onCancel
  } = props;
  const f = id => focus === id;
  const proxiable = meta.proxiable;
  if (st.step === 1) {
    const lines = ipLines(st.ips);
    const TA_ROWS = 6;
    const rows = [];
    rows.push([]);
    rows.push(fieldInner('Type', selectSegs(st.type, 16, f('type')), f('type')));
    rows.push([]);
    if (proxiable) {
      rows.push(radioInner('Proxy', st.proxied, f('proxy')));
      rows.push([]);
    }
    rows.push(fieldInner('Seed host', wellSegs(st.first, 's1000.' + ZONE, 50, f('first')), f('first')));
    rows.push([{
      t: ' '.repeat(LABELW + 2)
    }, {
      t: 'the number is incremented per IP below',
      color: C.faint
    }]);
    rows.push([]);
    rows.push([{
      t: 'IP list',
      w: LABELW,
      align: 'right',
      color: f('ips') ? C.orange : C.muted
    }, {
      t: '  '
    }, {
      t: f('ips') ? 'one IP per line  ▼' : 'one IP per line',
      color: C.faint
    }]);
    // textarea region
    for (let i = 0; i < TA_ROWS; i++) {
      const line = lines[i] || '';
      const isCur = f('ips') && i === Math.min(lines.length, TA_ROWS - 1) && lines.length < TA_ROWS;
      const lineCur = f('ips') && i === lines.length;
      const txt = line || '';
      const used = 1 + txt.length + (lineCur ? 1 : 0);
      const seg = [{
        t: '  '
      }, {
        t: ' ',
        bg: INSET
      }, ...(txt ? [{
        t: txt,
        bg: INSET,
        color: C.fg
      }] : []), ...(lineCur ? [{
        t: '█',
        bg: INSET,
        color: C.orange,
        cls: 'tui-cursor'
      }] : []), {
        t: ' '.repeat(Math.max(0, 52 - used)),
        bg: INSET
      }];
      rows.push(seg);
    }
    rows.push([{
      t: '  '
    }, {
      t: `${lines.length} IP${lines.length === 1 ? '' : 's'} entered`,
      color: C.muted
    }]);
    rows.push([]);
    rows.push([{
      t: 'Tab next field   Enter in IP list = new line   Esc cancel',
      color: C.faint
    }]);
    rows.push([]);
    rows.push([{
      fill: true
    }, {
      t: ' Next ▸ ',
      bg: C.orange,
      color: C.ink,
      bold: true
    }, {
      t: '   '
    }, {
      t: '[ Cancel ]',
      color: C.fg
    }, {
      t: ' '
    }]);
    rows.push([]);
    return /*#__PURE__*/React.createElement(DialogOverlay, {
      onBackdrop: onCancel
    }, /*#__PURE__*/React.createElement(TopBorder, {
      w: DWB,
      title: "Bulk Add \xB7 1 of 2 \u2014 Configure",
      focus: true
    }), rows.map((inner, i) => /*#__PURE__*/React.createElement(BodyRow, {
      key: i,
      w: DWB,
      inner: inner,
      focus: true,
      onClick: i === 2 && false ? null : i === 1 ? () => onCycleType(1) : proxiable && i === 3 ? onToggleProxy : i === rows.length - 2 ? onNext : null,
      className: i === 1 || proxiable && i === 3 || i === rows.length - 2 ? 'tui-clickrow' : ''
    })), /*#__PURE__*/React.createElement(BotBorder, {
      w: DWB,
      focus: true
    }));
  }

  // step 2 — preview
  const pairs = expand(st.first, st.ips);
  const SHOW = 9;
  const shown = pairs.slice(0, SHOW);
  const extra = pairs.length - SHOW;
  const rows = [];
  rows.push([]);
  rows.push([{
    t: 'Will create ',
    color: C.fg
  }, {
    t: String(pairs.length),
    color: C.orange,
    bold: true
  }, {
    t: ` ${st.type} record${pairs.length === 1 ? '' : 's'}`,
    color: C.fg
  }, ...(proxiable ? [{
    t: '   '
  }, {
    t: st.proxied ? '▲ proxied' : '○ dns-only',
    color: st.proxied ? C.orange : C.muted
  }] : [])]);
  rows.push([]);
  shown.forEach(p => {
    rows.push([{
      t: p.name,
      w: 34,
      color: C.fg
    }, {
      t: '=> ',
      color: C.faint
    }, {
      t: p.content,
      color: C.blue
    }]);
  });
  if (extra > 0) rows.push([{
    t: `  … and ${extra} more`,
    color: C.faint
  }]);
  if (pairs.length === 0) rows.push([{
    t: '  Nothing to expand — check the seed host and IP list.',
    color: C.yellow
  }]);
  rows.push([]);
  rows.push([{
    fill: true
  }, {
    t: '[ ◂ Back ]',
    color: C.fg
  }, {
    t: '   '
  }, {
    t: ` Add ${pairs.length} Record${pairs.length === 1 ? '' : 's'} `,
    bg: pairs.length ? C.orange : C.disabled,
    color: C.ink,
    bold: true
  }, {
    t: '   '
  }, {
    t: '[ Cancel ]',
    color: C.fg
  }, {
    t: ' '
  }]);
  rows.push([]);
  const lastIdx = rows.length - 2;
  return /*#__PURE__*/React.createElement(DialogOverlay, {
    onBackdrop: onCancel
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: DWB,
    title: "Bulk Add \xB7 2 of 2 \u2014 Preview",
    focus: true
  }), rows.map((inner, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: DWB,
    inner: inner,
    focus: true,
    onClick: i === lastIdx ? pairs.length ? onConfirm : null : null,
    className: i === lastIdx && pairs.length ? 'tui-clickrow' : ''
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: DWB,
    focus: true
  }));
}
Object.assign(window, {
  BulkWizard,
  parseSeed,
  expand,
  ipLines
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/dns-client/BulkWizard.jsx", error: String((e && e.message) || e) }); }

// ui_kits/dns-client/Chrome.jsx
try { (() => {
/* Chrome.jsx — StatusBar (context + transient message) and FunctionBar
   (context-sensitive F-key actions). Both are full-width bands. */

function StatusBar({
  zone,
  left,
  message,
  messageType,
  spinner
}) {
  const mc = {
    ok: 'tui-ok',
    err: 'tui-err',
    warn: 'tui-warn',
    info: 'tui-info'
  }[messageType] || '';
  return /*#__PURE__*/React.createElement("div", {
    className: "tui-statusbar"
  }, /*#__PURE__*/React.createElement("span", {
    className: "sb-zone"
  }, "zone ", /*#__PURE__*/React.createElement("b", null, zone)), /*#__PURE__*/React.createElement("span", {
    className: "sb-left"
  }, left), /*#__PURE__*/React.createElement("span", {
    className: "sb-legend"
  }, /*#__PURE__*/React.createElement("span", {
    className: "sb-prx"
  }, "\u25B2"), " proxied\xA0\xA0", /*#__PURE__*/React.createElement("span", {
    className: "sb-dns"
  }, "\u25CB"), " dns-only"), /*#__PURE__*/React.createElement("span", {
    className: 'sb-msg ' + mc
  }, spinner && /*#__PURE__*/React.createElement(Spinner, null), message));
}
function Spinner() {
  const frames = '⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏';
  const [i, setI] = React.useState(0);
  React.useEffect(() => {
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (reduce) return;
    const id = setInterval(() => setI(x => (x + 1) % frames.length), 80);
    return () => clearInterval(id);
  }, []);
  return /*#__PURE__*/React.createElement("span", {
    className: "sb-spin"
  }, frames[i], "\xA0");
}
function FunctionBar({
  items
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "tui-fnbar"
  }, items.map((it, i) => /*#__PURE__*/React.createElement("span", {
    key: i,
    className: 'fk' + (it.active ? ' fk-active' : ''),
    onClick: it.onClick
  }, /*#__PURE__*/React.createElement("span", {
    className: "fk-cap"
  }, it.key), /*#__PURE__*/React.createElement("span", {
    className: 'fk-lab' + (it.danger ? ' fk-danger' : '')
  }, it.label))));
}
Object.assign(window, {
  StatusBar,
  FunctionBar,
  Spinner
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/dns-client/Chrome.jsx", error: String((e && e.message) || e) }); }

// ui_kits/dns-client/Dialogs.jsx
try { (() => {
/* Dialogs.jsx — modal overlays drawn as char-grid panels:
   shared field/select/radio/button builders, AddEditDialog, DeleteDialog, HelpOverlay.
   Text editing & navigation are driven by the app's global key handler; rows are
   also clickable for mouse use. */

const INSET = 'var(--bg-inset)';
const DW = 66; // dialog width in cells
const LABELW = 13;
function DialogOverlay({
  children,
  onBackdrop
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "tui-overlay",
    onClick: onBackdrop
  }, /*#__PURE__*/React.createElement("div", {
    className: "tui-dialog",
    onClick: e => e.stopPropagation()
  }, children));
}

/* ---- field builders (return inner-seg arrays for a BodyRow) ---- */
function wellSegs(value, ph, width, focused) {
  const showPh = !value && !focused;
  const txt = value || (showPh ? ph : '');
  const used = 1 + txt.length + (focused ? 1 : 0);
  const trail = Math.max(0, width - used);
  const segs = [{
    t: ' ',
    bg: INSET
  }];
  if (txt) segs.push({
    t: txt,
    bg: INSET,
    color: value ? C.fg : C.faint
  });
  if (focused) segs.push({
    t: '█',
    bg: INSET,
    color: C.orange,
    cls: 'tui-cursor'
  });
  segs.push({
    t: ' '.repeat(trail),
    bg: INSET
  });
  return segs;
}
function selectSegs(value, width, focused, disabled) {
  const txt = value,
    used = 1 + txt.length,
    trail = Math.max(0, width - used - 2);
  return [{
    t: ' ',
    bg: INSET
  }, {
    t: txt,
    bg: INSET,
    color: disabled ? C.disabled : C.fg
  }, {
    t: ' '.repeat(trail),
    bg: INSET
  }, {
    t: '▾ ',
    bg: INSET,
    color: disabled ? C.disabled : focused ? C.orange : C.muted
  }];
}
function labelSeg(label, focused) {
  return {
    t: label,
    w: LABELW,
    align: 'right',
    color: focused ? C.orange : C.muted
  };
}
function fieldInner(label, segs, focused) {
  return [labelSeg(label, focused), {
    t: '  '
  }, ...segs];
}
function radioInner(label, proxied, focused) {
  return [labelSeg(label, focused), {
    t: '  '
  }, {
    t: proxied ? '(•)' : '( )',
    color: proxied ? C.orange : C.muted
  }, {
    t: ' ▲ Proxied',
    color: proxied ? C.orange : C.fg
  }, {
    t: '     '
  }, {
    t: !proxied ? '(•)' : '( )',
    color: !proxied ? C.orange : C.muted
  }, {
    t: ' ○ DNS only',
    color: !proxied ? C.bright : C.fg
  }];
}
/* default action rendered reverse-video; returns inner segs, right-aligned */
function buttonInner(primaryLabel) {
  return [{
    fill: true
  }, {
    t: ` ${primaryLabel} `,
    bg: C.orange,
    color: C.ink,
    bold: true
  }, {
    t: '   '
  }, {
    t: '[ Cancel ]',
    color: C.fg
  }, {
    t: '  '
  }];
}

/* ============================ Add / Edit ============================ */
function AddEditDialog({
  mode,
  form,
  focus,
  meta,
  onFocusField,
  onCycleType,
  onToggleProxy,
  onCycleTTL,
  onSave,
  onCancel
}) {
  const title = mode === 'edit' ? 'Edit DNS Record' : 'Add DNS Record';
  const f = id => focus === id;
  const proxiable = meta.proxiable;
  const ttlDisabled = proxiable && form.proxied;
  const rows = [];
  const push = (inner, onClick) => rows.push({
    inner,
    onClick
  });
  push([]);
  push(fieldInner('Type', selectSegs(form.type, 16, f('type')), f('type')), () => onCycleType(1));
  push([]);
  push(fieldInner('Name', wellSegs(form.name, 'subdomain or @', 40, f('name')), f('name')), () => onFocusField('name'));
  push([]);
  push(fieldInner(meta.label[0].toUpperCase() + meta.label.slice(1), wellSegs(form.content, meta.ph, 44, f('content')), f('content')), () => onFocusField('content'));
  if (meta.priority) {
    push([]);
    push(fieldInner('Priority', wellSegs(String(form.priority ?? ''), '10', 10, f('priority')), f('priority')), () => onFocusField('priority'));
  }
  push([]);
  push(fieldInner('TTL', selectSegs(ttlDisabled ? 'Auto (proxied)' : ttlLabel(form.ttl), 18, f('ttl'), ttlDisabled), f('ttl')), () => {
    if (!ttlDisabled) onCycleTTL(1);
  });
  if (proxiable) {
    push([]);
    push(radioInner('Proxy', form.proxied, f('proxy')), () => onToggleProxy());
  }
  push([]);
  push([{
    t: 'Tab next  ←/→ change  Enter save  Esc cancel',
    color: C.faint
  }]);
  push([]);
  push(buttonInner(mode === 'edit' ? 'Save' : 'Add Record'));
  push([]);
  return /*#__PURE__*/React.createElement(DialogOverlay, {
    onBackdrop: onCancel
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: DW,
    title: title,
    focus: true
  }), rows.map((r, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: DW,
    inner: r.inner,
    focus: true,
    onClick: i === rows.length - 2 ? onSave : r.onClick,
    className: r.onClick || i === rows.length - 2 ? 'tui-clickrow' : ''
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: DW,
    focus: true
  }));
}

/* ============================ Delete confirm ============================ */
function DeleteDialog({
  rec,
  onConfirm,
  onCancel
}) {
  const DWX = 56;
  const rows = [[], [{
    t: 'Delete ',
    color: C.fg
  }, {
    t: rec.type + ' ',
    color: TYPE_COLOR[rec.type] || C.fg
  }, {
    t: '"' + rec.name + '"',
    color: C.fg
  }, {
    t: ' ?',
    color: C.fg
  }], [{
    t: rec.content,
    color: C.muted
  }], [], [{
    t: 'This record cannot be recovered.',
    color: C.muted
  }], [], [{
    fill: true
  }, {
    t: ' Delete ',
    bg: C.red,
    color: '#1a0000',
    bold: true
  }, {
    t: '   '
  }, {
    t: '[ Cancel ]',
    color: C.fg
  }, {
    t: ' '
  }], []];
  return /*#__PURE__*/React.createElement(DialogOverlay, {
    onBackdrop: onCancel
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: DWX,
    title: "Confirm Delete",
    double: true
  }), rows.map((inner, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: DWX,
    inner: inner,
    double: true,
    onClick: i === 6 ? onConfirm : null,
    className: i === 6 ? 'tui-clickrow' : ''
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: DWX,
    double: true
  }));
}

/* ============================ Help ============================ */
function HelpOverlay({
  onCancel
}) {
  const DWH = 60;
  const k = (key, desc) => [{
    t: key,
    w: 10,
    color: C.orangeB
  }, {
    t: desc,
    color: C.fg
  }];
  const rows = [[], [{
    t: 'NAVIGATION',
    cls: 'tui-colhead'
  }], k('↑ ↓', 'move selection'), k('Home End', 'jump to first / last'), k('Enter', 'edit selected record'), [], [{
    t: 'ACTIONS',
    cls: 'tui-colhead'
  }], k('F2', 'add a new record'), k('F3', 'edit selected'), k('F4', 'toggle proxy (A · AAAA · CNAME)'), k('F5', 'bulk add — sequential hostnames'), k('F6', 'filter records'), k('F8', 'delete selected'), k('F10 / q', 'quit'), [], [{
    t: 'Esc closes any dialog · Tab moves between fields',
    color: C.faint
  }], []];
  return /*#__PURE__*/React.createElement(DialogOverlay, {
    onBackdrop: onCancel
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: DWH,
    title: "Help \xB7 Key Bindings",
    focus: true
  }), rows.map((inner, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: DWH,
    inner: inner,
    focus: true
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: DWH,
    focus: true
  }));
}
Object.assign(window, {
  DialogOverlay,
  AddEditDialog,
  DeleteDialog,
  HelpOverlay,
  wellSegs,
  selectSegs,
  fieldInner,
  radioInner,
  INSET,
  DW,
  LABELW
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/dns-client/Dialogs.jsx", error: String((e && e.message) || e) }); }

// ui_kits/dns-client/Header.jsx
try { (() => {
/* Header.jsx — top band: solid brand block + wordmark on the left, zone & account
   on the right, closed by a full-width rule. A TUI can't draw logo art, so the
   mark is an honest solid orange rectangle sized to the two-line wordmark. */

function Header({
  zone,
  count,
  proxiedCount
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "tui-header"
  }, /*#__PURE__*/React.createElement("div", {
    className: "hdr-row"
  }, /*#__PURE__*/React.createElement("div", {
    className: "hdr-left"
  }, /*#__PURE__*/React.createElement("div", {
    className: "hdr-mark"
  }), /*#__PURE__*/React.createElement("div", {
    className: "hdr-word"
  }, /*#__PURE__*/React.createElement("div", {
    className: "hdr-name"
  }, "cfdns"), /*#__PURE__*/React.createElement("div", {
    className: "hdr-tag"
  }, "Cloudflare DNS Console"))), /*#__PURE__*/React.createElement("div", {
    className: "hdr-right"
  }, /*#__PURE__*/React.createElement("div", {
    className: "hdr-zone"
  }, "zone\xA0", /*#__PURE__*/React.createElement("b", null, zone), "\xA0", /*#__PURE__*/React.createElement("span", {
    className: "hdr-car"
  }, "\u25BE")), /*#__PURE__*/React.createElement("div", {
    className: "hdr-acct"
  }, "ops@", zone, " \xB7 Free plan"), /*#__PURE__*/React.createElement("div", {
    className: "hdr-count"
  }, count, " records \xB7 ", /*#__PURE__*/React.createElement("span", {
    className: "hdr-prx"
  }, proxiedCount, " proxied")))), /*#__PURE__*/React.createElement(Row, {
    w: COLS,
    segs: [{
      fill: true,
      ch: '─',
      color: C.line
    }]
  }));
}
window.Header = Header;
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/dns-client/Header.jsx", error: String((e && e.message) || e) }); }

// ui_kits/dns-client/RecordsTable.jsx
try { (() => {
/* RecordsTable.jsx — the central scrollable DNS records grid.
   Columns (inner width 101): [marker 2][TYPE 6][NAME 20][CONTENT 58][TTL 9][PRX 6] */

const COL = {
  mark: 2,
  type: 6,
  name: 20,
  content: 58,
  ttl: 9,
  prx: 6
};
const VISIBLE = 15;
function proxySeg(rec) {
  const meta = TYPE_META[rec.type];
  if (!meta || !meta.proxiable) return {
    t: '·',
    w: COL.prx,
    align: 'center',
    color: C.disabled
  };
  return rec.proxied ? {
    t: '▲',
    w: COL.prx,
    align: 'center',
    color: C.orange
  } : {
    t: '○',
    w: COL.prx,
    align: 'center',
    color: C.muted
  };
}
function contentText(rec) {
  if (rec.priority != null) return rec.priority + '  ' + rec.content;
  return rec.content;
}
function RecordsTable({
  records,
  selected,
  onSelect,
  focused,
  height = VISIBLE
}) {
  // scroll window
  let start = 0;
  if (records.length > height) {
    start = Math.min(Math.max(0, selected - (height >> 1)), records.length - height);
  }
  const window_ = records.slice(start, start + height);
  const more = records.length - height;
  const headSegs = [{
    t: '',
    w: COL.mark
  }, {
    t: 'TYPE',
    w: COL.type,
    cls: 'tui-colhead'
  }, {
    t: 'NAME',
    w: COL.name,
    cls: 'tui-colhead'
  }, {
    t: 'CONTENT',
    w: COL.content,
    cls: 'tui-colhead'
  }, {
    t: 'TTL',
    w: COL.ttl,
    cls: 'tui-colhead'
  }, {
    t: 'PRX',
    w: COL.prx,
    cls: 'tui-colhead',
    align: 'center'
  }];
  const rightInfo = records.length + (more > 0 ? ` · ${start + 1}\u2013${start + height}` : '');
  return /*#__PURE__*/React.createElement("div", {
    className: "tui-table"
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: COLS,
    title: "Records",
    focus: focused,
    right: rightInfo
  }), /*#__PURE__*/React.createElement(BodyRow, {
    w: COLS,
    inner: headSegs,
    focus: focused
  }), /*#__PURE__*/React.createElement(SepBorder, {
    w: COLS,
    focus: focused
  }), window_.map((rec, i) => {
    const idx = start + i;
    const isSel = idx === selected;
    const inner = [{
      t: isSel ? '▸' : ' ',
      w: COL.mark,
      color: C.orange
    }, {
      t: rec.type,
      w: COL.type,
      color: TYPE_COLOR[rec.type] || C.fg
    }, {
      t: rec.name,
      w: COL.name,
      color: rec.name === '@' ? C.muted : C.fg
    }, {
      t: contentText(rec),
      w: COL.content,
      color: C.fg
    }, {
      t: ttlLabel(rec.ttl),
      w: COL.ttl,
      color: C.muted
    }, proxySeg(rec)];
    return /*#__PURE__*/React.createElement(BodyRow, {
      key: rec.id,
      w: COLS,
      inner: inner,
      focus: focused,
      bg: isSel ? focused ? C.orange : 'var(--bg-sel)' : null,
      ink: isSel && focused ? C.ink : null,
      onClick: () => onSelect(idx),
      className: "tui-clickrow"
    });
  }), Array.from({
    length: Math.max(0, height - window_.length)
  }).map((_, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: 'pad' + i,
    w: COLS,
    inner: [],
    focus: focused
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: COLS,
    focus: focused
  }));
}
window.RecordsTable = RecordsTable;
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/dns-client/RecordsTable.jsx", error: String((e && e.message) || e) }); }

// ui_kits/dns-client/app.jsx
try { (() => {
/* app.jsx — cfdns application: state, global keyboard handling, and wiring. */

const TTL_CYCLE = [1, 60, 300, 1800, 3600, 86400];
function useMessage() {
  const [msg, setMsg] = React.useState({
    text: '↑↓ select · Enter edit · F2 add · F5 bulk · F1 help',
    type: ''
  });
  const t = React.useRef();
  const flash = (text, type = 'ok') => {
    setMsg({
      text,
      type
    });
    clearTimeout(t.current);
    t.current = setTimeout(() => setMsg({
      text: 'Ready.',
      type: ''
    }), 3200);
  };
  return [msg, flash];
}
function visibleFormFields(meta) {
  const v = ['type', 'name', 'content'];
  if (meta.priority) v.push('priority');
  v.push('ttl');
  if (meta.proxiable) v.push('proxy');
  return v;
}
function visibleBulkFields(meta) {
  const v = ['type'];
  if (meta.proxiable) v.push('proxy');
  v.push('first', 'ips');
  return v;
}
function App() {
  const [records, setRecords] = React.useState(SAMPLE_RECORDS);
  const [selected, setSelected] = React.useState(0);
  const [mode, setMode] = React.useState('list'); // list|add|edit|delete|bulk|help|filter
  const [form, setForm] = React.useState(null);
  const [formFocus, setFormFocus] = React.useState('name');
  const [bulk, setBulk] = React.useState(null);
  const [filter, setFilter] = React.useState('');
  const [msg, flash] = useMessage();
  const filtered = React.useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return records;
    return records.filter(r => (r.type + ' ' + r.name + ' ' + r.content).toLowerCase().includes(q));
  }, [records, filter]);
  const sel = filtered[Math.min(selected, filtered.length - 1)] || null;
  const proxiedCount = records.filter(r => r.proxied).length;
  const clampSel = i => setSelected(Math.max(0, Math.min(i, filtered.length - 1)));

  /* ---------------- actions ---------------- */
  const openAdd = () => {
    setForm({
      type: 'A',
      name: '',
      content: '',
      priority: 10,
      ttl: 1,
      proxied: true
    });
    setFormFocus('name');
    setMode('add');
  };
  const openEdit = () => {
    if (!sel) return;
    setForm({
      ...sel,
      priority: sel.priority ?? 10
    });
    setFormFocus('name');
    setMode('edit');
  };
  const cycleType = dir => {
    setForm(f => {
      const idx = RECORD_TYPES.findIndex(t => t.type === f.type);
      const nt = RECORD_TYPES[(idx + dir + RECORD_TYPES.length) % RECORD_TYPES.length];
      return {
        ...f,
        type: nt.type,
        proxied: nt.proxiable ? f.proxied : false
      };
    });
  };
  const cycleTTL = dir => setForm(f => {
    const i = TTL_CYCLE.indexOf(f.ttl);
    const ni = (i + dir + TTL_CYCLE.length) % TTL_CYCLE.length;
    return {
      ...f,
      ttl: TTL_CYCLE[ni]
    };
  });
  const toggleFormProxy = () => setForm(f => ({
    ...f,
    proxied: !f.proxied,
    ttl: !f.proxied ? 1 : f.ttl
  }));
  const saveForm = () => {
    if (!form.name.trim() || !form.content.trim()) {
      flash('Name and content are required.', 'err');
      return;
    }
    const meta = TYPE_META[form.type];
    const rec = {
      id: form.id || rid(),
      type: form.type,
      name: form.name.trim(),
      content: form.content.trim(),
      ttl: meta.proxiable && form.proxied ? 1 : form.ttl,
      proxied: meta.proxiable ? form.proxied : false
    };
    if (meta.priority) rec.priority = parseInt(form.priority, 10) || 0;
    if (mode === 'edit') {
      setRecords(rs => rs.map(r => r.id === rec.id ? rec : r));
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
    if (!meta.proxiable) {
      flash(`${sel.type} records cannot be proxied.`, 'warn');
      return;
    }
    setRecords(rs => rs.map(r => r.id === sel.id ? {
      ...r,
      proxied: !r.proxied,
      ttl: !r.proxied ? 1 : r.ttl
    } : r));
    flash(`${sel.name}: ${sel.proxied ? 'DNS only' : 'proxied'}.`, 'ok');
  };
  const confirmDelete = () => {
    if (!sel) return;
    setRecords(rs => rs.filter(r => r.id !== sel.id));
    flash(`Deleted ${sel.type} ${sel.name}.`, 'ok');
    setMode('list');
    clampSel(selected);
  };
  const openBulk = () => {
    setBulk({
      step: 1,
      type: 'A',
      proxied: true,
      first: 's1000.' + ZONE,
      ips: '',
      focus: 'first'
    });
    setMode('bulk');
  };
  const bulkCycleType = dir => setBulk(b => {
    const idx = RECORD_TYPES.findIndex(t => t.type === b.type);
    const nt = RECORD_TYPES[(idx + dir + RECORD_TYPES.length) % RECORD_TYPES.length];
    return {
      ...b,
      type: nt.type,
      proxied: nt.proxiable ? b.proxied : false
    };
  });
  const bulkConfirm = () => {
    const pairs = expand(bulk.first, bulk.ips);
    if (!pairs.length) {
      flash('Nothing to add.', 'warn');
      return;
    }
    const meta = TYPE_META[bulk.type];
    const recs = pairs.map(p => ({
      id: rid(),
      type: bulk.type,
      name: p.name,
      content: p.content,
      ttl: meta.proxiable && bulk.proxied ? 1 : 1,
      proxied: meta.proxiable ? bulk.proxied : false
    }));
    setRecords(rs => [...rs, ...recs]);
    flash(`Added ${recs.length} ${bulk.type} records.`, 'ok');
    setMode('list');
  };

  /* ---------------- keyboard ---------------- */
  React.useEffect(() => {
    const onKey = e => {
      const k = e.key;
      // global
      if (k === 'F1') {
        e.preventDefault();
        setMode(m => m === 'help' ? 'list' : 'help');
        return;
      }
      if (k === 'F10') {
        e.preventDefault();
        flash('Quit — close the tab. (demo)', 'warn');
        return;
      }
      if (mode === 'list') {
        if (k === 'ArrowDown') {
          e.preventDefault();
          clampSel(selected + 1);
        } else if (k === 'ArrowUp') {
          e.preventDefault();
          clampSel(selected - 1);
        } else if (k === 'Home') {
          e.preventDefault();
          clampSel(0);
        } else if (k === 'End') {
          e.preventDefault();
          clampSel(filtered.length - 1);
        } else if (k === 'Enter') {
          e.preventDefault();
          openEdit();
        } else if (k === 'F2') {
          e.preventDefault();
          openAdd();
        } else if (k === 'F3') {
          e.preventDefault();
          openEdit();
        } else if (k === 'F4') {
          e.preventDefault();
          toggleProxy();
        } else if (k === 'F5') {
          e.preventDefault();
          openBulk();
        } else if (k === 'F6' || k === '/') {
          e.preventDefault();
          setMode('filter');
        } else if (k === 'F8' || k === 'Delete') {
          e.preventDefault();
          if (sel) setMode('delete');
        }
        return;
      }
      if (mode === 'add' || mode === 'edit') {
        const meta = TYPE_META[form.type];
        const vf = visibleFormFields(meta);
        if (k === 'Escape') {
          e.preventDefault();
          setMode('list');
          return;
        }
        if (k === 'Enter') {
          e.preventDefault();
          saveForm();
          return;
        }
        if (k === 'Tab') {
          e.preventDefault();
          const i = vf.indexOf(formFocus);
          setFormFocus(vf[(i + (e.shiftKey ? -1 : 1) + vf.length) % vf.length]);
          return;
        }
        if (k === 'ArrowDown') {
          e.preventDefault();
          const i = vf.indexOf(formFocus);
          setFormFocus(vf[(i + 1) % vf.length]);
          return;
        }
        if (k === 'ArrowUp') {
          e.preventDefault();
          const i = vf.indexOf(formFocus);
          setFormFocus(vf[(i - 1 + vf.length) % vf.length]);
          return;
        }
        if (formFocus === 'type') {
          if (k === 'ArrowRight') {
            e.preventDefault();
            cycleType(1);
          } else if (k === 'ArrowLeft') {
            e.preventDefault();
            cycleType(-1);
          }
          return;
        }
        if (formFocus === 'ttl') {
          if (!(meta.proxiable && form.proxied)) {
            if (k === 'ArrowRight') {
              e.preventDefault();
              cycleTTL(1);
            } else if (k === 'ArrowLeft') {
              e.preventDefault();
              cycleTTL(-1);
            }
          }
          return;
        }
        if (formFocus === 'proxy') {
          if (k === 'ArrowRight' || k === 'ArrowLeft' || k === ' ') {
            e.preventDefault();
            toggleFormProxy();
          }
          return;
        }
        // text fields: name, content, priority
        if (k === 'Backspace') {
          e.preventDefault();
          setForm(f => ({
            ...f,
            [formFocus]: String(f[formFocus] ?? '').slice(0, -1)
          }));
          return;
        }
        if (k.length === 1 && !e.metaKey && !e.ctrlKey) {
          if (formFocus === 'priority' && !/\d/.test(k)) return;
          e.preventDefault();
          setForm(f => ({
            ...f,
            [formFocus]: String(f[formFocus] ?? '') + k
          }));
        }
        return;
      }
      if (mode === 'delete') {
        if (k === 'Escape' || k === 'n' || k === 'N') {
          e.preventDefault();
          setMode('list');
        } else if (k === 'Enter' || k === 'y' || k === 'Y') {
          e.preventDefault();
          confirmDelete();
        }
        return;
      }
      if (mode === 'help') {
        if (k === 'Escape' || k === 'F1') {
          e.preventDefault();
          setMode('list');
        }
        return;
      }
      if (mode === 'filter') {
        if (k === 'Escape' || k === 'Enter') {
          e.preventDefault();
          setMode('list');
          clampSel(0);
          return;
        }
        if (k === 'Backspace') {
          e.preventDefault();
          setFilter(s => s.slice(0, -1));
          return;
        }
        if (k.length === 1 && !e.metaKey && !e.ctrlKey) {
          e.preventDefault();
          setFilter(s => s + k);
        }
        return;
      }
      if (mode === 'bulk') {
        const meta = TYPE_META[bulk.type];
        const vf = visibleBulkFields(meta);
        if (k === 'Escape') {
          e.preventDefault();
          if (bulk.step === 2) setBulk(b => ({
            ...b,
            step: 1
          }));else setMode('list');
          return;
        }
        if (bulk.step === 1) {
          if (k === 'Tab') {
            e.preventDefault();
            const i = vf.indexOf(bulk.focus);
            setBulk(b => ({
              ...b,
              focus: vf[(i + (e.shiftKey ? -1 : 1) + vf.length) % vf.length]
            }));
            return;
          }
          if (k === 'Enter' && bulk.focus !== 'ips') {
            e.preventDefault();
            setBulk(b => ({
              ...b,
              step: 2
            }));
            return;
          }
          if (bulk.focus === 'type') {
            if (k === 'ArrowRight') {
              e.preventDefault();
              bulkCycleType(1);
            } else if (k === 'ArrowLeft') {
              e.preventDefault();
              bulkCycleType(-1);
            }
            return;
          }
          if (bulk.focus === 'proxy') {
            if (k === 'ArrowRight' || k === 'ArrowLeft' || k === ' ') {
              e.preventDefault();
              setBulk(b => ({
                ...b,
                proxied: !b.proxied
              }));
            }
            return;
          }
          if (bulk.focus === 'ips') {
            if (k === 'Enter') {
              e.preventDefault();
              setBulk(b => ({
                ...b,
                ips: b.ips + '\n'
              }));
              return;
            }
            if (k === 'Backspace') {
              e.preventDefault();
              setBulk(b => ({
                ...b,
                ips: b.ips.slice(0, -1)
              }));
              return;
            }
            if (k.length === 1 && !e.metaKey && !e.ctrlKey) {
              e.preventDefault();
              setBulk(b => ({
                ...b,
                ips: b.ips + k
              }));
            }
            return;
          }
          // first (seed) field
          if (k === 'Backspace') {
            e.preventDefault();
            setBulk(b => ({
              ...b,
              first: b.first.slice(0, -1)
            }));
            return;
          }
          if (k.length === 1 && !e.metaKey && !e.ctrlKey) {
            e.preventDefault();
            setBulk(b => ({
              ...b,
              first: b.first + k
            }));
          }
          return;
        } else {
          if (k === 'Enter') {
            e.preventDefault();
            bulkConfirm();
          } else if (k === 'ArrowLeft') {
            e.preventDefault();
            setBulk(b => ({
              ...b,
              step: 1
            }));
          }
          return;
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  /* ---------------- function bar ---------------- */
  const fkItems = () => {
    if (mode === 'list') return [{
      key: 'F1',
      label: 'Help',
      onClick: () => setMode('help')
    }, {
      key: 'F2',
      label: 'Add',
      onClick: openAdd
    }, {
      key: 'F3',
      label: 'Edit',
      onClick: openEdit
    }, {
      key: 'F4',
      label: 'Proxy',
      onClick: toggleProxy
    }, {
      key: 'F5',
      label: 'Bulk',
      onClick: openBulk,
      active: true
    }, {
      key: 'F6',
      label: 'Filter',
      onClick: () => setMode('filter')
    }, {
      key: 'F8',
      label: 'Delete',
      danger: true,
      onClick: () => {
        if (sel) setMode('delete');
      }
    }, {
      key: 'F10',
      label: 'Quit',
      onClick: () => flash('Quit — close the tab. (demo)', 'warn')
    }];
    if (mode === 'add' || mode === 'edit') return [{
      key: 'Esc',
      label: 'Cancel',
      onClick: () => setMode('list')
    }, {
      key: '↹',
      label: 'Next field'
    }, {
      key: '↵',
      label: mode === 'edit' ? 'Save' : 'Add',
      onClick: saveForm,
      active: true
    }];
    if (mode === 'delete') return [{
      key: 'N',
      label: 'Cancel',
      onClick: () => setMode('list')
    }, {
      key: 'Y',
      label: 'Delete',
      danger: true,
      onClick: confirmDelete,
      active: true
    }];
    if (mode === 'bulk' && bulk.step === 1) return [{
      key: 'Esc',
      label: 'Cancel',
      onClick: () => setMode('list')
    }, {
      key: '↹',
      label: 'Next field'
    }, {
      key: '↵',
      label: 'Next ▸',
      onClick: () => setBulk(b => ({
        ...b,
        step: 2
      })),
      active: true
    }];
    if (mode === 'bulk' && bulk.step === 2) return [{
      key: 'Esc',
      label: 'Back',
      onClick: () => setBulk(b => ({
        ...b,
        step: 1
      }))
    }, {
      key: '↵',
      label: `Add ${expand(bulk.first, bulk.ips).length}`,
      onClick: bulkConfirm,
      active: true
    }];
    return [{
      key: 'Esc',
      label: 'Close',
      onClick: () => setMode('list'),
      active: true
    }];
  };
  const statusLeft = mode === 'filter' ? /*#__PURE__*/React.createElement("span", null, "filter: ", /*#__PURE__*/React.createElement("span", {
    style: {
      color: 'var(--fg)'
    }
  }, filter, /*#__PURE__*/React.createElement("span", {
    className: "tui-cursor",
    style: {
      color: 'var(--orange)'
    }
  }, "\u2588")), "\xA0\xA0", /*#__PURE__*/React.createElement("span", {
    style: {
      color: 'var(--fg-faint)'
    }
  }, filtered.length, " match")) : filter ? /*#__PURE__*/React.createElement("span", null, "filter: ", /*#__PURE__*/React.createElement("span", {
    style: {
      color: 'var(--fg)'
    }
  }, filter), " \xB7 ", filtered.length, " of ", records.length) : '';
  const meta = form ? TYPE_META[form.type] : null;
  const bmeta = bulk ? TYPE_META[bulk.type] : null;
  const dim = mode !== 'list' && mode !== 'filter';
  return /*#__PURE__*/React.createElement(Screen, null, /*#__PURE__*/React.createElement(Header, {
    zone: ZONE,
    count: records.length,
    proxiedCount: proxiedCount
  }), /*#__PURE__*/React.createElement("div", {
    className: dim ? 'tui-dimmed' : ''
  }, /*#__PURE__*/React.createElement(RecordsTable, {
    records: filtered,
    selected: selected,
    onSelect: i => {
      clampSel(i);
    },
    focused: mode === 'list' || mode === 'filter'
  })), /*#__PURE__*/React.createElement(StatusBar, {
    zone: ZONE,
    left: statusLeft,
    message: msg.text,
    messageType: msg.type,
    spinner: false
  }), /*#__PURE__*/React.createElement(FunctionBar, {
    items: fkItems()
  }), (mode === 'add' || mode === 'edit') && /*#__PURE__*/React.createElement(AddEditDialog, {
    mode: mode,
    form: form,
    focus: formFocus,
    meta: meta,
    onFocusField: setFormFocus,
    onCycleType: cycleType,
    onToggleProxy: toggleFormProxy,
    onCycleTTL: cycleTTL,
    onSave: saveForm,
    onCancel: () => setMode('list')
  }), mode === 'delete' && sel && /*#__PURE__*/React.createElement(DeleteDialog, {
    rec: sel,
    onConfirm: confirmDelete,
    onCancel: () => setMode('list')
  }), mode === 'bulk' && /*#__PURE__*/React.createElement(BulkWizard, {
    st: bulk,
    meta: bmeta,
    focus: bulk.focus,
    onFocusField: id => setBulk(b => ({
      ...b,
      focus: id
    })),
    onCycleType: bulkCycleType,
    onToggleProxy: () => setBulk(b => ({
      ...b,
      proxied: !b.proxied
    })),
    onBack: () => setBulk(b => ({
      ...b,
      step: 1
    })),
    onNext: () => setBulk(b => ({
      ...b,
      step: 2
    })),
    onConfirm: bulkConfirm,
    onCancel: () => setMode('list')
  }), mode === 'help' && /*#__PURE__*/React.createElement(HelpOverlay, {
    onCancel: () => setMode('list')
  }));
}
ReactDOM.createRoot(document.getElementById('root')).render(/*#__PURE__*/React.createElement(App, null));
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/dns-client/app.jsx", error: String((e && e.message) || e) }); }

// ui_kits/dns-client/data.js
try { (() => {
/* data.js — sample zone + record-type metadata for the cfdns UI kit. */

const RECORD_TYPES = [{
  type: 'A',
  desc: 'IPv4 address',
  proxiable: true,
  label: 'IPv4 address',
  ph: '192.0.2.1'
}, {
  type: 'AAAA',
  desc: 'IPv6 address',
  proxiable: true,
  label: 'IPv6 address',
  ph: '2606:4700::1'
}, {
  type: 'CNAME',
  desc: 'alias',
  proxiable: true,
  label: 'target',
  ph: 'target.example.com'
}, {
  type: 'MX',
  desc: 'mail exchange',
  proxiable: false,
  label: 'mail server',
  ph: 'mail.example.com',
  priority: true
}, {
  type: 'TXT',
  desc: 'text',
  proxiable: false,
  label: 'content',
  ph: 'v=spf1 -all'
}, {
  type: 'NS',
  desc: 'nameserver',
  proxiable: false,
  label: 'nameserver',
  ph: 'ns.example.com'
}, {
  type: 'SRV',
  desc: 'service',
  proxiable: false,
  label: 'target',
  ph: '_sip._tcp target',
  priority: true
}, {
  type: 'CAA',
  desc: 'cert authority',
  proxiable: false,
  label: 'value',
  ph: '0 issue "letsencrypt.org"'
}];
const TYPE_META = Object.fromEntries(RECORD_TYPES.map(t => [t.type, t]));

// ttl: 1 = Auto; otherwise seconds
function ttlLabel(ttl) {
  if (ttl === 1 || ttl == null) return 'Auto';
  if (ttl < 60) return ttl + 's';
  if (ttl < 3600) return ttl / 60 + 'm';
  if (ttl < 86400) return ttl / 3600 + 'h';
  return ttl / 86400 + 'd';
}
let _id = 100;
const rid = () => 'r' + ++_id;
const ZONE = 'mydomain.com';
const SAMPLE_RECORDS = [{
  id: rid(),
  type: 'A',
  name: '@',
  content: '192.0.2.10',
  ttl: 1,
  proxied: true
}, {
  id: rid(),
  type: 'A',
  name: 'www',
  content: '192.0.2.10',
  ttl: 1,
  proxied: true
}, {
  id: rid(),
  type: 'AAAA',
  name: 'www',
  content: '2606:4700:3033::6815:1',
  ttl: 1,
  proxied: true
}, {
  id: rid(),
  type: 'A',
  name: 'api',
  content: '192.0.2.20',
  ttl: 1,
  proxied: true
}, {
  id: rid(),
  type: 'AAAA',
  name: 'api',
  content: '2606:4700:3033::6815:2',
  ttl: 1,
  proxied: true
}, {
  id: rid(),
  type: 'A',
  name: 'dev',
  content: '198.51.100.5',
  ttl: 300,
  proxied: false
}, {
  id: rid(),
  type: 'CNAME',
  name: 'blog',
  content: 'mydomain.ghost.io',
  ttl: 1,
  proxied: true
}, {
  id: rid(),
  type: 'CNAME',
  name: 'shop',
  content: 'shops.myshopify.com',
  ttl: 1,
  proxied: false
}, {
  id: rid(),
  type: 'CNAME',
  name: '_dnslink',
  content: 'cname.vercel-dns.com',
  ttl: 3600,
  proxied: false
}, {
  id: rid(),
  type: 'MX',
  name: '@',
  content: 'route1.mx.cloudflare.net',
  priority: 10,
  ttl: 1,
  proxied: false
}, {
  id: rid(),
  type: 'MX',
  name: '@',
  content: 'route2.mx.cloudflare.net',
  priority: 20,
  ttl: 1,
  proxied: false
}, {
  id: rid(),
  type: 'TXT',
  name: '@',
  content: 'v=spf1 include:_spf.google.com ~all',
  ttl: 1,
  proxied: false
}, {
  id: rid(),
  type: 'TXT',
  name: '_dmarc',
  content: 'v=DMARC1; p=reject; rua=mailto:dmarc@mydomain.com',
  ttl: 1,
  proxied: false
}, {
  id: rid(),
  type: 'NS',
  name: '@',
  content: 'ada.ns.cloudflare.com',
  ttl: 86400,
  proxied: false
}, {
  id: rid(),
  type: 'NS',
  name: '@',
  content: 'rob.ns.cloudflare.com',
  ttl: 86400,
  proxied: false
}, {
  id: rid(),
  type: 'CAA',
  name: '@',
  content: '0 issue "letsencrypt.org"',
  ttl: 1,
  proxied: false
}];
const TYPE_COLOR = {
  A: 'var(--blue)',
  AAAA: 'var(--blue)',
  CNAME: 'var(--green)',
  MX: 'var(--magenta)',
  TXT: 'var(--yellow)',
  NS: 'var(--fg-muted)',
  SRV: 'var(--magenta)',
  CAA: 'var(--fg-muted)'
};
Object.assign(window, {
  RECORD_TYPES,
  TYPE_META,
  ttlLabel,
  rid,
  ZONE,
  SAMPLE_RECORDS,
  TYPE_COLOR
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/dns-client/data.js", error: String((e && e.message) || e) }); }

// ui_kits/dns-client/tui.jsx
try { (() => {
/* tui.jsx — core primitives for rendering the cfdns terminal UI in React.
   Everything is drawn on a fixed character grid (COLS × ROWS) that scales to fit
   the viewport. Rows are <div class="tui-row"> with white-space:pre so box-drawing
   glyphs in fixed columns connect vertically. Coloring via inline <span>.        */

const COLS = 104;
const ROWS = 34;
const C = {
  fg: 'var(--fg)',
  bright: 'var(--fg-bright)',
  muted: 'var(--fg-muted)',
  faint: 'var(--fg-faint)',
  disabled: 'var(--fg-disabled)',
  orange: 'var(--orange)',
  orangeB: 'var(--orange-bright)',
  orangeD: 'var(--orange-dim)',
  ink: 'var(--orange-ink)',
  green: 'var(--green)',
  red: 'var(--red)',
  blue: 'var(--blue)',
  yellow: 'var(--yellow)',
  magenta: 'var(--magenta)',
  line: 'var(--line)'
};
const SETS = {
  single: ['┌', '┐', '└', '┘', '─', '│', '├', '┤'],
  double: ['╔', '╗', '╚', '╝', '═', '║', '╠', '╣']
};
function pad(s, w, align) {
  s = s == null ? '' : String(s);
  if (w == null) return s;
  if (s.length > w) return s.slice(0, w);
  const gap = w - s.length;
  if (align === 'right') return ' '.repeat(gap) + s;
  if (align === 'center') return ' '.repeat(gap >> 1) + s + ' '.repeat(gap - (gap >> 1));
  return s + ' '.repeat(gap);
}

/* Turn an array of segment descriptors into React <span>s, expanding any {fill}
   segment to consume the remaining width up to total `w`.
   seg = { t, w, align, color, bg, bold, cls, fill, ch }  */
function buildSpans(segs, w) {
  let fixed = 0,
    fills = 0;
  for (const s of segs) {
    if (s.fill) fills++;else fixed += s.w != null ? s.w : s.t == null ? 0 : String(s.t).length;
  }
  let rem = Math.max(0, (w != null ? w : fixed) - fixed);
  let per = fills ? Math.floor(rem / fills) : 0,
    extra = rem - per * fills,
    fi = 0;
  return segs.map((s, i) => {
    let t;
    if (s.fill) {
      const ww = per + (fi === fills - 1 ? extra : 0);
      fi++;
      t = (s.ch || ' ').repeat(ww);
    } else if (s.w != null) t = pad(s.t, s.w, s.align);else t = s.t == null ? '' : String(s.t);
    const st = {};
    if (s.color) st.color = s.color;
    if (s.bg) st.background = s.bg;
    if (s.bold) st.fontWeight = 500;
    return React.createElement('span', {
      key: i,
      className: s.cls || '',
      style: st
    }, t);
  });
}
function Row({
  segs,
  w = COLS,
  style,
  className,
  onClick,
  onMouseEnter
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: 'tui-row ' + (className || ''),
    style: style,
    onClick: onClick,
    onMouseEnter: onMouseEnter
  }, buildSpans(segs, w));
}
const bcOf = (focus, double) => focus ? C.orange : double ? C.orangeD : C.line;
function TopBorder({
  w = COLS,
  title,
  focus,
  double,
  right
}) {
  const [tl, tr,,, H,,,] = SETS[double ? 'double' : 'single'];
  const bc = bcOf(focus, double);
  const segs = [{
    t: tl + H,
    color: bc
  }];
  if (title) {
    segs.push({
      t: ' ',
      color: bc
    }, {
      t: title,
      color: focus ? C.bright : C.muted,
      bold: true
    }, {
      t: ' ',
      color: bc
    });
  }
  segs.push({
    fill: true,
    ch: H,
    color: bc
  });
  if (right) segs.push({
    t: ' ' + right + ' ',
    color: C.faint
  });
  segs.push({
    t: H + tr,
    color: bc
  });
  return /*#__PURE__*/React.createElement(Row, {
    w: w,
    segs: segs
  });
}
function SepBorder({
  w = COLS,
  focus,
  double
}) {
  const set = SETS[double ? 'double' : 'single'];
  const lj = set[6],
    rj = set[7],
    H = set[4];
  const bc = bcOf(focus, double);
  return /*#__PURE__*/React.createElement(Row, {
    w: w,
    segs: [{
      t: lj,
      color: bc
    }, {
      fill: true,
      ch: H,
      color: bc
    }, {
      t: rj,
      color: bc
    }]
  });
}
function BotBorder({
  w = COLS,
  focus,
  double
}) {
  const [,, bl, br, H,,,] = SETS[double ? 'double' : 'single'];
  const bc = bcOf(focus, double);
  return /*#__PURE__*/React.createElement(Row, {
    w: w,
    segs: [{
      t: bl,
      color: bc
    }, {
      fill: true,
      ch: H,
      color: bc
    }, {
      t: br,
      color: bc
    }]
  });
}

/* A content row inside a frame: │ <pad> ...inner... <fill> │
   When `bg` is set the whole inner region is tinted (reverse-video selection);
   `ink` overrides text color on that fill. Borders keep their own color. */
function BodyRow({
  inner,
  w = COLS,
  focus,
  double,
  bg,
  ink,
  onClick,
  onMouseEnter,
  className
}) {
  const V = SETS[double ? 'double' : 'single'][5];
  const bc = bcOf(focus, double);
  const lead = {
      t: ' '
    },
    fill = {
      fill: true
    };
  let content = inner;
  if (bg) {
    content = inner.map(s => ({
      ...s,
      bg,
      color: ink || s.color
    }));
    lead.bg = bg;
    fill.bg = bg;
  }
  const segs = [{
    t: V,
    color: bc
  }, lead, ...content, fill, {
    t: V,
    color: bc
  }];
  return /*#__PURE__*/React.createElement(Row, {
    w: w,
    segs: segs,
    onClick: onClick,
    onMouseEnter: onMouseEnter,
    className: className
  });
}

/* Simple titled box for dialogs: pass `rows` = array of inner-seg-arrays. */
function Panel({
  title,
  w,
  focus,
  double,
  rows,
  fillTo
}) {
  const body = rows.slice();
  if (fillTo) while (body.length < fillTo) body.push([]);
  return /*#__PURE__*/React.createElement(React.Fragment, null, /*#__PURE__*/React.createElement(TopBorder, {
    w: w,
    title: title,
    focus: focus,
    double: double
  }), body.map((inner, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    inner: inner,
    w: w,
    focus: focus,
    double: double
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: w,
    focus: focus,
    double: double
  }));
}

/* Center a fixed COLS×ROWS screen in the viewport, scaled to fit (letterboxed). */
function useScale(ref) {
  React.useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const fit = () => {
      const nw = el.offsetWidth,
        nh = el.offsetHeight;
      if (!nw || !nh) return;
      const margin = 0.96; // keep the frame off the very edge
      let s = Math.min(window.innerWidth / nw, window.innerHeight / nh) * margin;
      s = Math.min(s, 1.3); // don't blow the text up on huge screens
      el.style.transform = `translate(-50%,-50%) scale(${s})`;
    };
    // always measure AFTER layout settles — reading inside the observer
    // callback can catch a stale box and produce a wildly wrong scale
    let raf = 0;
    const schedule = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(fit);
    };
    schedule();
    window.addEventListener('resize', schedule);
    const ro = new ResizeObserver(schedule);
    ro.observe(el);
    if (document.fonts && document.fonts.ready) document.fonts.ready.then(schedule);
    // low-frequency safety net: re-assert the transform if anything external
    // clears it (print/export passes, screenshot tooling, devtools edits)
    const heal = setInterval(fit, 1000);
    return () => {
      cancelAnimationFrame(raf);
      clearInterval(heal);
      window.removeEventListener('resize', schedule);
      ro.disconnect();
    };
  }, []);
}
function Screen({
  children
}) {
  const ref = React.useRef(null);
  useScale(ref);
  return /*#__PURE__*/React.createElement("div", {
    className: "tui-stage"
  }, /*#__PURE__*/React.createElement("div", {
    className: "tui-screen",
    ref: ref
  }, children));
}
Object.assign(window, {
  COLS,
  ROWS,
  C,
  pad,
  buildSpans,
  Row,
  Panel,
  TopBorder,
  SepBorder,
  BotBorder,
  BodyRow,
  Screen,
  useScale
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/dns-client/tui.jsx", error: String((e && e.message) || e) }); }

// ui_kits/openadmin/AppChrome.jsx
try { (() => {
/* AppChrome.jsx — OpenAdmin header (brand block + screen tabs), status bar, F-key bar.
   Screen tabs and F-keys are mouse-hoverable and clickable. */

const SCREENS = [{
  id: 'hosts',
  label: 'Hosts'
}, {
  id: 'shells',
  label: 'Shells'
}, {
  id: 'chat',
  label: 'Chat'
}];
function Header({
  screen,
  onScreen,
  counts
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "oa-header"
  }, /*#__PURE__*/React.createElement("div", {
    className: "oa-hrow"
  }, /*#__PURE__*/React.createElement("div", {
    className: "oa-left"
  }, /*#__PURE__*/React.createElement("div", {
    className: "oa-mark"
  }), /*#__PURE__*/React.createElement("div", {
    className: "oa-word"
  }, /*#__PURE__*/React.createElement("div", {
    className: "oa-name"
  }, "OpenAdmin"), /*#__PURE__*/React.createElement("div", {
    className: "oa-tag"
  }, "remote hosts \xB7 shells \xB7 agent"))), /*#__PURE__*/React.createElement("div", {
    className: "oa-tabs"
  }, SCREENS.map((s, i) => /*#__PURE__*/React.createElement("span", {
    key: s.id,
    className: 'oa-tab' + (screen === s.id ? ' oa-tab-on' : ''),
    onClick: () => onScreen(s.id),
    title: `Alt+${i + 1} · switch to ${s.label}`
  }, /*#__PURE__*/React.createElement("span", {
    className: "oa-tabnum"
  }, i + 1), /*#__PURE__*/React.createElement("span", {
    className: "oa-tablab"
  }, s.label), counts[s.id] != null && /*#__PURE__*/React.createElement("span", {
    className: "oa-tabct"
  }, counts[s.id]))))), /*#__PURE__*/React.createElement(Row, {
    w: COLS,
    segs: [{
      fill: true,
      ch: '─',
      color: C.line
    }]
  }));
}
function StatusBar({
  left,
  hint,
  message,
  messageType,
  busy
}) {
  const mc = {
    ok: 'oa-ok',
    err: 'oa-err',
    warn: 'oa-warn',
    info: 'oa-info'
  }[messageType] || '';
  return /*#__PURE__*/React.createElement("div", {
    className: "oa-statusbar"
  }, /*#__PURE__*/React.createElement("span", {
    className: "oa-sb-left"
  }, left), hint && /*#__PURE__*/React.createElement("span", {
    className: "oa-sb-hint"
  }, hint), /*#__PURE__*/React.createElement("span", {
    className: 'oa-sb-msg ' + mc
  }, busy && /*#__PURE__*/React.createElement(React.Fragment, null, /*#__PURE__*/React.createElement(Spinner, null), "\xA0"), message));
}
function FunctionBar({
  items
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "oa-fnbar"
  }, items.map((it, i) => /*#__PURE__*/React.createElement("span", {
    key: i,
    className: 'oa-fk' + (it.active ? ' oa-fk-on' : '') + (it.disabled ? ' oa-fk-off' : ''),
    onClick: it.disabled ? null : it.onClick,
    title: it.title || ''
  }, /*#__PURE__*/React.createElement("span", {
    className: "oa-fk-cap"
  }, it.key), /*#__PURE__*/React.createElement("span", {
    className: 'oa-fk-lab' + (it.danger ? ' oa-fk-danger' : '')
  }, it.label))));
}
Object.assign(window, {
  Header,
  StatusBar,
  FunctionBar,
  SCREENS
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/openadmin/AppChrome.jsx", error: String((e && e.message) || e) }); }

// ui_kits/openadmin/ChatScreen.jsx
try { (() => {
/* ChatScreen.jsx — screen 3: agentic chat, opencode-inspired.
   A single transcript column: user turns are marked with an orange » prompt,
   assistant prose is plain, and tool calls render as collapsed boxes with a
   status glyph, the command, and its captured output indented under a tree rule. */

const TOOL_STATUS = {
  ok: {
    g: '✓',
    c: C.green,
    t: 'ok'
  },
  fail: {
    g: '×',
    c: C.red,
    t: 'exit 1'
  },
  empty: {
    g: '○',
    c: C.muted,
    t: 'no output'
  },
  run: {
    g: '…',
    c: C.orange,
    t: 'running'
  }
};
function wrap(text, width) {
  const out = [];
  for (const para of String(text).split('\n')) {
    if (!para) {
      out.push('');
      continue;
    }
    let line = '';
    for (const word of para.split(' ')) {
      if ((line + ' ' + word).trim().length > width) {
        out.push(line.trim());
        line = word;
      } else line += ' ' + word;
    }
    if (line.trim()) out.push(line.trim());
  }
  return out;
}
function chatRows(msgs, width) {
  const rows = [];
  const push = segs => rows.push(segs);
  msgs.forEach((m, mi) => {
    if (mi) push([]);
    if (m.role === 'user') {
      wrap(m.text, width - 4).forEach((ln, i) => push([{
        t: i === 0 ? '» ' : '  ',
        color: C.orange,
        bold: true
      }, {
        t: ln,
        color: C.bright
      }]));
    } else if (m.role === 'assistant') {
      wrap(m.text, width - 4).forEach(ln => push([{
        t: '  '
      }, {
        t: ln,
        color: C.fg
      }]));
    } else if (m.role === 'tool') {
      const st = TOOL_STATUS[m.status] || TOOL_STATUS.ok;
      push([{
        t: '  '
      }, {
        t: '┌ ',
        color: C.line
      }, {
        t: m.name,
        color: C.magenta,
        bold: true
      }, {
        t: ' · ',
        color: C.line
      }, {
        t: m.arg,
        color: C.muted
      }, {
        t: '  '
      }, {
        t: st.g,
        color: st.c
      }, {
        t: ' ' + st.t,
        color: st.c
      }]);
      (m.out || []).forEach((ln, i, a) => push([{
        t: '  '
      }, {
        t: i === a.length - 1 ? '└ ' : '│ ',
        color: C.line
      }, {
        t: ln,
        color: m.status === 'fail' ? C.fg : C.muted
      }]));
    }
  });
  return rows;
}
function ChatScreen({
  msgs,
  draft,
  busy,
  focused,
  model
}) {
  const WIDTH = COLS - 4;
  const VIEW = 17;
  const all = chatRows(msgs, WIDTH);
  if (busy) all.push([], [{
    t: '  '
  }, {
    t: '… ',
    color: C.orange
  }, {
    t: 'thinking',
    color: C.muted
  }]);
  const shown = all.slice(-VIEW);
  while (shown.length < VIEW) shown.unshift([]);
  const draftSegs = [{
    t: '» ',
    color: C.orange,
    bold: true
  }, ...(draft ? [{
    t: draft,
    color: C.fg
  }] : []), {
    t: '█',
    color: C.orange,
    cls: focused ? 'tui-cursor' : ''
  }, ...(!draft ? [{
    t: '  ask the agent to inspect or change a host…',
    color: C.faint
  }] : [])];
  return /*#__PURE__*/React.createElement("div", {
    className: "oa-chat"
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: COLS,
    title: "Agent",
    focus: true,
    right: `${model} · 3 hosts in context`
  }), shown.map((r, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: COLS,
    inner: r,
    focus: true
  })), /*#__PURE__*/React.createElement(SepBorder, {
    w: COLS,
    focus: true
  }), /*#__PURE__*/React.createElement(BodyRow, {
    w: COLS,
    inner: draftSegs,
    focus: true
  }), /*#__PURE__*/React.createElement(BodyRow, {
    w: COLS,
    inner: [{
      t: 'Enter send   Shift+Enter newline   @ add host to context   ^R run command',
      color: C.faint
    }, {
      fill: true
    }, {
      t: 'web-01 db-main bastion',
      color: C.faint
    }],
    focus: true
  }), /*#__PURE__*/React.createElement(BotBorder, {
    w: COLS,
    focus: true
  }));
}
Object.assign(window, {
  ChatScreen,
  chatRows,
  wrap
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/openadmin/ChatScreen.jsx", error: String((e && e.message) || e) }); }

// ui_kits/openadmin/HostDialogs.jsx
try { (() => {
/* HostDialogs.jsx — add/edit host, delete confirm, and SSH public key output.
   The Add/Edit form derives MOUNT POINT from NAME (/net/<name>) until the user
   edits the mount field themselves, after which it is left alone. */

const DW = 72;
function DialogOverlay({
  children,
  onBackdrop
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "oa-overlay",
    onClick: onBackdrop
  }, /*#__PURE__*/React.createElement("div", {
    className: "oa-dialog",
    onClick: e => e.stopPropagation()
  }, children));
}
const FORM_FIELDS = ['name', 'type', 'addr', 'port', 'mount', 'login', 'pass'];
function HostDialog({
  mode,
  form,
  focus,
  onFocus,
  onCycleType,
  onSave,
  onCancel,
  onGenKey
}) {
  const f = id => focus === id;
  const rows = [];
  const R = (inner, onClick) => rows.push({
    inner,
    onClick
  });
  R([]);
  R(fieldInner('Host name', wellSegs(form.name, 'nickname, e.g. web-01', 34, f('name')), f('name')), () => onFocus('name'));
  R([{
    t: ' '.repeat(LABELW + 2)
  }, {
    t: 'a human label — the mount point follows it',
    color: C.faint
  }]);
  R(fieldInner('Type', selectSegs(form.type, 12, f('type')), f('type')), () => onCycleType(1));
  R(fieldInner('Address', wellSegs(form.addr, 'host or IP', 34, f('addr')), f('addr')), () => onFocus('addr'));
  R(fieldInner('Port', wellSegs(String(form.port ?? ''), '22', 8, f('port')), f('port')), () => onFocus('port'));
  R(fieldInner('Mount point', wellSegs(form.mount, '/net/<name>', 34, f('mount')), f('mount')), () => onFocus('mount'));
  R([{
    t: ' '.repeat(LABELW + 2)
  }, {
    t: form.mountAuto ? 'auto from host name — type here to override' : 'overridden · clear to restore auto',
    color: form.mountAuto ? C.faint : C.yellow
  }]);
  R(fieldInner('Login', wellSegs(form.login, 'user', 24, f('login')), f('login')), () => onFocus('login'));
  R(fieldInner('Password', wellSegs(form.pass ? '•'.repeat(form.pass.length) : '', 'optional with a key', 24, f('pass')), f('pass')), () => onFocus('pass'));
  R([]);
  R([labelSeg('SSH key', false), {
    t: '  '
  }, ...(form.key ? [{
    t: '✓ ',
    color: C.green
  }, {
    t: 'key installed',
    color: C.fg
  }, {
    t: '    '
  }, {
    t: '[ Show public key ]',
    color: C.orangeB,
    cls: 'oa-btn'
  }] : [{
    t: '[ Generate SSH key ]',
    color: C.orangeB,
    cls: 'oa-btn'
  }, {
    t: '   '
  }, {
    t: 'prints the public key',
    color: C.faint
  }])], onGenKey);
  R([]);
  R([{
    t: 'Tab next field   ←/→ change type   Enter save   Esc cancel',
    color: C.faint
  }]);
  R([{
    fill: true
  }, {
    t: ` ${mode === 'edit' ? 'Save' : 'Add Host'} `,
    bg: C.orange,
    color: C.ink,
    bold: true
  }, {
    t: '   '
  }, {
    t: '[ Cancel ]',
    color: C.fg
  }, {
    t: ' '
  }]);
  R([]);
  const saveIdx = rows.length - 2;
  return /*#__PURE__*/React.createElement(DialogOverlay, {
    onBackdrop: onCancel
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: DW,
    title: mode === 'edit' ? 'Edit Host' : 'Add Host',
    focus: true
  }), rows.map((r, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: DW,
    inner: r.inner,
    focus: true,
    onClick: i === saveIdx ? onSave : r.onClick,
    className: r.onClick || i === saveIdx ? 'oa-clickrow' : ''
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: DW,
    focus: true
  }));
}
function DeleteDialog({
  targets,
  onConfirm,
  onCancel
}) {
  const W = 58;
  const many = targets.length > 1;
  const rows = [[], [{
    t: 'Delete ',
    color: C.fg
  }, {
    t: many ? `${targets.length} hosts` : `"${targets[0].name}"`,
    color: many ? C.mark : C.fg,
    bold: true
  }, {
    t: ' ?',
    color: C.fg
  }], ...(many ? targets.slice(0, 4).map(t => [{
    t: '  ' + t.name,
    color: C.muted
  }]).concat(targets.length > 4 ? [[{
    t: `  … and ${targets.length - 4} more`,
    color: C.faint
  }]] : []) : [[{
    t: `  ${targets[0].login}@${targets[0].addr}:${targets[0].port}`,
    color: C.muted
  }]]), [], [{
    t: 'Saved credentials and keys are removed too.',
    color: C.muted
  }], [], [{
    fill: true
  }, {
    t: ' Delete ',
    bg: C.red,
    color: '#1a0000',
    bold: true
  }, {
    t: '   '
  }, {
    t: '[ Cancel ]',
    color: C.fg
  }, {
    t: ' '
  }], []];
  const ci = rows.length - 2;
  return /*#__PURE__*/React.createElement(DialogOverlay, {
    onBackdrop: onCancel
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: W,
    title: "Confirm Delete",
    double: true
  }), rows.map((inner, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: W,
    inner: inner,
    double: true,
    onClick: i === ci ? onConfirm : null,
    className: i === ci ? 'oa-clickrow' : ''
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: W,
    double: true
  }));
}

/* SSH public key output — the key is shown wrapped so it can be copied out. */
function KeyDialog({
  host,
  onCancel,
  onCopy
}) {
  const W = 86;
  const inner = W - 4;
  const chunks = [];
  for (let i = 0; i < PUBKEY.length; i += inner) chunks.push(PUBKEY.slice(i, i + inner));
  const rows = [[], [{
    t: 'Public key for ',
    color: C.fg
  }, {
    t: host.name,
    color: C.orange,
    bold: true
  }, {
    t: '  ·  ed25519',
    color: C.muted
  }], [], [{
    t: 'Add this line to ',
    color: C.muted
  }, {
    t: '~/.ssh/authorized_keys',
    color: C.blue
  }, {
    t: ' on the remote host:',
    color: C.muted
  }], [], ...chunks.map(c => [{
    t: ' '
  }, {
    t: c,
    w: inner,
    bg: C.inset,
    color: C.green
  }, {
    t: ' ',
    bg: C.inset
  }]), [], [{
    t: 'Private key stored at ',
    color: C.muted
  }, {
    t: '~/.config/openadmin/keys/' + host.name,
    color: C.faint
  }], [], [{
    fill: true
  }, {
    t: ' Copy ',
    bg: C.orange,
    color: C.ink,
    bold: true
  }, {
    t: '   '
  }, {
    t: '[ Close ]',
    color: C.fg
  }, {
    t: ' '
  }], []];
  const ci = rows.length - 2;
  return /*#__PURE__*/React.createElement(DialogOverlay, {
    onBackdrop: onCancel
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: W,
    title: "SSH Public Key",
    focus: true
  }), rows.map((r, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: W,
    inner: r,
    focus: true,
    onClick: i === ci ? onCopy : null,
    className: i === ci ? 'oa-clickrow' : ''
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: W,
    focus: true
  }));
}
function HelpDialog({
  onCancel
}) {
  const W = 66;
  const k = (key, desc) => [{
    t: key,
    w: 14,
    color: C.orangeB
  }, {
    t: desc,
    color: C.fg
  }];
  const rows = [[], [{
    t: 'SCREENS',
    cls: 'tui-colhead'
  }], k('Alt+1/2/3', 'Hosts · Shells · Chat'), k('F9', 'cycle screen'), [], [{
    t: 'HOSTS',
    cls: 'tui-colhead'
  }], k('↑ ↓', 'move cursor'), k('Insert', 'mark / unmark host (multi-select)'), k('* ', 'invert marks     Ctrl+A select all'), k('F2 / F3', 'add · edit'), k('F4', 'mount / unmount'), k('F5', 'open shell (marked hosts → group tab)'), k('F6', 'use as proxy'), k('F7', 'generate SSH key'), k('F8', 'delete'), [], [{
    t: 'MOUSE',
    cls: 'tui-colhead'
  }], k('hover', 'row highlights, action hint in status bar'), k('click', 'move cursor    double-click opens a shell'), [], [{
    t: 'Esc closes any dialog · Tab moves between fields',
    color: C.faint
  }], []];
  return /*#__PURE__*/React.createElement(DialogOverlay, {
    onBackdrop: onCancel
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: W,
    title: "Help \xB7 Key Bindings",
    focus: true
  }), rows.map((r, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: W,
    inner: r,
    focus: true
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: W,
    focus: true
  }));
}
Object.assign(window, {
  HostDialog,
  DeleteDialog,
  KeyDialog,
  HelpDialog,
  DialogOverlay,
  FORM_FIELDS,
  DW
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/openadmin/HostDialogs.jsx", error: String((e && e.message) || e) }); }

// ui_kits/openadmin/HostsScreen.jsx
try { (() => {
/* HostsScreen.jsx — screen 1: known hosts + CRUD.
   Columns: NAME TYPE ADDR PORT MOUNT-POINT LOGIN PASSWORD KEY MNT PRX
   Marks (Insert) render in --yellow-mark so they stay legible on the orange
   cursor bar; the mark glyph ● keeps the state readable even when cursored. */

const HCOL = {
  mark: 2,
  name: 14,
  type: 6,
  addr: 22,
  port: 6,
  gap: 2,
  mount: 20,
  login: 11,
  pass: 11,
  key: 7,
  mnt: 5,
  prx: 5
};
const HROWS = 12;
function maskPass(p) {
  return p ? '•'.repeat(Math.min(8, p.length)) : '—';
}
function HostsScreen({
  hosts,
  cursor,
  marks,
  hovered,
  onCursor,
  onHover,
  onToggleMark,
  onGenKey,
  onOpen
}) {
  let start = 0;
  if (hosts.length > HROWS) start = Math.min(Math.max(0, cursor - (HROWS >> 1)), hosts.length - HROWS);
  const win = hosts.slice(start, start + HROWS);
  const head = [{
    t: '',
    w: HCOL.mark
  }, {
    t: 'NAME',
    w: HCOL.name,
    cls: 'tui-colhead'
  }, {
    t: 'TYPE',
    w: HCOL.type,
    cls: 'tui-colhead'
  }, {
    t: 'ADDR',
    w: HCOL.addr,
    cls: 'tui-colhead'
  }, {
    t: 'PORT',
    w: HCOL.port,
    cls: 'tui-colhead',
    align: 'right'
  }, {
    t: '',
    w: HCOL.gap
  }, {
    t: 'MOUNT POINT',
    w: HCOL.mount,
    cls: 'tui-colhead'
  }, {
    t: 'LOGIN',
    w: HCOL.login,
    cls: 'tui-colhead'
  }, {
    t: 'PASSWORD',
    w: HCOL.pass,
    cls: 'tui-colhead'
  }, {
    t: 'KEY',
    w: HCOL.key,
    cls: 'tui-colhead',
    align: 'center'
  }, {
    t: 'MNT',
    w: HCOL.mnt,
    cls: 'tui-colhead',
    align: 'center'
  }, {
    t: 'PRX',
    w: HCOL.prx,
    cls: 'tui-colhead',
    align: 'center'
  }];
  const nMarked = marks.size;
  const right = nMarked ? `${nMarked} marked of ${hosts.length}` : `${hosts.length} hosts`;
  return /*#__PURE__*/React.createElement("div", {
    className: "oa-hosts"
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: COLS,
    title: "Known Hosts",
    focus: true,
    right: right
  }), /*#__PURE__*/React.createElement(BodyRow, {
    w: COLS,
    inner: head,
    focus: true
  }), /*#__PURE__*/React.createElement(SepBorder, {
    w: COLS,
    focus: true
  }), win.map((h, i) => {
    const idx = start + i;
    const cur = idx === cursor,
      marked = marks.has(h.id),
      hov = idx === hovered;
    const markCol = C.mark;
    const inner = [{
      t: marked ? '●' : cur ? '▸' : ' ',
      w: HCOL.mark,
      color: marked ? markCol : C.orange,
      keepColor: marked,
      bold: marked
    }, {
      t: h.name,
      w: HCOL.name,
      color: marked ? markCol : C.fg,
      keepColor: marked,
      bold: marked
    }, {
      t: h.type,
      w: HCOL.type,
      color: h.type === 'SSH' ? C.blue : C.magenta
    }, {
      t: h.addr,
      w: HCOL.addr,
      color: C.fg
    }, {
      t: String(h.port),
      w: HCOL.port,
      align: 'right',
      color: C.muted
    }, {
      t: '',
      w: HCOL.gap
    }, {
      t: h.mount,
      w: HCOL.mount,
      color: h.mountAuto === false ? C.fg : C.muted
    }, {
      t: h.login,
      w: HCOL.login,
      color: C.fg
    }, {
      t: maskPass(h.pass),
      w: HCOL.pass,
      color: h.pass ? C.muted : C.disabled
    }, {
      t: h.key ? '✓' : '[gen]',
      w: HCOL.key,
      align: 'center',
      color: h.key ? C.green : C.orangeB,
      keepColor: !h.key,
      cls: h.key ? '' : 'oa-genkey'
    }, {
      t: h.mounted ? '●' : '○',
      w: HCOL.mnt,
      align: 'center',
      color: h.mounted ? C.green : C.faint,
      keepColor: h.mounted
    }, {
      t: h.proxy ? '●' : '○',
      w: HCOL.prx,
      align: 'center',
      color: h.proxy ? C.orange : C.faint,
      keepColor: h.proxy
    }];
    return /*#__PURE__*/React.createElement(BodyRow, {
      key: h.id,
      w: COLS,
      inner: inner,
      focus: true,
      bg: cur ? C.orange : marked ? 'var(--bg-sel)' : hov ? 'var(--bg-hover)' : null,
      ink: cur ? C.ink : null,
      onClick: e => {
        onCursor(idx);
      },
      onDoubleClick: () => onOpen(idx),
      onMouseEnter: () => onHover(idx),
      onMouseLeave: () => onHover(-1),
      className: "oa-row"
    });
  }), Array.from({
    length: Math.max(0, HROWS - win.length)
  }).map((_, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: 'p' + i,
    w: COLS,
    inner: [],
    focus: true
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: COLS,
    focus: true
  }));
}
window.HostsScreen = HostsScreen;
window.HCOL = HCOL;
window.maskPass = maskPass;
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/openadmin/HostsScreen.jsx", error: String((e && e.message) || e) }); }

// ui_kits/openadmin/ShellsScreen.jsx
try { (() => {
/* ShellsScreen.jsx — screen 2: open shells as tabs.
   A tab opened from a single host is named after that host. A tab opened with
   several hosts marked is named "Group: <first host>" and stacks one pane per
   host vertically, each with its own titled frame and a live prompt. */

const LINE_COLOR = {
  p: C.green,
  c: C.bright,
  d: C.fg,
  ok: C.green,
  warn: C.yellow,
  err: C.red
};
const PANE_TOTAL = 22; // rows available to the pane stack

function TabBar({
  tabs,
  active,
  onSelect,
  onClose
}) {
  if (!tabs.length) return null;
  return /*#__PURE__*/React.createElement("div", {
    className: "oa-tabbar"
  }, tabs.map(t => /*#__PURE__*/React.createElement("span", {
    key: t.id,
    className: 'oa-shtab' + (t.id === active ? ' oa-shtab-on' : ''),
    onClick: () => onSelect(t.id),
    title: t.hosts.join(', ')
  }, /*#__PURE__*/React.createElement("span", {
    className: "oa-shtab-dot",
    style: {
      color: t.group ? 'var(--yellow-mark)' : 'var(--green)'
    }
  }, "\u25CF"), /*#__PURE__*/React.createElement("span", {
    className: "oa-shtab-lab"
  }, t.title), t.group && /*#__PURE__*/React.createElement("span", {
    className: "oa-shtab-ct"
  }, t.hosts.length), /*#__PURE__*/React.createElement("span", {
    className: "oa-shtab-x",
    title: "close shell",
    onClick: e => {
      e.stopPropagation();
      onClose(t.id);
    }
  }, "\xD7"))));
}

/* one terminal pane: titled frame + scrollback + prompt */
function ShellPane({
  host,
  lines,
  rows,
  focused,
  onFocus
}) {
  const body = [];
  // flatten [kind,text] pairs into rows, merging a prompt with its command
  let i = 0;
  while (i < lines.length) {
    const [k, txt] = lines[i];
    if (k === 'p' && lines[i + 1] && lines[i + 1][0] === 'c') {
      body.push([{
        t: txt,
        color: LINE_COLOR.p
      }, {
        t: lines[i + 1][1],
        color: LINE_COLOR.c
      }]);
      i += 2;
    } else if (k === 'p') {
      body.push([{
        t: txt,
        color: LINE_COLOR.p
      }, {
        t: '█',
        color: C.orange,
        cls: focused ? 'tui-cursor' : ''
      }]);
      i++;
    } else {
      body.push([{
        t: txt,
        color: LINE_COLOR[k] || C.fg
      }]);
      i++;
    }
  }
  const shown = body.slice(-rows);
  while (shown.length < rows) shown.push([]);
  return /*#__PURE__*/React.createElement("div", {
    onClick: onFocus,
    className: "oa-pane"
  }, /*#__PURE__*/React.createElement(TopBorder, {
    w: COLS,
    title: host,
    focus: focused,
    right: focused ? 'active' : 'click to focus'
  }), shown.map((inner, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: COLS,
    inner: inner,
    focus: focused
  })));
}
function EmptyShells({
  onGo
}) {
  const rows = [[], [], [{
    t: '  No open shells.',
    color: C.muted
  }], [], [{
    t: '  Go to ',
    color: C.faint
  }, {
    t: 'Hosts',
    color: C.orange
  }, {
    t: ' and press ',
    color: C.faint
  }, {
    t: 'F5',
    color: C.orangeB
  }, {
    t: ' on a host to open one.',
    color: C.faint
  }], [], [{
    t: '  Mark several hosts with ',
    color: C.faint
  }, {
    t: 'Insert',
    color: C.mark
  }, {
    t: ' first and F5 opens a single',
    color: C.faint
  }], [{
    t: '  grouped tab with one stacked pane per host.',
    color: C.faint
  }]];
  return /*#__PURE__*/React.createElement(React.Fragment, null, /*#__PURE__*/React.createElement(TopBorder, {
    w: COLS,
    title: "Shells",
    focus: true
  }), rows.map((r, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: i,
    w: COLS,
    inner: r,
    focus: true
  })), Array.from({
    length: 14
  }).map((_, i) => /*#__PURE__*/React.createElement(BodyRow, {
    key: 'p' + i,
    w: COLS,
    inner: [],
    focus: true
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: COLS,
    focus: true
  }));
}
function ShellsScreen({
  tabs,
  active,
  paneFocus,
  onSelect,
  onClose,
  onPaneFocus
}) {
  const tab = tabs.find(t => t.id === active);
  if (!tab) return /*#__PURE__*/React.createElement(EmptyShells, null);
  const n = tab.hosts.length;
  // split the available rows between stacked panes (2 rows of chrome per pane)
  const per = Math.max(3, Math.floor((PANE_TOTAL - n * 1) / n) - 1);
  return /*#__PURE__*/React.createElement("div", {
    className: "oa-shells"
  }, /*#__PURE__*/React.createElement(TabBar, {
    tabs: tabs,
    active: active,
    onSelect: onSelect,
    onClose: onClose
  }), tab.hosts.map((h, i) => /*#__PURE__*/React.createElement(ShellPane, {
    key: h,
    host: h,
    lines: tab.lines[h] || [],
    rows: per,
    focused: n === 1 || paneFocus === i,
    onFocus: () => onPaneFocus(i)
  })), /*#__PURE__*/React.createElement(BotBorder, {
    w: COLS,
    focus: true
  }));
}
Object.assign(window, {
  ShellsScreen,
  TabBar,
  ShellPane,
  EmptyShells
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/openadmin/ShellsScreen.jsx", error: String((e && e.message) || e) }); }

// ui_kits/openadmin/app.jsx
try { (() => {
/* app.jsx — OpenAdmin: state, keyboard routing, mouse affordances. */

function App() {
  const [hosts, setHosts] = React.useState(HOSTS);
  const [cursor, setCursor] = React.useState(0);
  const [marks, setMarks] = React.useState(new Set());
  const [hovered, setHover] = React.useState(-1);
  const [screen, setScreen] = React.useState('hosts');
  const [modal, setModal] = React.useState(null); // null|add|edit|delete|key|help
  const [form, setForm] = React.useState(null);
  const [ffocus, setFfocus] = React.useState('name');
  const [tabs, setTabs] = React.useState([]);
  const [activeTab, setTab] = React.useState(null);
  const [paneFocus, setPane] = React.useState(0);
  const [msgs, setMsgs] = React.useState(CHAT);
  const [draft, setDraft] = React.useState('');
  const [busy, setBusy] = React.useState(false);
  const [msg, setMsg] = React.useState({
    text: 'Alt+1/2/3 switch screens · Insert marks hosts · F1 help',
    type: ''
  });
  const tmr = React.useRef();
  const flash = (text, type = 'ok') => {
    setMsg({
      text,
      type
    });
    clearTimeout(tmr.current);
    tmr.current = setTimeout(() => setMsg({
      text: 'Ready.',
      type: ''
    }), 3400);
  };
  const host = hosts[cursor] || null;
  const markedHosts = hosts.filter(h => marks.has(h.id));
  const targets = markedHosts.length ? markedHosts : host ? [host] : [];
  const clamp = i => setCursor(Math.max(0, Math.min(i, hosts.length - 1)));

  /* ---------------- host actions ---------------- */
  const openAdd = () => {
    setForm({
      name: '',
      type: 'SSH',
      addr: '',
      port: 22,
      mount: '',
      mountAuto: true,
      login: '',
      pass: '',
      key: false
    });
    setFfocus('name');
    setModal('add');
  };
  const openEdit = () => {
    if (!host) return;
    setForm({
      ...host,
      mountAuto: host.mountAuto !== false
    });
    setFfocus('name');
    setModal('edit');
  };
  /* name drives the mount point until the user edits the mount field */
  const setFormName = name => setForm(f => ({
    ...f,
    name,
    mount: f.mountAuto ? mountFor(name) : f.mount
  }));
  const setFormMount = mount => setForm(f => ({
    ...f,
    mount,
    mountAuto: mount.trim() === '' ? true : false
  }));
  const cycleType = d => setForm(f => {
    const i = HOST_TYPES.indexOf(f.type);
    const t = HOST_TYPES[(i + d + HOST_TYPES.length) % HOST_TYPES.length];
    return {
      ...f,
      type: t,
      port: DEFAULT_PORT[t]
    };
  });
  const saveForm = () => {
    if (!form.name.trim() || !form.addr.trim()) {
      flash('Host name and address are required.', 'err');
      return;
    }
    const rec = {
      ...form,
      id: form.id || nid('h'),
      name: form.name.trim(),
      addr: form.addr.trim(),
      port: parseInt(form.port, 10) || DEFAULT_PORT[form.type],
      mount: (form.mount || '').trim() || mountFor(form.name),
      mounted: form.mounted || false,
      proxy: form.proxy || false
    };
    if (modal === 'edit') {
      setHosts(hs => hs.map(h => h.id === rec.id ? rec : h));
      flash(`Saved ${rec.name}.`);
    } else {
      setHosts(hs => [...hs, rec]);
      setCursor(hosts.length);
      flash(`Added ${rec.name}.`);
    }
    setModal(null);
  };
  const doDelete = () => {
    const ids = new Set(targets.map(t => t.id));
    setHosts(hs => hs.filter(h => !ids.has(h.id)));
    setMarks(new Set());
    flash(`Deleted ${ids.size} host${ids.size > 1 ? 's' : ''}.`);
    setModal(null);
    clamp(cursor);
  };
  const toggleMount = () => {
    if (!targets.length) return;
    const ids = new Set(targets.map(t => t.id));
    const anyUnmounted = targets.some(t => !t.mounted);
    setHosts(hs => hs.map(h => ids.has(h.id) ? {
      ...h,
      mounted: anyUnmounted
    } : h));
    flash(`${anyUnmounted ? 'Mounted' : 'Unmounted'} ${ids.size} host${ids.size > 1 ? 's' : ''}.`);
  };
  const toggleProxy = () => {
    if (!host) return;
    setHosts(hs => hs.map(h => h.id === host.id ? {
      ...h,
      proxy: !h.proxy
    } : h));
    flash(host.proxy ? `${host.name} no longer used as proxy.` : `Routing through ${host.name}.`);
  };
  const genKey = h => {
    const t = h || host;
    if (!t) return;
    setHosts(hs => hs.map(x => x.id === t.id ? {
      ...x,
      key: true
    } : x));
    setModal('key');
  };
  const toggleMark = () => {
    if (!host) return;
    setMarks(m => {
      const n = new Set(m);
      n.has(host.id) ? n.delete(host.id) : n.add(host.id);
      return n;
    });
    clamp(cursor + 1);
  };

  /* ---------------- shells ---------------- */
  const openShell = () => {
    const sel = markedHosts.length ? markedHosts : host ? [host] : [];
    if (!sel.length) return;
    const group = sel.length > 1;
    const names = sel.map(h => h.name);
    const lines = {};
    names.forEach(n => {
      lines[n] = SHELL_OUTPUT[n] || GENERIC_OUTPUT(n);
    });
    const tab = {
      id: nid('t'),
      title: group ? `Group: ${names[0]}` : names[0],
      hosts: names,
      group,
      lines
    };
    setTabs(ts => [...ts, tab]);
    setTab(tab.id);
    setPane(0);
    setScreen('shells');
    setMarks(new Set());
    flash(group ? `Opened ${names.length} shells in one tab.` : `Shell open on ${names[0]}.`);
  };
  const closeTab = id => {
    setTabs(ts => {
      const n = ts.filter(t => t.id !== id);
      if (activeTab === id) setTab(n.length ? n[n.length - 1].id : null);
      return n;
    });
  };

  /* ---------------- chat ---------------- */
  const sendChat = () => {
    if (!draft.trim()) return;
    const text = draft.trim();
    setMsgs(m => [...m, {
      role: 'user',
      text
    }]);
    setDraft('');
    setBusy(true);
    setTimeout(() => {
      setMsgs(m => [...m, {
        role: 'tool',
        name: 'ssh',
        arg: 'web-01 · scp web-02:/srv/api/.env /tmp/env.ref',
        status: 'ok',
        out: ['/tmp/env.ref  1.2 KB  100%']
      }, {
        role: 'assistant',
        text: 'Pulled the reference env from web-02 and diffed it — `DATABASE_URL` is the only missing key. Restoring it and restarting `api.service` now.'
      }]);
      setBusy(false);
    }, 1400);
  };

  /* ---------------- keyboard ---------------- */
  React.useEffect(() => {
    const onKey = e => {
      const k = e.key;
      if (e.altKey && ['1', '2', '3'].includes(k)) {
        e.preventDefault();
        setScreen(SCREENS[+k - 1].id);
        return;
      }
      if (k === 'F9') {
        e.preventDefault();
        setScreen(s => SCREENS[(SCREENS.findIndex(x => x.id === s) + 1) % 3].id);
        return;
      }
      if (k === 'F1') {
        e.preventDefault();
        setModal(m => m === 'help' ? null : 'help');
        return;
      }
      if (k === 'F10') {
        e.preventDefault();
        flash('Quit — close the tab. (demo)', 'warn');
        return;
      }
      if (modal === 'help' || modal === 'key') {
        if (k === 'Escape') {
          e.preventDefault();
          setModal(null);
        }
        return;
      }
      if (modal === 'delete') {
        if (k === 'Escape' || k === 'n' || k === 'N') {
          e.preventDefault();
          setModal(null);
        } else if (k === 'Enter' || k === 'y' || k === 'Y') {
          e.preventDefault();
          doDelete();
        }
        return;
      }
      if (modal === 'add' || modal === 'edit') {
        if (k === 'Escape') {
          e.preventDefault();
          setModal(null);
          return;
        }
        if (k === 'Enter') {
          e.preventDefault();
          saveForm();
          return;
        }
        if (k === 'Tab') {
          e.preventDefault();
          const i = FORM_FIELDS.indexOf(ffocus);
          setFfocus(FORM_FIELDS[(i + (e.shiftKey ? -1 : 1) + FORM_FIELDS.length) % FORM_FIELDS.length]);
          return;
        }
        if (ffocus === 'type') {
          if (k === 'ArrowRight') {
            e.preventDefault();
            cycleType(1);
          } else if (k === 'ArrowLeft') {
            e.preventDefault();
            cycleType(-1);
          }
          return;
        }
        if (k === 'Backspace') {
          e.preventDefault();
          if (ffocus === 'name') setFormName(String(form.name).slice(0, -1));else if (ffocus === 'mount') setFormMount(String(form.mount).slice(0, -1));else setForm(f => ({
            ...f,
            [ffocus]: String(f[ffocus] ?? '').slice(0, -1)
          }));
          return;
        }
        if (k.length === 1 && !e.metaKey && !e.ctrlKey) {
          if (ffocus === 'port' && !/\d/.test(k)) return;
          e.preventDefault();
          if (ffocus === 'name') setFormName(String(form.name) + k);else if (ffocus === 'mount') setFormMount(String(form.mount) + k);else setForm(f => ({
            ...f,
            [ffocus]: String(f[ffocus] ?? '') + k
          }));
        }
        return;
      }
      if (screen === 'hosts') {
        if (k === 'ArrowDown') {
          e.preventDefault();
          clamp(cursor + 1);
        } else if (k === 'ArrowUp') {
          e.preventDefault();
          clamp(cursor - 1);
        } else if (k === 'Home') {
          e.preventDefault();
          clamp(0);
        } else if (k === 'End') {
          e.preventDefault();
          clamp(hosts.length - 1);
        } else if (k === 'Insert' || k === ' ') {
          e.preventDefault();
          toggleMark();
        } else if (k === '*') {
          e.preventDefault();
          setMarks(m => new Set(hosts.filter(h => !m.has(h.id)).map(h => h.id)));
        } else if ((e.ctrlKey || e.metaKey) && k === 'a') {
          e.preventDefault();
          setMarks(new Set(hosts.map(h => h.id)));
        } else if (k === 'Escape') {
          e.preventDefault();
          setMarks(new Set());
        } else if (k === 'Enter' || k === 'F3') {
          e.preventDefault();
          openEdit();
        } else if (k === 'F2') {
          e.preventDefault();
          openAdd();
        } else if (k === 'F4') {
          e.preventDefault();
          toggleMount();
        } else if (k === 'F5') {
          e.preventDefault();
          openShell();
        } else if (k === 'F6') {
          e.preventDefault();
          toggleProxy();
        } else if (k === 'F7') {
          e.preventDefault();
          genKey();
        } else if (k === 'F8' || k === 'Delete') {
          e.preventDefault();
          if (targets.length) setModal('delete');
        }
        return;
      }
      if (screen === 'shells') {
        if (k === 'Tab' && tabs.length) {
          e.preventDefault();
          const i = tabs.findIndex(t => t.id === activeTab);
          setTab(tabs[(i + (e.shiftKey ? -1 : 1) + tabs.length) % tabs.length].id);
        } else if (k === 'F4' && activeTab) {
          e.preventDefault();
          closeTab(activeTab);
          flash('Shell closed.');
        }
        return;
      }
      if (screen === 'chat') {
        if (k === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          sendChat();
          return;
        }
        if (k === 'Backspace') {
          e.preventDefault();
          setDraft(d => d.slice(0, -1));
          return;
        }
        if (k.length === 1 && !e.metaKey && !e.ctrlKey) {
          e.preventDefault();
          setDraft(d => d + k);
        }
        return;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  /* ---------------- chrome ---------------- */
  const nm = marks.size;
  const fkeys = () => {
    if (modal === 'add' || modal === 'edit') return [{
      key: 'Esc',
      label: 'Cancel',
      onClick: () => setModal(null)
    }, {
      key: '↹',
      label: 'Next field'
    }, {
      key: '↵',
      label: modal === 'edit' ? 'Save' : 'Add',
      onClick: saveForm,
      active: true
    }];
    if (modal === 'delete') return [{
      key: 'N',
      label: 'Cancel',
      onClick: () => setModal(null)
    }, {
      key: 'Y',
      label: 'Delete',
      danger: true,
      onClick: doDelete,
      active: true
    }];
    if (modal) return [{
      key: 'Esc',
      label: 'Close',
      onClick: () => setModal(null),
      active: true
    }];
    if (screen === 'hosts') return [{
      key: 'F1',
      label: 'Help',
      onClick: () => setModal('help')
    }, {
      key: 'F2',
      label: 'Add',
      onClick: openAdd
    }, {
      key: 'F3',
      label: 'Edit',
      onClick: openEdit
    }, {
      key: 'F4',
      label: targets.some(t => !t.mounted) ? 'Mount' : 'Unmount',
      onClick: toggleMount
    }, {
      key: 'F5',
      label: nm > 1 ? `Shell ×${nm}` : 'Shell',
      onClick: openShell,
      active: nm > 1
    }, {
      key: 'F6',
      label: 'Proxy',
      onClick: toggleProxy
    }, {
      key: 'F7',
      label: 'GenKey',
      onClick: () => genKey()
    }, {
      key: 'F8',
      label: 'Delete',
      danger: true,
      onClick: () => targets.length && setModal('delete')
    }, {
      key: 'Ins',
      label: nm ? `Marked ${nm}` : 'Mark',
      onClick: toggleMark,
      active: !!nm
    }, {
      key: 'F10',
      label: 'Quit',
      onClick: () => flash('Quit — close the tab. (demo)', 'warn')
    }];
    if (screen === 'shells') return [{
      key: 'F1',
      label: 'Help',
      onClick: () => setModal('help')
    }, {
      key: '↹',
      label: 'Next tab'
    }, {
      key: 'F4',
      label: 'Close tab',
      onClick: () => activeTab && closeTab(activeTab),
      disabled: !activeTab
    }, {
      key: 'F5',
      label: 'New shell',
      onClick: () => setScreen('hosts')
    }, {
      key: 'F9',
      label: 'Screen',
      onClick: () => setScreen('chat')
    }, {
      key: 'F10',
      label: 'Quit',
      onClick: () => flash('Quit — close the tab. (demo)', 'warn')
    }];
    return [{
      key: 'F1',
      label: 'Help',
      onClick: () => setModal('help')
    }, {
      key: '↵',
      label: 'Send',
      onClick: sendChat,
      active: true
    }, {
      key: '^R',
      label: 'Run command'
    }, {
      key: '@',
      label: 'Add context'
    }, {
      key: 'F9',
      label: 'Screen',
      onClick: () => setScreen('hosts')
    }, {
      key: 'F10',
      label: 'Quit',
      onClick: () => flash('Quit — close the tab. (demo)', 'warn')
    }];
  };

  /* hover-driven hint — a real use of terminal mouse reporting */
  const hoverHost = hovered >= 0 ? hosts[hovered] : null;
  const hint = screen === 'hosts' && hoverHost ? /*#__PURE__*/React.createElement(React.Fragment, null, hoverHost.login, "@", hoverHost.addr, ":", hoverHost.port, " \xB7 ", hoverHost.mounted ? 'mounted at ' + hoverHost.mount : 'not mounted', " \xB7 double-click for a shell") : null;
  const left = screen === 'hosts' ? /*#__PURE__*/React.createElement(React.Fragment, null, hosts.length, " hosts \xB7 ", hosts.filter(h => h.mounted).length, " mounted", nm ? /*#__PURE__*/React.createElement(React.Fragment, null, " \xB7 ", /*#__PURE__*/React.createElement("b", {
    className: "oa-marked"
  }, nm, " marked")) : null) : screen === 'shells' ? /*#__PURE__*/React.createElement(React.Fragment, null, tabs.length, " shell", tabs.length === 1 ? '' : 's', " open") : /*#__PURE__*/React.createElement(React.Fragment, null, "agent \xB7 claude-sonnet-4.5");
  return /*#__PURE__*/React.createElement(Screen, null, /*#__PURE__*/React.createElement(Header, {
    screen: screen,
    onScreen: setScreen,
    counts: {
      hosts: hosts.length,
      shells: tabs.length,
      chat: null
    }
  }), /*#__PURE__*/React.createElement("div", {
    className: modal ? 'oa-dim' : ''
  }, screen === 'hosts' && /*#__PURE__*/React.createElement(HostsScreen, {
    hosts: hosts,
    cursor: cursor,
    marks: marks,
    hovered: hovered,
    onCursor: clamp,
    onHover: setHover,
    onToggleMark: toggleMark,
    onGenKey: genKey,
    onOpen: i => {
      clamp(i);
      setTimeout(openShell, 0);
    }
  }), screen === 'shells' && /*#__PURE__*/React.createElement(ShellsScreen, {
    tabs: tabs,
    active: activeTab,
    paneFocus: paneFocus,
    onSelect: setTab,
    onClose: closeTab,
    onPaneFocus: setPane
  }), screen === 'chat' && /*#__PURE__*/React.createElement(ChatScreen, {
    msgs: msgs,
    draft: draft,
    busy: busy,
    focused: true,
    model: "claude-sonnet-4.5"
  })), /*#__PURE__*/React.createElement(StatusBar, {
    left: left,
    hint: hint,
    message: msg.text,
    messageType: msg.type,
    busy: false
  }), /*#__PURE__*/React.createElement(FunctionBar, {
    items: fkeys()
  }), (modal === 'add' || modal === 'edit') && /*#__PURE__*/React.createElement(HostDialog, {
    mode: modal,
    form: form,
    focus: ffocus,
    onFocus: setFfocus,
    onCycleType: cycleType,
    onSave: saveForm,
    onCancel: () => setModal(null),
    onGenKey: () => {
      setForm(f => ({
        ...f,
        key: true
      }));
      setModal('key');
    }
  }), modal === 'delete' && targets.length > 0 && /*#__PURE__*/React.createElement(DeleteDialog, {
    targets: targets,
    onConfirm: doDelete,
    onCancel: () => setModal(null)
  }), modal === 'key' && (host || form) && /*#__PURE__*/React.createElement(KeyDialog, {
    host: form && modal === 'key' && form.name ? form : host,
    onCancel: () => setModal(null),
    onCopy: () => {
      setModal(null);
      flash('Public key copied to clipboard.');
    }
  }), modal === 'help' && /*#__PURE__*/React.createElement(HelpDialog, {
    onCancel: () => setModal(null)
  }));
}
ReactDOM.createRoot(document.getElementById('root')).render(/*#__PURE__*/React.createElement(App, null));
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/openadmin/app.jsx", error: String((e && e.message) || e) }); }

// ui_kits/openadmin/data.js
try { (() => {
/* data.js — OpenAdmin sample hosts, shell output, and chat transcript. */

let _id = 0;
const nid = p => p + ++_id;
const HOST_TYPES = ['SSH', 'FTP'];
const DEFAULT_PORT = {
  SSH: 22,
  FTP: 21
};

/* mount point is derived from the nickname unless the user overrode it */
const mountFor = name => '/net/' + String(name || '').trim().replace(/\s+/g, '-').toLowerCase();
const HOSTS = [{
  id: nid('h'),
  name: 'web-01',
  type: 'SSH',
  addr: '10.0.4.11',
  port: 22,
  login: 'deploy',
  pass: 'hunter2',
  key: true,
  mount: '/net/web-01',
  mounted: true,
  proxy: false
}, {
  id: nid('h'),
  name: 'web-02',
  type: 'SSH',
  addr: '10.0.4.12',
  port: 22,
  login: 'deploy',
  pass: 'hunter2',
  key: true,
  mount: '/net/web-02',
  mounted: true,
  proxy: false
}, {
  id: nid('h'),
  name: 'db-main',
  type: 'SSH',
  addr: '10.0.8.3',
  port: 22,
  login: 'postgres',
  pass: 's3cret',
  key: true,
  mount: '/net/db-main',
  mounted: false,
  proxy: false
}, {
  id: nid('h'),
  name: 'bastion',
  type: 'SSH',
  addr: 'edge.corp.net',
  port: 2222,
  login: 'jump',
  pass: '',
  key: true,
  mount: '/net/bastion',
  mounted: false,
  proxy: true
}, {
  id: nid('h'),
  name: 'build-rig',
  type: 'SSH',
  addr: '192.168.50.20',
  port: 22,
  login: 'ci',
  pass: 'buildpass',
  key: false,
  mount: '/net/build-rig',
  mounted: true,
  proxy: false
}, {
  id: nid('h'),
  name: 'nas',
  type: 'FTP',
  addr: '192.168.1.240',
  port: 21,
  login: 'media',
  pass: 'nasnas',
  key: false,
  mount: '/net/nas',
  mounted: true,
  proxy: false
}, {
  id: nid('h'),
  name: 'archive',
  type: 'FTP',
  addr: 'ftp.archive.lan',
  port: 21,
  login: 'anon',
  pass: '',
  key: false,
  mount: '/net/archive',
  mounted: false,
  proxy: false
}, {
  id: nid('h'),
  name: 'staging',
  type: 'SSH',
  addr: '10.0.9.41',
  port: 22,
  login: 'deploy',
  pass: 'stagepass',
  key: true,
  mount: '/net/staging',
  mounted: false,
  proxy: false
}, {
  id: nid('h'),
  name: 'metrics',
  type: 'SSH',
  addr: '10.0.12.7',
  port: 22,
  login: 'grafana',
  pass: 'dash',
  key: true,
  mount: '/net/metrics',
  mounted: true,
  proxy: false
}, {
  id: nid('h'),
  name: 'sandbox',
  type: 'SSH',
  addr: '172.16.0.99',
  port: 22,
  login: 'root',
  pass: 'toor',
  key: false,
  mount: '/srv/sandbox',
  mounted: false,
  proxy: false
}];
const PUBKEY = 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJ1kQm7vX2pLd8rTgYs4NwBqZ0cHf6EaVuMxKpR9tSbC openadmin@workstation';

/* fake shell scrollback, keyed by host name */
const SHELL_OUTPUT = {
  'web-01': [['p', 'deploy@web-01:~$ '], ['c', 'systemctl status nginx'], ['ok', '● nginx.service - A high performance web server'], ['d', '     Loaded: loaded (/lib/systemd/system/nginx.service; enabled)'], ['ok', '     Active: active (running) since Tue 09:14:02 UTC; 3 days ago'], ['d', '   Main PID: 1421 (nginx)'], ['d', '      Tasks: 5 (limit: 4915)'], ['p', 'deploy@web-01:~$ '], ['c', 'tail -n2 /var/log/nginx/access.log'], ['d', '10.0.4.1 - - [11/Sep/2026:08:22:14] "GET /health HTTP/1.1" 200 2'], ['d', '10.0.4.1 - - [11/Sep/2026:08:22:19] "GET /api/v2/items HTTP/1.1" 200 8841'], ['p', 'deploy@web-01:~$ ']],
  'web-02': [['p', 'deploy@web-02:~$ '], ['c', 'uptime'], ['d', ' 08:23:04 up 12 days,  4:11,  1 user,  load average: 0.18, 0.24, 0.21'], ['p', 'deploy@web-02:~$ ']],
  'db-main': [['p', 'postgres@db-main:~$ '], ['c', 'psql -c "select count(*) from orders;"'], ['d', ' count '], ['d', '-------'], ['d', '  48213'], ['d', '(1 row)'], ['p', 'postgres@db-main:~$ ']],
  'build-rig': [['p', 'ci@build-rig:~$ '], ['c', 'cargo build --release'], ['d', '   Compiling openadmin v0.4.1'], ['warn', 'warning: unused variable: `ctx`'], ['ok', '    Finished release [optimized] target(s) in 41.28s'], ['p', 'ci@build-rig:~$ ']]
};
const GENERIC_OUTPUT = name => [['p', 'user@' + name + ':~$ '], ['c', 'uname -a'], ['d', 'Linux ' + name + ' 6.8.0-45-generic #45-Ubuntu SMP x86_64 GNU/Linux'], ['p', 'user@' + name + ':~$ ']];

/* agentic chat transcript — opencode-inspired: turns carry tool calls */
const CHAT = [{
  role: 'user',
  text: 'nginx on web-01 is throwing 502s since the deploy. find out why.'
}, {
  role: 'tool',
  name: 'ssh',
  arg: 'web-01 · journalctl -u nginx -n 50',
  status: 'ok',
  out: ['connect() failed (111: Connection refused) while connecting to upstream', 'upstream: "http://127.0.0.1:8080/api/v2/items"']
}, {
  role: 'assistant',
  text: 'nginx is fine — it cannot reach the upstream on :8080. Checking whether the app service is listening.'
}, {
  role: 'tool',
  name: 'ssh',
  arg: 'web-01 · ss -lntp | grep 8080',
  status: 'empty',
  out: ['(no output)']
}, {
  role: 'tool',
  name: 'ssh',
  arg: 'web-01 · systemctl status api',
  status: 'fail',
  out: ['● api.service - Items API', '     Active: failed (Result: exit-code)', '   Process: 2214 ExecStart=/srv/api/bin/server (code=exited, status=1/FAILURE)', 'api[2214]: FATAL: config key "DATABASE_URL" missing']
}, {
  role: 'assistant',
  text: 'Root cause: `api.service` on web-01 is down — it exits at boot because `DATABASE_URL` is missing from its environment file. The deploy replaced /srv/api/.env and dropped that key.\n\nI can restore it from web-02, which is running the same release, then restart the unit. Want me to apply it?'
}];
Object.assign(window, {
  HOST_TYPES,
  DEFAULT_PORT,
  mountFor,
  HOSTS,
  PUBKEY,
  SHELL_OUTPUT,
  GENERIC_OUTPUT,
  CHAT,
  nid
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/openadmin/data.js", error: String((e && e.message) || e) }); }

// ui_kits/openadmin/tui.jsx
try { (() => {
/* tui.jsx — character-grid primitives for OpenAdmin (shared vocabulary with the
   cfdns kit, widened to 120 columns for the hosts table). Rows are
   <div class="tui-row"> with white-space:pre so box-drawing glyphs connect;
   coloring is inline <span> which does not disturb the grid. */

const COLS = 120;
const C = {
  fg: 'var(--fg)',
  bright: 'var(--fg-bright)',
  muted: 'var(--fg-muted)',
  faint: 'var(--fg-faint)',
  disabled: 'var(--fg-disabled)',
  orange: 'var(--orange)',
  orangeB: 'var(--orange-bright)',
  orangeD: 'var(--orange-dim)',
  ink: 'var(--orange-ink)',
  green: 'var(--green)',
  red: 'var(--red)',
  blue: 'var(--blue)',
  yellow: 'var(--yellow)',
  mark: 'var(--yellow-mark)',
  magenta: 'var(--magenta)',
  line: 'var(--line)',
  inset: 'var(--bg-inset)'
};
const SETS = {
  single: ['┌', '┐', '└', '┘', '─', '│', '├', '┤'],
  double: ['╔', '╗', '╚', '╝', '═', '║', '╠', '╣']
};
function pad(s, w, align) {
  s = s == null ? '' : String(s);
  if (w == null) return s;
  if (s.length > w) return s.length > 1 ? s.slice(0, Math.max(0, w - 1)) + '…' : s.slice(0, w);
  const gap = w - s.length;
  if (align === 'right') return ' '.repeat(gap) + s;
  if (align === 'center') return ' '.repeat(gap >> 1) + s + ' '.repeat(gap - (gap >> 1));
  return s + ' '.repeat(gap);
}
function buildSpans(segs, w) {
  let fixed = 0,
    fills = 0;
  for (const s of segs) {
    if (s.fill) fills++;else fixed += s.w != null ? s.w : s.t == null ? 0 : String(s.t).length;
  }
  const rem = Math.max(0, (w != null ? w : fixed) - fixed);
  const per = fills ? Math.floor(rem / fills) : 0;
  let extra = rem - per * fills,
    fi = 0;
  return segs.map((s, i) => {
    let t;
    if (s.fill) {
      const ww = per + (fi === fills - 1 ? extra : 0);
      fi++;
      t = (s.ch || ' ').repeat(ww);
    } else if (s.w != null) t = pad(s.t, s.w, s.align);else t = s.t == null ? '' : String(s.t);
    const st = {};
    if (s.color) st.color = s.color;
    if (s.bg) st.background = s.bg;
    if (s.bold) st.fontWeight = 500;
    if (s.ul) st.textDecoration = 'underline';
    return React.createElement('span', {
      key: i,
      className: s.cls || '',
      style: st
    }, t);
  });
}
function Row({
  segs,
  w = COLS,
  style,
  className,
  onClick,
  onDoubleClick,
  onMouseEnter,
  onMouseLeave,
  title
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: 'tui-row ' + (className || ''),
    style: style,
    title: title,
    onClick: onClick,
    onDoubleClick: onDoubleClick,
    onMouseEnter: onMouseEnter,
    onMouseLeave: onMouseLeave
  }, buildSpans(segs, w));
}
const bcOf = (focus, double) => focus ? C.orange : double ? C.orangeD : C.line;
function TopBorder({
  w = COLS,
  title,
  focus,
  double,
  right
}) {
  const [tl, tr,,, H] = SETS[double ? 'double' : 'single'];
  const bc = bcOf(focus, double);
  const segs = [{
    t: tl + H,
    color: bc
  }];
  if (title) {
    segs.push({
      t: ' ',
      color: bc
    }, {
      t: title,
      color: focus ? C.bright : C.muted,
      bold: true
    }, {
      t: ' ',
      color: bc
    });
  }
  segs.push({
    fill: true,
    ch: H,
    color: bc
  });
  if (right) segs.push({
    t: ' ' + right + ' ',
    color: C.faint
  });
  segs.push({
    t: H + tr,
    color: bc
  });
  return /*#__PURE__*/React.createElement(Row, {
    w: w,
    segs: segs
  });
}
function SepBorder({
  w = COLS,
  focus,
  double
}) {
  const set = SETS[double ? 'double' : 'single'];
  const bc = bcOf(focus, double);
  return /*#__PURE__*/React.createElement(Row, {
    w: w,
    segs: [{
      t: set[6],
      color: bc
    }, {
      fill: true,
      ch: set[4],
      color: bc
    }, {
      t: set[7],
      color: bc
    }]
  });
}
function BotBorder({
  w = COLS,
  focus,
  double
}) {
  const [,, bl, br, H] = SETS[double ? 'double' : 'single'];
  const bc = bcOf(focus, double);
  return /*#__PURE__*/React.createElement(Row, {
    w: w,
    segs: [{
      t: bl,
      color: bc
    }, {
      fill: true,
      ch: H,
      color: bc
    }, {
      t: br,
      color: bc
    }]
  });
}

/* A content row inside a frame: │ <pad> ...inner... <fill> │
   `bg` tints the whole inner region (reverse-video); `ink` overrides text color. */
function BodyRow({
  inner,
  w = COLS,
  focus,
  double,
  bg,
  ink,
  onClick,
  onDoubleClick,
  onMouseEnter,
  onMouseLeave,
  className,
  title
}) {
  const V = SETS[double ? 'double' : 'single'][5];
  const bc = bcOf(focus, double);
  const lead = {
      t: ' '
    },
    fill = {
      fill: true
    };
  let content = inner;
  if (bg) {
    content = inner.map(s => ({
      ...s,
      bg,
      color: s.keepColor ? s.color : ink || s.color
    }));
    lead.bg = bg;
    fill.bg = bg;
  }
  return /*#__PURE__*/React.createElement(Row, {
    w: w,
    segs: [{
      t: V,
      color: bc
    }, lead, ...content, fill, {
      t: V,
      color: bc
    }],
    onClick: onClick,
    onDoubleClick: onDoubleClick,
    onMouseEnter: onMouseEnter,
    onMouseLeave: onMouseLeave,
    className: className,
    title: title
  });
}

/* Center a fixed-width screen in the viewport, scaled to fit. */
function Screen({
  children
}) {
  const ref = React.useRef(null);
  React.useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const fit = () => {
      const nw = el.offsetWidth,
        nh = el.offsetHeight;
      if (!nw || !nh) return;
      let s = Math.min(window.innerWidth / nw, window.innerHeight / nh) * 0.96;
      el.style.transform = `translate(-50%,-50%) scale(${Math.min(s, 1.3)})`;
    };
    // always measure AFTER layout settles — reading inside the observer
    // callback can catch a stale box and produce a wildly wrong scale
    let raf = 0;
    const schedule = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(fit);
    };
    schedule();
    window.addEventListener('resize', schedule);
    const ro = new ResizeObserver(schedule);
    ro.observe(el);
    if (document.fonts && document.fonts.ready) document.fonts.ready.then(schedule);
    // low-frequency safety net: re-assert the transform if anything external
    // clears it (print/export passes, screenshot tooling, devtools edits)
    const heal = setInterval(fit, 1000);
    return () => {
      cancelAnimationFrame(raf);
      clearInterval(heal);
      window.removeEventListener('resize', schedule);
      ro.disconnect();
    };
  }, []);
  return /*#__PURE__*/React.createElement("div", {
    className: "tui-stage"
  }, /*#__PURE__*/React.createElement("div", {
    className: "tui-screen",
    ref: ref
  }, children));
}

/* text field / select / cursor helpers shared by dialogs */
function wellSegs(value, ph, width, focused) {
  const txt = value || (!focused ? ph : '');
  const used = 1 + txt.length + (focused ? 1 : 0);
  const segs = [{
    t: ' ',
    bg: C.inset
  }];
  if (txt) segs.push({
    t: txt,
    bg: C.inset,
    color: value ? C.fg : C.faint
  });
  if (focused) segs.push({
    t: '█',
    bg: C.inset,
    color: C.orange,
    cls: 'tui-cursor'
  });
  segs.push({
    t: ' '.repeat(Math.max(0, width - used)),
    bg: C.inset
  });
  return segs;
}
function selectSegs(value, width, focused, disabled) {
  const used = 1 + String(value).length;
  return [{
    t: ' ',
    bg: C.inset
  }, {
    t: value,
    bg: C.inset,
    color: disabled ? C.disabled : C.fg
  }, {
    t: ' '.repeat(Math.max(0, width - used - 2)),
    bg: C.inset
  }, {
    t: '▾ ',
    bg: C.inset,
    color: disabled ? C.disabled : focused ? C.orange : C.muted
  }];
}
const LABELW = 14;
const labelSeg = (label, focused) => ({
  t: label,
  w: LABELW,
  align: 'right',
  color: focused ? C.orange : C.muted
});
const fieldInner = (label, segs, focused) => [labelSeg(label, focused), {
  t: '  '
}, ...segs];
function Spinner() {
  const frames = '⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏';
  const [i, setI] = React.useState(0);
  React.useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const id = setInterval(() => setI(x => (x + 1) % frames.length), 80);
    return () => clearInterval(id);
  }, []);
  return /*#__PURE__*/React.createElement("span", {
    style: {
      color: 'var(--orange)'
    }
  }, frames[i]);
}
Object.assign(window, {
  COLS,
  C,
  pad,
  buildSpans,
  Row,
  TopBorder,
  SepBorder,
  BotBorder,
  BodyRow,
  Screen,
  wellSegs,
  selectSegs,
  fieldInner,
  labelSeg,
  LABELW,
  Spinner
});
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/openadmin/tui.jsx", error: String((e && e.message) || e) }); }

__ds_ns.FunctionBar = __ds_scope.FunctionBar;

__ds_ns.StatusBar = __ds_scope.StatusBar;

__ds_ns.TuiPanel = __ds_scope.TuiPanel;

})();
