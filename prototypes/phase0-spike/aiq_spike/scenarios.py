"""Synthetic task scenarios with ground-truth labels (Phase 0 spike).

Each scenario is ONE task: one unit of intent, with its job, the rung it was done at, and the
next reachable rung (the rung model's "potential rung"). The steps are written by hand to look
like real Claude Code / Copilot CLI work in a fictional repo ("acme-shop": a Python web shop
with an api/ package, docs/ and specs/). No real person's data is used.

Labelling rules (from the design doc's Rung model tab):
  1 Assisted     short ad hoc prompts; the person does the work, AI fills in pieces
  2 Directed     structured prompt (goal, context, constraints); output checked before use
  3 Delegated    a whole task handed to an agent; the person reviews the result
  4 Orchestrated several agents/steps coordinated with a written spec, guardrails, automated checks
  5 Factory      a reusable, versioned workflow that others can trigger, run with little change
  A task sits at the highest rung whose signals are clearly met; mixed signals -> the lower rung.
  Potential = the NEXT reachable rung (one step), or the same rung when the task already fits.

Jobs: "code_change" (Code a change), "write_doc" (Write a spec or doc), "other" (any job with no
alpha pack; the product must report it as Unclassified).

`ambiguous` marks scenarios where a careful human labeller could reasonably disagree; the
evaluation reports them separately rather than hiding them in the average.
"""

# --- step constructors -------------------------------------------------------------------
def U(text):
    """A human prompt. Starts a new turn."""
    return ("user", text)


def CMD(name, args=""):
    """A slash command / skill invocation typed by the person (starts a turn)."""
    return ("command", name, args)


def A(text):
    """Assistant text."""
    return ("assistant", text)


def T(tool, args, result="ok"):
    """A tool call by the agent. Tool names use Claude Code's names; the Copilot renderer maps them."""
    return ("tool", tool, args, result)


def SUB(name, prompt, tools=()):
    """A sub-agent run: name, its prompt, and the tool calls it made."""
    return ("subagent", name, prompt, list(tools))


def GAP(minutes):
    """Idle time inside the task (the person is away or working outside the agent)."""
    return ("gap", minutes)


SPLIT = ("split",)  # the task may be resumed here in a later session or on another surface


S = []  # the library


def scenario(sid, job, rung, potential, steps, ambiguous=None, note=""):
    S.append({"id": sid, "job": job, "rung": rung, "potential": potential, "steps": steps,
              "ambiguous": ambiguous, "note": note})


# =========================================================================== Code a change
scenario("c01", "code_change", 1, 2, [
    U("whats wrong with this\n\n    if user.email = None:\n        raise ValueError"),
    A("You used `=` (assignment) instead of `is` for the comparison. Use `if user.email is None:`."),
    U("and how do i make it also reject empty string"),
    A("Use `if not user.email:` which covers both None and an empty string."),
], note="ad hoc snippet help; person edits the code themselves")

scenario("c02", "code_change", 1, 2, [
    U("fix the checkout test"),
    T("Bash", {"command": "pytest tests/test_checkout.py -q"}, "1 failed: test_total_with_coupon AssertionError 90 != 81"),
    A("The coupon is applied after tax. I changed the order in `api/checkout.py`."),
    T("Edit", {"file_path": "api/checkout.py", "old_string": "total = tax(sub) - coupon", "new_string": "total = tax(sub - coupon)"}),
    U("still failing"),
    T("Bash", {"command": "pytest tests/test_checkout.py -q"}, "1 failed: test_total_with_coupon AssertionError 91.8 != 81"),
    A("Now the tax is applied to the discounted subtotal. The test expects 81, which looks like no tax at all."),
    U("still failing, fix it"),
    A("Could you tell me whether coupons should be applied before or after tax?"),
    U("just make it pass"),
    T("Edit", {"file_path": "tests/test_checkout.py", "old_string": "== 81", "new_string": "== 91.8"}),
], note="re-prompting without adding context; output used unchecked")

scenario("c03", "code_change", 2, 3, [
    U("Goal: add server-side validation to POST /signup in api/routes/signup.py.\n"
      "Context: we use pydantic v2 models in api/models.py; the frontend already checks format.\n"
      "Constraints: email must be RFC 5322-ish, password >= 12 chars, return 422 with field errors, "
      "don't touch the frontend. Show me the diff before writing."),
    T("Read", {"file_path": "api/routes/signup.py"}, "def signup(req): ..."),
    T("Read", {"file_path": "api/models.py"}, "class SignupIn(BaseModel): email: str; password: str"),
    A("Proposed diff: add `EmailStr` and a `Field(min_length=12)` to `SignupIn`, and return 422 from the "
      "route's validation handler. Shall I apply it?"),
    U("looks right but use our existing error envelope from api/errors.py, then apply"),
    T("Read", {"file_path": "api/errors.py"}, "def error_envelope(code, fields): ..."),
    T("Edit", {"file_path": "api/models.py", "old_string": "email: str", "new_string": "email: EmailStr"}),
    T("Edit", {"file_path": "api/routes/signup.py", "old_string": "def signup(req):", "new_string": "def signup(req: SignupIn):"}),
    A("Applied. I have not run the tests."),
    U("ok I ran them locally, all green. thanks"),
], note="structured prompt, output checked, but the person drives verification")

scenario("c04", "code_change", 2, 3, [
    U("open api/cart.py and show me the add_item function"),
    T("Read", {"file_path": "api/cart.py"}, "def add_item(cart, sku, qty): ..."),
    A("Here is `add_item`: it appends a line without checking stock."),
    U("add a stock check using inventory.available(sku) before appending"),
    T("Edit", {"file_path": "api/cart.py", "old_string": "cart.lines.append", "new_string": "if inventory.available(sku) < qty: raise OutOfStock(sku)\n    cart.lines.append"}),
    U("now import OutOfStock from api.errors"),
    T("Edit", {"file_path": "api/cart.py", "old_string": "import inventory", "new_string": "import inventory\nfrom api.errors import OutOfStock"}),
    U("now add a test for it in tests/test_cart.py"),
    T("Edit", {"file_path": "tests/test_cart.py", "old_string": "# end", "new_string": "def test_out_of_stock(): ...\n# end"}),
    U("run that test"),
    T("Bash", {"command": "pytest tests/test_cart.py::test_out_of_stock -q"}, "1 passed"),
], note="person drives the agent step by step (rubric: not Delegated)")

scenario("c05", "code_change", 3, 4, [
    U("Implement rate limiting on POST /login (issue #42). Acceptance criteria: max 5 attempts per IP per "
      "minute, 429 with Retry-After, limits stored in redis via api/cache.py, unit tests for the limiter "
      "and an integration test for the route. Follow CLAUDE.md conventions. Run the full test suite and "
      "ruff before you hand back, and summarise what you changed."),
    T("Read", {"file_path": "CLAUDE.md"}, "conventions..."),
    T("Grep", {"pattern": "def login", "path": "api"}, "api/routes/auth.py:14"),
    T("Read", {"file_path": "api/routes/auth.py"}, "def login(...): ..."),
    T("Read", {"file_path": "api/cache.py"}, "def get_redis(): ..."),
    T("Write", {"file_path": "api/ratelimit.py", "content": "class SlidingWindowLimiter: ..."}),
    T("Edit", {"file_path": "api/routes/auth.py", "old_string": "def login(", "new_string": "@limit(5, per=60)\ndef login("}),
    T("Write", {"file_path": "tests/test_ratelimit.py", "content": "def test_sixth_attempt_blocked(): ..."}),
    T("Bash", {"command": "pytest -q"}, "2 failed: redis connection refused"),
    SPLIT,
    U("continue the login rate limiting from before; tests were failing on redis. use the fakeredis fixture in tests/conftest.py"),
    T("Read", {"file_path": "tests/conftest.py"}, "@pytest.fixture def fake_redis(): ..."),
    T("Edit", {"file_path": "tests/test_ratelimit.py", "old_string": "def test_sixth", "new_string": "def test_sixth(fake_redis"}),
    T("Bash", {"command": "pytest -q && ruff check ."}, "212 passed; All checks passed!"),
    A("Done. Added `api/ratelimit.py` (sliding window in redis), decorated `login`, 4 unit tests and 1 "
      "integration test. Full suite and ruff pass."),
    U("reviewed the diff, one nit: make the limit configurable via settings.RATE_LIMIT_LOGIN"),
    T("Edit", {"file_path": "api/routes/auth.py", "old_string": "@limit(5, per=60)", "new_string": "@limit(settings.RATE_LIMIT_LOGIN, per=60)"}),
    T("Bash", {"command": "pytest -q"}, "212 passed"),
], note="whole task handed over with acceptance criteria; person reviews")

scenario("c06", "code_change", 3, 3, [
    U("Bug: order confirmation emails show the price in cents (e.g. '4599' instead of '45.99'). Repro: "
      "place any order on staging. Find the cause, fix it, add a regression test, run the email tests. "
      "Don't change the template wording."),
    T("Grep", {"pattern": "confirmation", "path": "api/email"}, "api/email/confirm.py:22"),
    T("Read", {"file_path": "api/email/confirm.py"}, "price=order.total_cents"),
    T("Edit", {"file_path": "api/email/confirm.py", "old_string": "price=order.total_cents", "new_string": "price=format_money(order.total_cents)"}),
    T("Write", {"file_path": "tests/email/test_confirm_price.py", "content": "def test_price_formatted(): ..."}),
    T("Bash", {"command": "pytest tests/email -q"}, "9 passed"),
    A("The template received raw cents. I now pass `format_money(...)`; regression test added; email tests pass."),
    U("checked the rendered email in the test output, good"),
], note="one-off bug delegated end to end; already at its right rung")

scenario("c07", "code_change", 4, 4, [
    U("Execute the migration plan in specs/payments-v2-migration.md. Use one sub-agent per service module "
      "(orders, refunds, payouts) working from the spec's checklist; each must keep its module's tests "
      "green. Then run a reviewer sub-agent against the spec's acceptance criteria. Gate: full pytest, "
      "mypy --strict on api/payments, and the contract tests in tests/contract. Stop and ask if a gate fails twice."),
    T("Read", {"file_path": "specs/payments-v2-migration.md"}, "## Checklist ... ## Acceptance criteria ..."),
    SUB("orders-migrator", "Migrate api/payments/orders.py to the v2 client per checklist items 1-4.",
        [T("Edit", {"file_path": "api/payments/orders.py", "old_string": "v1.charge", "new_string": "v2.payments.create"}),
         T("Bash", {"command": "pytest tests/payments/test_orders.py -q"}, "31 passed")]),
    SUB("refunds-migrator", "Migrate api/payments/refunds.py per checklist items 5-7.",
        [T("Edit", {"file_path": "api/payments/refunds.py", "old_string": "v1.refund", "new_string": "v2.refunds.create"}),
         T("Bash", {"command": "pytest tests/payments/test_refunds.py -q"}, "18 passed")]),
    SUB("payouts-migrator", "Migrate api/payments/payouts.py per checklist items 8-9.",
        [T("Edit", {"file_path": "api/payments/payouts.py", "old_string": "v1.transfer", "new_string": "v2.payouts.create"}),
         T("Bash", {"command": "pytest tests/payments/test_payouts.py -q"}, "12 passed")]),
    SUB("spec-reviewer", "Review the three diffs against the acceptance criteria in specs/payments-v2-migration.md. Report gaps only.",
        [T("Read", {"file_path": "specs/payments-v2-migration.md"}, "...")]),
    T("Bash", {"command": "pytest -q && mypy --strict api/payments && pytest tests/contract -q"}, "240 passed; Success: no issues; 14 passed"),
    A("All three modules migrated; reviewer found one gap (idempotency key on payouts), fixed; all gates green."),
    U("read the reviewer report and the diff. merging."),
], note="spec + parallel agents + reviewer + automated gates; a one-off migration, so 4 is its ceiling")

scenario("c08", "code_change", 5, 5, [
    CMD("/dependency-bump", "minor"),
    T("Read", {"file_path": ".claude/commands/dependency-bump.md"}, "v3 - maintained by platform team. Steps: ..."),
    T("Bash", {"command": "pip-compile --upgrade-package '*' --minor-only"}, "updated 7 packages"),
    T("Bash", {"command": "pytest -q && ruff check . && pip-audit"}, "240 passed; no issues; no known vulnerabilities"),
    T("Bash", {"command": "gh pr create --fill --label deps"}, "https://github.com/acme/acme-shop/pull/311"),
    A("Opened PR #311 with 7 minor bumps; all checks pass. (dependency-bump v3, the same command CI runs every Monday.)"),
], note="runs the team's versioned workflow that CI and others also trigger")

scenario("c09", "code_change", 2, 3, [
    U("write unit tests for api/pricing.py::apply_discount. Cases: percentage, fixed amount, discount larger "
      "than subtotal (clamp to 0), expired code. Use pytest parametrize and our factories in tests/factories.py. "
      "Put them in tests/test_pricing.py."),
    T("Read", {"file_path": "api/pricing.py"}, "def apply_discount(...): ..."),
    T("Read", {"file_path": "tests/factories.py"}, "def make_order(...): ..."),
    T("Write", {"file_path": "tests/test_pricing.py", "content": "@pytest.mark.parametrize(...)"}),
    A("Added 4 parametrized cases in tests/test_pricing.py."),
    U("the expired case asserts the wrong exception, it should be DiscountExpired not ValueError. fix and stop"),
    T("Edit", {"file_path": "tests/test_pricing.py", "old_string": "ValueError", "new_string": "DiscountExpired"}),
], note="structured prompt; person checks output; tests never run by the agent")

scenario("c10", "code_change", 1, 2, [
    U("make this async\n\ndef fetch_rates(currency):\n    r = requests.get(URL + currency)\n    return r.json()"),
    A("```python\nasync def fetch_rates(currency):\n    async with httpx.AsyncClient() as c:\n        r = await c.get(URL + currency)\n        return r.json()\n```"),
    U("thx"),
], note="paste in, copy out")

scenario("c11", "code_change", 3, 4, [
    U("Rename the `Customer` model to `Account` across the codebase (models, routes, serializers, tests, "
      "alembic migration). Keep a deprecated alias `Customer = Account` for one release. Run tests and "
      "mypy at the end and report anything you could not rename safely."),
    T("Grep", {"pattern": "Customer", "path": "."}, "47 matches in 19 files"),
    T("Edit", {"file_path": "api/models.py", "old_string": "class Customer", "new_string": "class Account"}),
    T("Edit", {"file_path": "api/routes/customers.py", "old_string": "Customer", "new_string": "Account"}),
    T("Edit", {"file_path": "api/serializers.py", "old_string": "Customer", "new_string": "Account"}),
    T("Write", {"file_path": "migrations/versions/0042_rename_customer.py", "content": "op.rename_table(...)"}),
    T("Bash", {"command": "pytest -q && mypy api"}, "238 passed; Success"),
    A("Renamed in 19 files plus a migration; alias kept. Two string literals in analytics/export.py left unchanged "
      "because they are column names in a third-party report."),
    U("good call on the analytics strings. approved"),
], note="whole refactor delegated; person reviews")

scenario("c12", "code_change", 2, 3, [
    U("in api/search.py the query is slow. here's the EXPLAIN output:\n\nSeq Scan on products (cost=0.00..4410.00 rows=1)\n  Filter: (lower(name) ~~ '%shoe%')\n\n"
      "we're on postgres 15. suggest an index and the code change; don't run anything"),
    T("Read", {"file_path": "api/search.py"}, "WHERE lower(name) LIKE %s"),
    A("Add a trigram GIN index: `CREATE INDEX ... USING gin (lower(name) gin_trgm_ops)` and keep the query. "
      "Here's an alembic migration you can review."),
    U("ok write the migration file but I'll run it"),
    T("Write", {"file_path": "migrations/versions/0043_trgm_name.py", "content": "op.execute('CREATE EXTENSION IF NOT EXISTS pg_trgm') ..."}),
], note="good context and constraints, deliberately kept hands-on")

# =========================================================================== Write a spec or doc
scenario("d01", "write_doc", 1, 2, [
    U("rephrase: 'The system shall be able to provide the user the capability of exporting their data.'"),
    A("'Users can export their data.'"),
    U("more formal"),
    A("'The system allows users to export their data.'"),
    U("again"),
    A("'Users shall be able to export their account data.'"),
], note="ad hoc rewording, re-prompting without context")

scenario("d02", "write_doc", 2, 3, [
    U("Write the 'Getting started' section of README.md. Audience: new backend engineers. Must cover: "
      "prerequisites (python 3.12, docker), `make dev`, running tests, where config lives (api/settings.py). "
      "Max 40 lines, no marketing tone, match the style of docs/CONTRIBUTING.md."),
    T("Read", {"file_path": "docs/CONTRIBUTING.md"}, "## Setup ..."),
    T("Read", {"file_path": "Makefile"}, "dev: docker compose up ..."),
    T("Read", {"file_path": "api/settings.py"}, "class Settings(BaseSettings): ..."),
    A("Draft below (34 lines). I used the Makefile targets as they exist today."),
    U("drop the docker desktop note, we use colima. otherwise good, write it"),
    T("Edit", {"file_path": "README.md", "old_string": "## Getting started\n\nTBD", "new_string": "## Getting started\n\n..."}),
], note="structured prompt, reviewed before use")

scenario("d03", "write_doc", 2, 3, [
    U("Draft a one-page product spec for 'Export order history to CSV'. Users: small-business owners doing "
      "bookkeeping. Problem: they copy orders by hand into spreadsheets. Must include: goals, non-goals, "
      "user stories, acceptance criteria, open questions. Constraints: no new infra, GDPR export rules apply."),
    A("# Export order history to CSV\n## Goals ... ## Non-goals ... ## User stories ... ## Acceptance criteria ... ## Open questions ..."),
    U("the acceptance criteria are vague. make each one testable, and add a limit of 10k rows per export"),
    A("Updated acceptance criteria: 1. Given ... When ... Then ... (6 criteria), plus the 10k row limit."),
    U("better. I'll take it from here and edit it in the doc myself"),
], ambiguous="rung 2 vs 1: good structure, but the person finishes by hand", note="PM spec drafting in a chat-like loop")

scenario("d04", "write_doc", 3, 3, [
    U("Write ADR 0007 choosing Postgres over DynamoDB for the orders store. Read docs/architecture/overview.md, "
      "the discussion in docs/notes/2026-10-orders-store.md, and follow docs/adr/template.md exactly. Include "
      "the rejected option's trade-offs and a consequences section. Save as docs/adr/0007-orders-store.md and "
      "list any claim you couldn't source from those files."),
    T("Read", {"file_path": "docs/architecture/overview.md"}, "..."),
    T("Read", {"file_path": "docs/notes/2026-10-orders-store.md"}, "..."),
    T("Read", {"file_path": "docs/adr/template.md"}, "# ADR NNNN: Title\n## Status ..."),
    T("Write", {"file_path": "docs/adr/0007-orders-store.md", "content": "# ADR 0007: Postgres for the orders store ..."}),
    A("Written. Unsourced claims: none; one inference (write volume growth) is labelled as such."),
    U("read it through. accepted, committing."),
], note="whole document delegated with sources and template; reviewed")

scenario("d05", "write_doc", 4, 4, [
    U("Produce the 'Loyalty points' feature spec using our spec workflow: (1) a research sub-agent summarises "
      "docs/research/loyalty-*.md and support tickets export in data/tickets-loyalty.csv, (2) draft the spec from "
      "specs/template.md, (3) a reviewer sub-agent checks it against docs/standards/spec-checklist.md, "
      "(4) run markdownlint and the link checker. Iterate until the reviewer passes, max 2 rounds."),
    SUB("researcher", "Summarise loyalty research notes and the support-ticket CSV into needs with evidence.",
        [T("Read", {"file_path": "docs/research/loyalty-interviews.md"}, "..."),
         T("Read", {"file_path": "data/tickets-loyalty.csv"}, "id,subject,...")]),
    T("Read", {"file_path": "specs/template.md"}, "..."),
    T("Write", {"file_path": "specs/loyalty-points.md", "content": "# Loyalty points ..."}),
    SUB("spec-reviewer", "Check specs/loyalty-points.md against docs/standards/spec-checklist.md. Return failures only.",
        [T("Read", {"file_path": "docs/standards/spec-checklist.md"}, "...")]),
    T("Edit", {"file_path": "specs/loyalty-points.md", "old_string": "TBD", "new_string": "Points expire after 12 months (source: interview 4)."}),
    T("Bash", {"command": "npx markdownlint-cli2 specs/loyalty-points.md && npx lychee specs/loyalty-points.md"}, "0 errors; 0 broken links"),
    A("Spec written; reviewer passed in round 2; lint and links clean."),
    U("skimmed the reviewer notes and the spec, good to share"),
], note="multi-agent doc pipeline with automated checks; one-off spec")

scenario("d06", "write_doc", 5, 5, [
    CMD("/release-notes", "v2.14.0"),
    T("Read", {"file_path": ".claude/commands/release-notes.md"}, "v5 - owned by docs guild; used by every team"),
    T("Bash", {"command": "gh pr list --state merged --search 'milestone:v2.14.0' --json title,labels"}, "[...23 PRs]"),
    T("Write", {"file_path": "docs/releases/v2.14.0.md", "content": "# v2.14.0 ..."}),
    T("Bash", {"command": "npx markdownlint-cli2 docs/releases/v2.14.0.md"}, "0 errors"),
    A("Release notes for v2.14.0 written from 23 merged PRs (release-notes v5)."),
], note="team-wide versioned doc workflow")

scenario("d07", "write_doc", 1, 1, [
    U("typo in docs/faq.md line 12, 'recieve' -> 'receive'"),
    T("Edit", {"file_path": "docs/faq.md", "old_string": "recieve", "new_string": "receive"}),
], note="trivial fix; already at its right rung")

scenario("d08", "write_doc", 2, 3, [
    U("add google-style docstrings to every public function in api/pricing.py. Explain units (cents) and "
      "raised exceptions. Don't change any code."),
    T("Read", {"file_path": "api/pricing.py"}, "def apply_discount(...): ..."),
    T("Edit", {"file_path": "api/pricing.py", "old_string": "def apply_discount(order, code):", "new_string": "def apply_discount(order, code):\n    \"\"\"Apply a discount code ...\"\"\""}),
    A("Added docstrings to 6 public functions."),
    U("checked them, one is wrong: round_money returns int cents not float. fix"),
    T("Edit", {"file_path": "api/pricing.py", "old_string": "Returns: float", "new_string": "Returns: int (cents)"}),
], ambiguous="job: docstrings are documentation but the edit is in a code file", note="")

scenario("d09", "write_doc", 3, 4, [
    U("Write the on-call runbook for the payments service at docs/runbooks/payments.md. Pull alert names "
      "from ops/alerts/payments.yaml, dashboards from ops/dashboards.md, and the escalation list from "
      "docs/oncall.md. One section per alert: symptom, first checks, mitigation, escalation. Flag anything "
      "you had to guess."),
    T("Read", {"file_path": "ops/alerts/payments.yaml"}, "- alert: PaymentsErrorRateHigh ..."),
    T("Read", {"file_path": "ops/dashboards.md"}, "..."),
    T("Read", {"file_path": "docs/oncall.md"}, "..."),
    T("Write", {"file_path": "docs/runbooks/payments.md", "content": "# Payments runbook ..."}),
    A("7 alert sections written. Guessed: mitigation for PayoutsLagging (no source) - marked TODO."),
    U("reviewed. we'll want this for every service, but ship this one"),
], note="delegated doc; recurring across services, so could become orchestrated")

# =========================================================================== Other jobs (expect Unclassified)
scenario("o01", "other", 1, 1, [
    U("what does 'ECONNRESET' mean when calling redis from docker"),
    A("The connection was closed by the other side. Common causes: redis restarted, idle timeout, or the "
      "container network changed."),
    U("how do I see redis logs in compose"),
    A("`docker compose logs -f redis`"),
], note="Q&A / learning")

scenario("o02", "other", 2, 3, [
    U("Compare celery vs dramatiq vs arq for our background jobs. Context: python 3.12, redis already in the "
      "stack, ~50k jobs/day, need retries and scheduling. Give a table and a recommendation; cite docs."),
    A("| | celery | dramatiq | arq | ... Recommendation: dramatiq ..."),
    U("arq is async-native which we want; re-weigh with that and list migration effort from our cron jobs"),
    A("Re-weighed: arq ... migration effort: 3 cron jobs, ~2 days."),
], note="research / decision support, no artifact written")

scenario("o03", "other", 2, 3, [
    U("Here's data/conversion-oct.csv. Summarise conversion by channel and week, flag any week-over-week "
      "drop over 15%, and give me the pandas code so I can rerun it."),
    T("Read", {"file_path": "data/conversion-oct.csv"}, "week,channel,visits,orders ..."),
    T("Bash", {"command": "python -c \"import pandas as pd; ...\""}, "channel email: wk43 -22% ..."),
    A("Email dropped 22% in week 43; all other channels within 8%. Code below."),
    U("checked the numbers against the dashboard, they match"),
], note="data analysis")

scenario("o04", "other", 1, 2, [
    U("write my weekly status update from what I did"),
    T("Bash", {"command": "git log --author=me --since='1 week ago' --oneline"}, "a1b2 rate limit login\nc3d4 fix email price ..."),
    A("This week: shipped login rate limiting; fixed price formatting in confirmation emails; ..."),
    U("shorter"),
    A("Shipped login rate limiting and an email price fix."),
], ambiguous="job: a status update is writing, but status reporting is its own job (not 'spec or doc')",
   note="status reporting")

scenario("o05", "other", 2, 2, [
    U("docker compose up fails: 'port 5432 already allocated'. I'm on macOS with colima. Diagnose; don't kill "
      "anything without asking."),
    T("Bash", {"command": "lsof -i :5432"}, "postgres 812 me ... (Homebrew)"),
    A("A Homebrew postgres is running on 5432. Option: `brew services stop postgresql@15`. Shall I?"),
    U("yes"),
    T("Bash", {"command": "brew services stop postgresql@15 && docker compose up -d db"}, "Started"),
], note="environment troubleshooting")

scenario("o06", "other", 3, 3, [
    U("Review PR #305 (gh pr diff 305) against docs/standards/review-checklist.md. Report blocking issues "
      "first, then nits, with file:line. Don't post comments; I'll do that."),
    T("Bash", {"command": "gh pr diff 305"}, "diff --git a/api/refunds.py ..."),
    T("Read", {"file_path": "docs/standards/review-checklist.md"}, "..."),
    A("Blocking: refunds.py:88 no idempotency key. Nits: 3 ..."),
    U("agree on the blocker. thanks"),
], note="code review (reviewing, not changing code)")

# =========================================================================== Mixed / boundary
scenario("x01", "write_doc", 2, 3, [
    U("before I build it, write a short design note for the wishlist feature in docs/design/wishlist.md: data "
      "model (wishlist, wishlist_item), API endpoints, and what we won't do in v1. Keep it under a page."),
    T("Write", {"file_path": "docs/design/wishlist.md", "content": "# Wishlist design ..."}),
    A("Design note written: two tables, 4 endpoints, non-goals: sharing, price alerts."),
    U("drop the PATCH endpoint, v1 is add/remove only"),
    T("Edit", {"file_path": "docs/design/wishlist.md", "old_string": "PATCH /wishlist/items/{id}", "new_string": ""}),
], note="paired with x02: same feature, different job; must be two tasks")

scenario("x02", "code_change", 3, 4, [
    U("now implement the wishlist per docs/design/wishlist.md: models, alembic migration, the 3 endpoints, "
      "tests. Run the suite when done."),
    T("Read", {"file_path": "docs/design/wishlist.md"}, "..."),
    T("Edit", {"file_path": "api/models.py", "old_string": "# models end", "new_string": "class Wishlist(Base): ...\n# models end"}),
    T("Write", {"file_path": "migrations/versions/0044_wishlist.py", "content": "..."}),
    T("Write", {"file_path": "api/routes/wishlist.py", "content": "..."}),
    T("Write", {"file_path": "tests/test_wishlist.py", "content": "..."}),
    T("Bash", {"command": "pytest -q"}, "251 passed"),
    A("Implemented; 11 new tests; suite green."),
    U("diff looks good"),
], note="follows x01 in the same session within minutes")

scenario("x03", "code_change", 3, 4, [
    U("Add a `--dry-run` flag to scripts/reindex.py that prints what would be reindexed without writing. "
      "Update the CHANGELOG and the script's section in docs/ops.md. Test it with the sample index."),
    T("Read", {"file_path": "scripts/reindex.py"}, "..."),
    T("Edit", {"file_path": "scripts/reindex.py", "old_string": "parser.add_argument('--index')", "new_string": "parser.add_argument('--index')\nparser.add_argument('--dry-run', action='store_true')"}),
    T("Edit", {"file_path": "CHANGELOG.md", "old_string": "## Unreleased", "new_string": "## Unreleased\n- reindex: --dry-run"}),
    T("Edit", {"file_path": "docs/ops.md", "old_string": "### reindex", "new_string": "### reindex\nUse `--dry-run` to preview."}),
    T("Bash", {"command": "python scripts/reindex.py --index sample --dry-run"}, "would reindex 120 docs"),
    A("Flag added, docs and changelog updated, dry run on sample works."),
    U("nice"),
], note="code change with incidental doc edits (job is still code)")

SCENARIOS = {s["id"]: s for s in S}
