/* ChatScreen.jsx — screen 3: agentic chat, opencode-inspired.
   A single transcript column: user turns are marked with an orange » prompt,
   assistant prose is plain, and tool calls render as collapsed boxes with a
   status glyph, the command, and its captured output indented under a tree rule. */

const TOOL_STATUS = {
  ok:    { g:'✓', c:C.green,  t:'ok' },
  fail:  { g:'×', c:C.red,    t:'exit 1' },
  empty: { g:'○', c:C.muted,  t:'no output' },
  run:   { g:'…', c:C.orange, t:'running' },
};

function wrap(text, width){
  const out = [];
  for (const para of String(text).split('\n')){
    if (!para){ out.push(''); continue; }
    let line = '';
    for (const word of para.split(' ')){
      if ((line+' '+word).trim().length > width){ out.push(line.trim()); line = word; }
      else line += ' ' + word;
    }
    if (line.trim()) out.push(line.trim());
  }
  return out;
}

function chatRows(msgs, width){
  const rows = [];
  const push = (segs) => rows.push(segs);

  msgs.forEach((m, mi) => {
    if (mi) push([]);
    if (m.role==='user'){
      wrap(m.text, width-4).forEach((ln, i) => push([
        { t: i===0 ? '» ' : '  ', color:C.orange, bold:true },
        { t: ln, color:C.bright },
      ]));
    }
    else if (m.role==='assistant'){
      wrap(m.text, width-4).forEach((ln) => push([{ t:'  ' }, { t:ln, color:C.fg }]));
    }
    else if (m.role==='tool'){
      const st = TOOL_STATUS[m.status] || TOOL_STATUS.ok;
      push([
        { t:'  ' },
        { t:'┌ ', color:C.line },
        { t:m.name, color:C.magenta, bold:true },
        { t:' · ', color:C.line },
        { t:m.arg, color:C.muted },
        { t:'  ' }, { t:st.g, color:st.c }, { t:' '+st.t, color:st.c },
      ]);
      (m.out||[]).forEach((ln, i, a) => push([
        { t:'  ' },
        { t: i===a.length-1 ? '└ ' : '│ ', color:C.line },
        { t: ln, color: m.status==='fail' ? C.fg : C.muted },
      ]));
    }
  });
  return rows;
}

function ChatScreen({ msgs, draft, busy, focused, model }){
  const WIDTH = COLS - 4;
  const VIEW = 17;
  const all = chatRows(msgs, WIDTH);
  if (busy) all.push([], [{ t:'  ' }, { t:'… ', color:C.orange }, { t:'thinking', color:C.muted }]);
  const shown = all.slice(-VIEW);
  while (shown.length < VIEW) shown.unshift([]);

  const draftSegs = [
    { t:'» ', color:C.orange, bold:true },
    ...(draft ? [{ t:draft, color:C.fg }] : []),
    { t:'█', color:C.orange, cls: focused?'tui-cursor':'' },
    ...(!draft ? [{ t:'  ask the agent to inspect or change a host…', color:C.faint }] : []),
  ];

  return (
    <div className="oa-chat">
      <TopBorder w={COLS} title="Agent" focus={true} right={`${model} · 3 hosts in context`} />
      {shown.map((r,i)=><BodyRow key={i} w={COLS} inner={r} focus={true} />)}
      <SepBorder w={COLS} focus={true} />
      <BodyRow w={COLS} inner={draftSegs} focus={true} />
      <BodyRow w={COLS} inner={[
        { t:'Enter send   Shift+Enter newline   @ add host to context   ^R run command', color:C.faint },
        { fill:true },
        { t:'web-01 db-main bastion', color:C.faint },
      ]} focus={true} />
      <BotBorder w={COLS} focus={true} />
    </div>
  );
}

Object.assign(window, { ChatScreen, chatRows, wrap });
