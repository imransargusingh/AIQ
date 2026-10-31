#!/usr/bin/env python3
"""coord_ids.py - collision-proof identifiers, in ONE place.

Imported by both `coord-core.py` and `audit-log.py`. It exists as its own module rather than
as a copy in each because six lines duplicated across two scripts is ONE-A -- a shared rule
with no gate accretes private copies, and the copies only diverge later, when one is edited.
The underscore in the filename is deliberate: a hyphen is not importable, which is why
`bounded_process.py` is named the way it is.

WHY NOT `uuid.uuid7()`: absent on the installed 3.12 (it landed in 3.14) and present on the
"3.x"-pinned CI runner. A stdlib call that exists on the runner and not on the developer's
machine is PACK-J by construction. Established by spike S1, not assumed.

WHY NOT SCANNING: the prevention built for KG-B scans every remote branch before allocating.
It works, it takes about a second over 22 branches, and it collided again within the hour --
two sessions that mint before either has pushed are invisible to each other. Scanning is
rejected as a DESIGN, not as an implementation.

Design: docs/design/coord-federation-phase3.md · ADR-0008.
"""
import os
import time

# Crockford base32: no I, L, O or U, so an id read aloud or copied by hand cannot become a
# different valid id.
_ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
ID_BODY_LEN = 26
_last_ms = 0


def _next_ms(now):
    """A millisecond stamp that is strictly increasing within this process (class ID-A).

    Two ids minted in one tick would otherwise order by their random bits, so "newest id" and
    `--since <id>` were non-deterministic under a burst. The stamp is per PROCESS: strict order
    is a per-issuer promise; collision-freedom across issuers is the 80 random bits, unchanged.
    Moved here from coord-mail.py so every prefix the pack mints gets it, not only `mail-`.
    """
    global _last_ms
    ms = int(now * 1000)
    if ms <= _last_ms:
        ms = _last_ms + 1
    _last_ms = ms
    return ms


def new_id(scheme, ts_ms=None):
    """48 bits of millisecond timestamp + 80 bits of os.urandom, Crockford base32.

    Time-ordered, so a register still sorts chronologically without a sequence. Issuance
    requires no communication between issuers -- that second property is the whole point,
    because a scheme that is only safe when issuers can see each other is the defect.

    Proven at 1,500 ids from 6 separate processes pinned to a single millisecond with no
    shared state and no network: 0 collisions.
    """
    ts = _next_ms(time.time()) if ts_ms is None else int(ts_ms)
    n = (ts << 80) | int.from_bytes(os.urandom(10), "big")
    body = "".join(_ALPHABET[(n >> (5 * i)) & 31] for i in range(ID_BODY_LEN - 1, -1, -1))
    return "{}-{}".format(scheme, body)


def resolve_prefix(rows, prefix):
    """(status, result, corpus_size) where status is unique | ambiguous | nomatch.

    The git short-hash idiom. ADR-0008 accepted that a 26-character id is not human-sayable
    but did not name the consumer that breaks: asking for an entry by number. Prefix recall
    restores that WITHOUT introducing a second identity.

    The corpus size is returned with EVERY verdict (R4/PACK-P): "not found" over an empty
    register must never render the same as "not found" over a full one.
    """
    corpus = len(rows)
    matches = [r for r in rows if str(r.get("id", "")).startswith(prefix)]
    if not matches:
        return "nomatch", None, corpus
    if len(matches) > 1:
        return "ambiguous", matches, corpus     # never "the first match"
    return "unique", matches[0], corpus
