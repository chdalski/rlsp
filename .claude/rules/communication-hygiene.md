# Communication Hygiene

When agents share a team, every `SendMessage` is processed
by the recipient's full conversation context — each message
costs a turn, not a token. Bare acknowledgments, redundant
re-sends, and stale-message processing multiply that cost:
one advisor saying "got it" to a dispatch that needed no
reply forces the dispatcher to re-process its full context
on the next turn, and a chatty team can burn an order of
magnitude more tokens than a terse one for the same work.

Keep inter-agent messages load-bearing.

## The Three Rules

### 1. Messages must carry load

Send a message only when it carries one of:

- A **request** — asking another agent to do something
  (consult, review, provide a sign-off)
- A **result** — an artifact the requester is waiting for
  (test list, sign-off, review verdict, completed work)
- A **blocker** — something preventing your progress that
  the recipient can resolve

Do not send bare acknowledgments — "got it," "confirmed,"
"noted," "will do," "understood." Silence after receiving
a message is the default. The sender will see your next
load-bearing message and know you processed their input.

**Why:** each ack is a full turn for the sender on their
next wake. A four-advisor team that each acks every
dispatch adds four unnecessary turns per task cycle.

### 2. Sign-offs stand until invalidated

When you have issued a sign-off — an advisor approval, a
reviewer verdict — do not resend it if the requester asks
again. Respond once with a single line ("sign-off stands"),
then stay silent on the same thread.

**Why:** a production incident had the test-engineer
send progressively more elaborate versions of the same
sign-off in response to the developer's repeated
re-requests, each response consuming a full-context turn
across both agents. The underlying problem was stale
inbox state on the developer's side (rule 3), but the
advisor's re-confirmation behavior compounded the cost.

**How to apply:** if the requester asks to re-confirm a
sign-off you already gave, a single "sign-off from prior
message stands" is the full response. If they ask a third
time, something structural is wrong on their side — do
not send a third confirmation; wait for them to escalate
or send a substantive blocker-style message describing
what you think is going wrong.

### 3. Read the whole thread before acting on any message

When you return from idle and find multiple unread
messages from the same sender on the same topic, read
them all before acting on any one. Later messages on a
thread supersede earlier ones — an instruction from a
prior message may have been canceled by the one you
haven't read yet.

**Why:** a production incident had the developer act on
a stale test-engineer concern while a later "never mind,
sign-off stands" message sat unread directly below it.
The developer re-engaged the advisor and triggered a
rejection cycle on an issue that was already resolved.

**How to apply:** scan all unread messages from a given
sender before taking action based on any one of them. If
the newest message resolves a concern raised earlier,
the earlier messages are historical — do not re-trigger
work on them.

## Scope

This rule applies to all agents that use `SendMessage`:
the developer, the reviewer, and all advisors. It does
not govern communication with the user — the lead's
`AskUserQuestion` flow and user-facing status updates are
outside the inter-agent message stream.

## Related Rules

- `advisor-gate-independence.md` — one advisor's sign-off
  cannot substitute for another advisor's gate. A sign-off
  that stands per rule 2 above still only covers its own
  gate.
- `procedural-fidelity.md` — staying silent when silence
  is the default is not a skipped procedural step. The
  gate is the original sign-off; re-confirmations are
  noise, not additional gates.
