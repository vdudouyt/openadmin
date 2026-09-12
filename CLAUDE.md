# Working on OpenAdmin

Notes for whoever picks this up next. The README says what the program does;
this says what it is easy to get wrong.

## A tool must state its whole contract in its schema

The model pays for anything it has to discover by being refused. Not in
tokens — in **wall-clock time the operator watches**: a refusal is a round
trip, and before that it is a round of reasoning spent guessing at what might
be allowed. A boundary the model can read is a boundary it does not have to
infer.

So a tool schema here names the entire contract, not a summary of it.
`run_readonly` is the worked example. It used to describe its whitelist in
prose — "systemctl is limited to status/show/cat/is-\*/list-\*; find has no
-exec or -delete" — which told the model a boundary existed without telling it
where. Now `readonly::describe` renders the policy table in full: every
command, every option, the required first word, the read-only verb that may
follow it, whether options may be bundled, and the substrings an argument may
not contain.

Three things keep that honest, and all three are load-bearing:

1. **Generated, never written.** `describe` reads the same table `validate`
   enforces. A description that contradicts the rule is not something anyone
   can accidentally write, and a rule added to the table cannot ship
   undescribed — a test fails if a permitted command is missing from it.
2. **It lives in the schema, beside the arguments.** That is where it is read
   at the moment it is needed. The system prompt points at it and says the
   lists are closed; it does not repeat them. A second verbatim copy adds
   nothing to read at call time.

   Beside is not close enough when the contract is a closed set. A set of
   permitted values belongs in the `enum` of the field it constrains, not only
   in the description above it — `kind` on a plan step, `type` on a host, the
   `what` of every probe. Weak local models decode a field steered by that
   field's own schema and attend to a long description weakly, so a list eight
   hundred tokens upstream loses to the model's prior about what the field
   looks like. An enum also stops being advice: where the backend compiles the
   schema into a decoding grammar, which llama.cpp, vLLM and Ollama all do, the
   value cannot be emitted. That is the gate-is-a-type argument below, applied
   to a field instead of a constructor.

   And a field whose type is "string" or "array of string" states no contract at
   all. `run_readonly` used to take `command` plus `args: [string]`; naming the
   permitted commands in `command`'s enum fixed half of it and moved the problem
   into `args`, because an argv is a token stream and a token stream is a thing
   models put pipes in. The fix was to delete the token stream: `src/agent/probe.rs`
   is one tool per question, every option a named typed field, and the argv
   assembled from those fields here. `|` is then not refused, it is
   unrepresentable — there is no field it fits in. Prefer that shape for anything
   new. If a tool needs a free-form string or a list of them, ask what the model
   could put in it that you would have to refuse, because it will.

   Two layers, not one: `probe` decides the shape of a call and
   `readonly::validate` still judges the argv that results. The second is the
   boundary, unchanged from when the model wrote the argv itself, so the first
   can only ever be narrower — and
   `every_probe_renders_what_validate_accepts` fails rather than an operator
   discovering the gap.
3. **A refusal that does happen must end the thinking, not start it.** Name
   the fix, not just the rule. `journalctl -rn 50` used to be refused with a
   list containing both `-r` and `-n`, leaving the model to deduce that
   bundling was the problem; it now says to pass them separately — and only
   when splitting would actually work, because a hint that sends the model
   down a dead end costs another trip.

4. **Say what makes the boundary unnecessary, not only where it is.** Every
   refusal seen in practice was the model working around a guarantee nobody had
   told it about: `| head -n 100` bounding output that `run_capture` already
   caps, `2>&1` merging a stream it is already shown, `|| fallback` hedging
   against a failure it would have been handed. The boundary was documented
   exhaustively and the reasons to cross it were not documented at all, so the
   model kept finding them. Naming the guarantee removes the motive; refusing
   the syntax only removes the option, and it costs a round trip each time.

Apply the same test to any tool added later: *could the model have known this
would be refused before it called?* If not, the schema is incomplete. And the
second test, for a refusal seen in the wild: *what was the model trying to
achieve, and does anything tell it that it already had it?*

This is about latency, and it is worth real prefix tokens — those are cached
and prefill fast, while a round trip is seconds the operator spends waiting.

## What must not become inferrable

The counterweight: the model is told everything about **what it may do**, and
as little as possible about **the fleet**. `list_hosts` returns names only —
no address, port, login or mount point — because a name is the handle every
tool takes and OpenAdmin fills in the rest itself. Do not add a field there to
save the model a question; it does not need one, and the transcript is an
inventory of somebody's infrastructure sitting on somebody else's server.

## The gate is a type, not an instruction

`Plan` is what the model builds; `ConfirmedPlan` is what the executor takes,
and its constructor is private to `app::approve`. Keep it that way: no
channel, handle or callback that reaches the executor may become visible to
`crate::agent`. The system prompt describes this as a fact about the program
rather than a rule, so the model does not waste turns looking for a way
around — but the prompt is not what makes it true.

Whitelists, never blacklists, for the same reason: a blacklist is only as good
as the last time someone read the man page, and next year's release adds an
option that is permitted by default.
