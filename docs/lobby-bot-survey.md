# Lobby, Bot Challenge, and Role Questionnaire

Three interlocking onboarding features. None are built yet; this is the
design we'd start from.

- **Feature 1 — Security Level Lobby**: a confined entry state for users
  below the hub's `min_security_level`. They connect, run PoW in the
  background, and get auto-promoted when they reach the threshold.
- **Feature 2 — "Not a bot" challenge**: a server-generated lightweight
  human-check (math/pattern) gating PoW start. Independent of PoW level.
- **Feature 3 — Role questionnaire**: an admin-defined survey that runs
  during onboarding. Multiple-choice answers auto-map to roles;
  free-text answers route the user to manual review.

The three stack as a single flow: challenge -> lobby (PoW + survey) ->
full member or pending review.

Authoritative code targets (not yet written):
- `server/voxply-hub/src/routes/lobby.rs`
- `server/voxply-hub/src/routes/challenge.rs`
- `server/voxply-hub/src/routes/survey.rs`
- `client/voxply-desktop/src/components/Lobby.tsx`
- `client/voxply-desktop/src/components/BotChallenge.tsx`
- `client/voxply-desktop/src/components/Survey.tsx`

Cross-references: anti-spam design space in
[future-features.md](future-features.md); PoW primitives at
`shared/voxply-identity/src/pow.rs`; approval flow today via
`users.approval_status`.

---

## Feature 1 — Security Level Lobby

### What it does

A user joining a hub whose `min_security_level` exceeds their current
PoW level lands in a **lobby state**: they are authenticated, can see a
welcome message and (if enabled) the role questionnaire, but cannot
read or post in regular channels, cannot join voice, and do not appear
in the member list. The client computes PoW in the background and
shows progress. When the user's level reaches the threshold, the hub
promotes them automatically and the regular UI appears.

### DB additions

- `hub_settings` rows (no new tables):
  - `lobby_enabled` (`'0'` / `'1'`, default `'1'` — only used when
    `min_security_level > 0`)
  - `lobby_welcome_md` (markdown text, nullable)
- `users` columns (additive, default-safe):
  - `lobby_status TEXT NOT NULL DEFAULT 'none'` — `'none' | 'lobby' |
    'promoted'`. `'none'` covers all legacy rows; `'lobby'` users are
    excluded from the regular member list; `'promoted'` is a terminal
    transition state that ages to `'none'` on next login (purely so
    the client can show a one-shot "welcome to the hub" toast).
  - `lobby_entered_at INTEGER` (unix seconds, nullable) — used for
    progress estimation and lobby-timeout cleanup.
  - `pow_level INTEGER NOT NULL DEFAULT 0` — current verified level.
    Verified at auth time from the proof in the identity payload.

No new tables. The lobby is a state of an existing user row.

### Routes

- `GET /lobby/status` — auth required (lobby or full).
  Response: `{ status: 'lobby' | 'promoted' | 'member', required_level,
  current_level, entered_at, welcome_md?: string }`. Polled by the
  lobby client; promotion is detected here.
- `POST /lobby/submit-pow` — auth required.
  Body: `{ pow_proof }`. Hub re-verifies, updates `users.pow_level`,
  and if `pow_level >= min_security_level` flips `lobby_status` to
  `'promoted'` and returns `{ promoted: true, new_level }`. Otherwise
  returns `{ promoted: false, new_level }` (partial progress is
  allowed — see "key decisions").
- `GET /lobby/welcome` — auth required, lobby-only.
  Response: `{ welcome_md, hub_name, required_level }`. Cached
  client-side for the duration of the lobby session.
- `PUT /hub/settings/lobby` — admin only.
  Body: `{ lobby_enabled, welcome_md? }`. Writes the two settings rows.

The existing auth handshake (`/auth/challenge`, `/auth/verify`)
classifies the user: if `pow_level < min_security_level` and
`lobby_enabled = '1'`, the session token is issued but tagged
`scope: 'lobby'`; otherwise `scope: 'member'`. The token scope is
checked by middleware on every non-lobby route.

### Tauri commands

- `lobby_status(hub_url)` -> `LobbyStatus { status, required_level,
  current_level, welcome_md?, eta_seconds? }`. Polls
  `GET /lobby/status` and computes ETA from the local PoW progress.
- `lobby_start_pow(hub_url, target_level)` -> `()`. Spawns the
  background PoW worker for that hub; idempotent if already running.
- `lobby_stop_pow(hub_url)` -> `()`. Cancels the worker (e.g. user
  navigates away from the lobby hub).
- `lobby_submit_proof(hub_url)` -> `LobbyStatus`. Pulls the latest
  cached proof from the local worker, POSTs `/lobby/submit-pow`,
  returns the resulting status.

PoW progress events are emitted via the existing Tauri event channel
(`lobby:progress` with `{ hub_url, attempts, level }`).

### UI sketch

- **Lobby view** replaces the normal channel sidebar for any hub where
  `lobby_status === 'lobby'`. Layout:
  - Hub name + "Lobby" badge.
  - Welcome markdown block (read-only).
  - Progress card: "Verifying you're human... level 3 / 5 required",
    progress bar, ETA in plain words ("about 4 minutes"), Pause /
    Resume buttons.
  - Optional embedded **Survey** form (Feature 3) — visible only if
    the hub has a survey configured.
  - Footer: "You'll be let in automatically when verification finishes."
- **Hub list (sidebar)**: lobby hubs render with a small clock icon and
  a tooltip ("Verifying...").
- **Transition**: on `status === 'promoted'`, the lobby view fades out,
  the regular channel sidebar mounts, and a non-blocking toast says
  "You're in. Welcome to <hub>."
- **Failure / timeout**: if the PoW computation is paused and the
  lobby session sits idle past `lobby_idle_timeout` (24h, hub setting,
  not user-facing in v1), the next status poll returns `expired` and
  the client surfaces "Click to resume verification." No data is lost
  — partial proofs persist in the local PoW state file.

### Key decisions and tradeoffs

- **Lobby is a user state, not a separate "lobby room" entity.** A
  user row exists; it's just scoped. Alternative was a parallel
  `lobby_users` table — rejected because it duplicates identity
  bookkeeping and complicates the eventual promotion (would need
  row-move). Single table + scope flag is the same shape we use for
  `approval_status`.
- **PoW progress is reported partially.** Each successful intermediate
  level (e.g. solved level 3 while heading to 5) is submitted and
  stored. Rationale: a 15-minute PoW that the user pauses halfway must
  not lose work. Alternative was "submit only the final proof";
  rejected because partial submissions are cheap to verify and dramatic
  UX improvement.
- **Lobby users count for nothing.** Not in member list, not in
  permission grants, not in any role. They're tokens with a
  `lobby` scope. This keeps the "what does this user see" check
  trivially safe by default — anything not explicitly allowed for
  `lobby` scope is denied. Alternative was a hidden `@lobby` role;
  rejected as it muddies the role model for one transient case.
- **Welcome message is hub markdown, not a channel.** Reusing a real
  channel for the welcome would require permission overrides and
  inventing a "channel visible only to lobby" concept. A standalone
  markdown blob in `hub_settings` is one column.
- **Auto-promotion is server-decided on submit.** The client cannot
  self-promote; the server re-verifies and flips the bit. The lobby
  view polls status (or receives a WS notification once the lobby WS
  scope is wired) and re-mounts when promoted.

### Deferred

- WS push for promotion (today: poll every 5s). Trivial to add when
  the WS handshake supports the `lobby` scope.
- Multiple parallel lobby hubs sharing one PoW worker pool with
  priority scheduling. v1 runs one worker per lobby hub.
- Lobby chat (letting lobby users talk to each other or to admins).
  Out of scope; if users need help during PoW, DMs to admins work.
- "Skip PoW with a hub-issued invite code" — out of scope; the
  invite-only flow is a separate gate that already bypasses PoW
  considerations.

---

## Feature 2 — "Not a bot" Challenge

### What it does

Before a hub starts issuing PoW work to a fresh client, it can require
the client to solve a small interactive puzzle (e.g. "what number do
you see?" with an obfuscated rendering, or "tap the third square
from the left"). Server generates the puzzle, client solves it, server
verifies the answer and issues a short-lived **challenge token**
that the client must present when starting PoW (or, equivalently, when
hitting `/auth/verify` at lobby entry).

The challenge is **additive to PoW**, not a replacement. It exists to
make automated mass-onboarding annoying — humans solve it in seconds,
bots have to either OCR/parse a server-rendered SVG or run a real
browser per identity.

### DB additions

- `hub_settings` rows:
  - `challenge_mode` (`'off' | 'click' | 'puzzle' | 'both'`, default
    `'off'`) — replaces the old boolean `challenge_enabled`.
    - `'off'`: no challenge, straight to auth.
    - `'click'`: one button press issues the token; stops HTTP-only
      bots with no user friction.
    - `'puzzle'`: server-generated SVG challenge (math or pattern);
      answer verified before token is issued.
    - `'both'`: click first, then puzzle. Maximum friction for bots,
      still fast for humans (~5 seconds total).
  - `challenge_difficulty` (`'easy' | 'medium'`, default `'easy'`) —
    applies only when `challenge_mode` includes `'puzzle'`.
- New table `bot_challenges` — used only for `puzzle` / `both` modes:
  - `id TEXT PRIMARY KEY` — challenge id (UUID).
  - `pubkey TEXT NOT NULL` — who requested it (Ed25519 pubkey hex).
  - `kind TEXT NOT NULL` — `'click' | 'puzzle'`.
  - `expected_answer TEXT` — server-stored, never sent to the client.
    NULL for `click` rows (no answer to check).
  - `created_at INTEGER NOT NULL` — unix seconds.
  - `expires_at INTEGER NOT NULL` — `created_at + 300`.
  - `consumed_at INTEGER` — set when token is issued (one-shot).
  - Index on `(pubkey, expires_at)`.
- New table `challenge_tokens`:
  - `token TEXT PRIMARY KEY` — random 32-byte hex.
  - `pubkey TEXT NOT NULL`.
  - `issued_at INTEGER NOT NULL`.
  - `expires_at INTEGER NOT NULL` — `issued_at + 600` (10 min).
  - `consumed_at INTEGER`.

### Routes

- `GET /challenge/new` — unauthenticated.
  Query: `?pubkey=<hex>`. Returns `{ id, mode, prompt_svg?, expires_at
  }`. For `click` and the click step of `both`, `prompt_svg` is
  absent — the client just shows a button. For `puzzle` / `both`
  (puzzle step), `prompt_svg` is an inline SVG. Rate-limited per IP
  and per pubkey.
- `POST /challenge/verify` — unauthenticated.
  Body: `{ id, pubkey, answer? }`.
  - For `click` rows: `answer` is omitted; mere receipt of the signed
    request (pubkey matches) is enough to issue the token.
  - For `puzzle` rows: `answer` is required and verified against
    `expected_answer`. Three failures invalidate the challenge.
  - On success: marks the challenge consumed, issues a
    `challenge_tokens` row, returns `{ token, expires_at }`.
  - On failure: returns `{ ok: false, attempts_remaining }`.
- Existing `/auth/verify` gains an optional `challenge_token` field.
  When `challenge_mode != 'off'`, the field is required; the hub
  validates and consumes the token. For `'both'` mode, a single token
  covers the whole flow — the click step issues it, the puzzle step
  re-uses the same `id` (or a chained second step, see Deferred).

The `min_security_level` and `challenge_mode` settings are
independent. A hub at level 0 can still use `click` or `puzzle`.

### Tauri commands

- `challenge_fetch(hub_url, pubkey)` -> `ChallengePrompt { id, mode,
  prompt_svg?, expires_at }`. Calls `GET /challenge/new`.
- `challenge_submit(hub_url, id, pubkey, answer?)` ->
  `ChallengeResult { ok, token?, expires_at?, attempts_remaining? }`.
  `answer` is optional; omitted for `click` mode.
- The existing `add_hub` command grows an optional `challenge_token`
  argument. When `/info` reports `challenge_mode != 'off'`, the client
  runs the challenge flow first and passes the resulting token.

### UI sketch

- **Add Hub modal** — new "Quick check" step inserted before PoW:
  - `'click'` mode: large centred "I'm not a bot" button with the
    hub name above it. One tap → passed → moves on.
  - `'puzzle'` mode: prompt SVG inline, text input below, "Submit"
    button, "New puzzle" link. States: `loading`, `awaiting-answer`,
    `submitting`, `wrong` (attempts remaining shown), `passed`.
  - `'both'` mode: click step first (same as above), then
    automatically advances to the puzzle step.
  - `'off'` mode: step is silently skipped.
- **Passed state**: 1-second green checkmark, then the modal advances.
- **Failure / expiry**: modal returns to URL-entry with inline error
  "Couldn't verify — try again."

### Key decisions and tradeoffs

- **`challenge_mode` enum, not a boolean.** Admin chooses the right
  level of friction for their community: `click` for open friendly
  hubs ("just filter headless scripts"), `puzzle` for communities
  that expect scripted attacks, `both` for maximum deterrence.
  A boolean `enabled` would force every hub onto one model.
- **Click mode issues a token on bare receipt.** There is no secret
  to verify for a click — the value is purely that a browser had to
  render and present a button. The Ed25519 pubkey in the request body
  is enough to bind the token to the right identity.
- **Server-side rendering, no third-party service.** Keeps federation
  sovereignty: no hub depends on an external captcha provider to gate
  joins. Weaker than commercial alternatives, but the right tradeoff
  for a decentralised platform.
- **Token-based, single-use, short-lived.** Bound to pubkey, one row
  per challenge, periodic cleanup sweep. Cheaper than a pre-session
  cookie model.
- **Prompt kinds are pluggable.** v1 ships `math` and `pattern`.
  A third kind is one generator + SVG template.

### Deferred

- `'both'` mode chaining: v1 uses a single token (click issues it,
  puzzle is a second verification pass on the same row before the
  token is handed to the client). A cleaner two-token chain can be
  designed once usage patterns are known.
- Accessibility (audio challenge). `prompt_kind` is extensible; gap
  is documented.
- Adaptive difficulty per IP.
- Cross-hub challenge token acceptance (federation extension).

---

## Feature 3 — Role Questionnaire / Onboarding Survey

### What it does

A hub admin defines an ordered list of questions. Each question is
multiple-choice (with optional role mappings per answer) or free-text.
On joining the hub (in the lobby if Feature 1 is active, otherwise
immediately after auth), the user answers the survey. Multiple-choice
answers auto-apply the mapped roles. If any question is free-text or
flagged `requires_review`, the user's `approval_status` becomes
`'pending'` and the answers appear in the admin's pending-members
panel. Otherwise the user is promoted directly to full member with
their mapped roles applied.

### DB additions

New tables:
- `surveys`:
  - `id TEXT PRIMARY KEY` (UUID).
  - `hub_id TEXT NOT NULL` (always the local hub; a column for
    forward-compatibility with farm model).
  - `enabled INTEGER NOT NULL DEFAULT 0`.
  - `updated_at INTEGER NOT NULL`.
  - At most one row enabled at a time. (Unique partial index.)
- `survey_questions`:
  - `id TEXT PRIMARY KEY`.
  - `survey_id TEXT NOT NULL REFERENCES surveys(id) ON DELETE CASCADE`.
  - `prompt TEXT NOT NULL`.
  - `kind TEXT NOT NULL` — `'choice' | 'text'`.
  - `required INTEGER NOT NULL DEFAULT 1`.
  - `display_order INTEGER NOT NULL`.
- `survey_choices`:
  - `id TEXT PRIMARY KEY`.
  - `question_id TEXT NOT NULL REFERENCES survey_questions(id) ON
    DELETE CASCADE`.
  - `label TEXT NOT NULL`.
  - `display_order INTEGER NOT NULL`.
- `survey_choice_roles`:
  - `choice_id TEXT NOT NULL REFERENCES survey_choices(id) ON DELETE
    CASCADE`.
  - `role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE`.
  - `PRIMARY KEY (choice_id, role_id)`.
- `survey_responses`:
  - `id TEXT PRIMARY KEY`.
  - `pubkey TEXT NOT NULL`.
  - `survey_id TEXT NOT NULL`.
  - `submitted_at INTEGER NOT NULL`.
  - One per (pubkey, survey_id).
- `survey_answers`:
  - `response_id TEXT NOT NULL REFERENCES survey_responses(id) ON
    DELETE CASCADE`.
  - `question_id TEXT NOT NULL`.
  - `choice_id TEXT` — set for choice questions.
  - `text_answer TEXT` — set for free-text questions.
  - `PRIMARY KEY (response_id, question_id)`.

### Routes

- `GET /survey/current` — auth required (lobby or member).
  Response: `{ id, questions: [{ id, prompt, kind, required,
  choices?: [{ id, label }] }] }` or `null` if none enabled. Choice
  -> role mappings are not sent to the client.
- `POST /survey/submit` — auth required, scope `lobby` or `member` if
  not previously submitted.
  Body: `{ survey_id, answers: [{ question_id, choice_id? | text? }] }`.
  Server validates against the active survey, writes
  `survey_responses` + `survey_answers`, applies mapped roles for any
  choice answers, and decides next state:
  - If any answer is free-text OR any answered choice has a flag
    `requires_review` (deferred for v1 — see decisions): set
    `approval_status='pending'`, keep `lobby_status` (if applicable).
  - Else: clear `approval_status` to `'approved'` and (if lobby and
    PoW already satisfied) flip `lobby_status` to `'promoted'`.
  Returns `{ next_state: 'approved' | 'pending' | 'lobby',
  applied_roles: [...] }`.
- `GET /admin/survey` — admin only. Returns the full survey
  definition including role mappings.
- `PUT /admin/survey` — admin only. Body is the full survey shape
  (questions + choices + role mappings + `enabled`). Replace-all
  semantics for simplicity; the survey is small and rarely edited.
- `GET /admin/survey/responses?status=pending|all&limit=&cursor=` —
  admin only. Joined view of `survey_responses` + `users` for the
  pending-members panel.
- `GET /admin/survey/responses/:pubkey` — admin only. Full answer
  set for one user.

The existing pending-members admin panel reads from this same data
when it shows a user — answers appear inline next to the
approve/reject buttons.

### Tauri commands

- `survey_current(hub_url)` -> `Survey | null`.
- `survey_submit(hub_url, answers)` -> `SurveySubmitResult { next_state,
  applied_roles }`.
- `survey_admin_get(hub_url)` -> `SurveyDef` (full shape with role
  mappings).
- `survey_admin_put(hub_url, survey_def)` -> `()`.
- `survey_admin_responses(hub_url, filter)` ->
  `SurveyResponseList`.

### UI sketch

- **Lobby view** (Feature 1) embeds the survey form below the welcome
  block when one is enabled and not yet submitted. Multi-choice
  rendered as radio groups; free-text as a textarea with a per-field
  character limit (e.g. 500). Submit button is disabled until all
  `required` questions have an answer.
- **Open hub (no PoW)**: the survey runs as a modal immediately after
  joining. Closing the modal aborts the join (the user is logged out
  on this hub until they submit). Rationale: the survey is the
  approval gate; you can't bypass it by closing it.
- **Pending state**: after submit, if `next_state === 'pending'`, the
  view becomes a static "Your answers are with the admins. You'll get
  in when they approve." card. The user can still leave the hub. If
  approved later, a notification fires and the regular UI mounts on
  next focus.
- **Admin — Hub Settings > Onboarding survey** tab:
  - Toggle "Enable survey."
  - Ordered list of questions; per-question editor with kind selector,
    prompt, required toggle, and (for choice) an ordered list of
    answers each with multi-select of roles to apply.
  - "Save" performs the full `PUT /admin/survey`.
- **Admin — Pending members panel** gains a per-row expand showing
  the survey answers verbatim, with each role that would auto-apply
  highlighted.

### Key decisions and tradeoffs

- **Role mappings live on choices, not questions.** A question like
  "what platform?" with answers "PC / Console / Mobile" can map each
  to a different role (or none). Putting the mapping on the question
  would force one role per question. Many-to-many via
  `survey_choice_roles` keeps it flexible.
- **Free-text always implies manual review.** Even if an admin wants
  free-text "just for vibes," the only sane default is review — the
  hub cannot mechanically decide if a free-text answer earns roles.
  Admins who want auto-only should use only choice questions; the UI
  warns when a survey contains free-text ("this survey routes joiners
  to manual review").
- **No conditional questions in v1** ("if you answered X, then ask Y").
  Decision tree complexity not justified by current need. The schema
  is forward-compatible: a `depends_on_choice_id` column on
  `survey_questions` later, plus a client-side traversal.
- **One active survey per hub.** Multiple drafts would invite admin
  confusion ("which one are users seeing right now?"). Versioning the
  survey for historical responses is achieved by `survey_responses`
  carrying the `survey_id` at submission time; editing creates a new
  survey row (admin chooses to enable it).
- **Responses survive the user.** If a user leaves and rejoins later,
  their old response is still there and admins can see it. Hard-
  deleting a user (compliance) cascades through `survey_responses`.
- **No federation.** The survey is purely a local-hub join gate;
  there's no cross-hub or alliance equivalent. Personal-axis state
  this is not — it's community-axis state belonging to the community
  hub, consistent with the two-axis decision in
  [decisions.md](decisions.md).

### Deferred

- `requires_review` flag per choice (allowing certain answers to a
  choice question to still route to manual review, e.g. "I'm a
  vendor"). Schema-compatible; just a column add. v1 either auto-roles
  every choice or routes the whole user via the free-text rule.
- Conditional / branching questions.
- Localized prompts (multiple languages per question).
- Survey analytics for admins ("70% of joiners picked PC").
- Editing a submitted response. v1: one-shot.
- Cross-hub survey templates / sharing via the directory.

---

## How the three features compose

For a new user joining a hub where all three are enabled:

1. Client fetches `/info`, sees `challenge_enabled`,
   `min_security_level > 0`, and `survey enabled` (via
   `/survey/current` after auth).
2. Client calls `GET /challenge/new`, user solves it, client POSTs
   `/challenge/verify` -> `challenge_token`.
3. Client does Ed25519 handshake at `/auth/verify` with the token; the
   hub issues a `lobby`-scoped session because `pow_level <
   min_security_level`.
4. Lobby view mounts. PoW worker starts. Survey is rendered below the
   welcome message.
5. User submits survey -> server applies any auto-roles, decides
   whether to mark `approval_status='pending'`.
6. PoW completes -> client POSTs `/lobby/submit-pow` -> server flips
   `lobby_status='promoted'` (only if approval is not pending).
7. Status poll returns `member` -> regular UI mounts.
8. If approval is pending: lobby view becomes "waiting for admin"
   until the admin approves; promotion happens then.

A hub running only the lobby (no challenge, no survey): straight PoW
in the lobby, auto-promote on completion. A hub running only the
survey: modal at join time, auto-roles applied. A hub running only the
challenge: pre-auth puzzle, then straight in. All combinations are
valid by design — the features share storage primitives but no
control flow assumes the others are present.
