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
3. **A refusal that does happen must end the thinking, not start it.** Name
   the fix, not just the rule. `journalctl -rn 50` used to be refused with a
   list containing both `-r` and `-n`, leaving the model to deduce that
   bundling was the problem; it now says to pass them separately — and only
   when splitting would actually work, because a hint that sends the model
   down a dead end costs another trip.

Apply the same test to any tool added later: *could the model have known this
would be refused before it called?* If not, the schema is incomplete.

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
