"""Group human turns into tasks. Two strategies, compared by evaluate.py.

heuristic  the rung model's three signals, deterministic and on-device: time gap, shared artifact
           (files touched or named), and lexical similarity of prompts; plus explicit cue words
           ("continue ... from before", "now implement"). When uncertain it starts a new task
           (design doc: "When grouping is uncertain, it keeps the tasks separate").
model      one model call per day of turns: the model reads a compact list of turns (time,
           surface, prompt excerpt, files) and returns a task id per turn.
"""
import datetime as dt
import json
import re

STOP = set("""a an the and or to of in on for with from by is are be it this that these those i we you my our
me us do does did can could should would will please just also then now so if at as not no yes ok okay
use using make add run show write fix get let what how why when which there here all any one into out
up don't dont its it's be been was were has have had them they he she""".split())
CONTINUE_RX = re.compile(r"^(and |also |now |then |still |again|more |shorter|longer|ok |okay|yes|no |thx|thanks|"
                         r"looks |good|great|nice|better|agree|same |that |it |this |drop |fix |run that|checked|"
                         r"reviewed|read it|skimmed|diff |the )", re.I)
RESUME_RX = re.compile(r"\b(continue|resume|back to|pick up|from before|where we left|carry on)\b", re.I)
NEW_RX = re.compile(r"^(new task|unrelated|separately|different thing|switching to)\b", re.I)


def _ts(s):
    return dt.datetime.fromisoformat(s.replace("Z", "+00:00"))


def words(text):
    return {w for w in re.findall(r"[a-z][a-z0-9_]+", text.lower()) if w not in STOP and len(w) > 2}


def jaccard(a, b):
    return len(a & b) / len(a | b) if a and b else 0.0


def turn_files(t):
    return set(t["files"]) | set(t["mentioned"])


def heuristic(turns, gap_min=30, resume_hours=72):
    tasks = []  # each: {"id", "turns", "files", "words", "last", "session"}
    out = {}
    for t in turns:
        now = _ts(t["start"])
        tw, tf = words(t["prompt"]), turn_files(t)
        prev = tasks[-1] if tasks else None
        choice = None
        if t["command"]:
            choice = None  # a typed workflow command is its own unit of intent
        elif NEW_RX.search(t["prompt"]):
            choice = None
        elif RESUME_RX.search(t["prompt"]):
            # attach to the most similar earlier task within the resume window, any surface
            best, score = None, 0.0
            for k in tasks:
                if now - k["last"] > dt.timedelta(hours=resume_hours):
                    continue
                s = jaccard(tw, k["words"]) + 2 * jaccard(tf, k["files"])
                if s > score:
                    best, score = k, s
            choice = best if score >= 0.15 else None
        elif prev and prev["session"] == t["session"]:
            gap = (now - prev["last"]).total_seconds() / 60
            short = len(t["prompt"]) < 120
            overlap = bool(tf & prev["files"])
            sim = jaccard(tw, prev["words"])
            cue = bool(CONTINUE_RX.search(t["prompt"]))
            structured = len(t["prompt"]) >= 160
            if gap <= gap_min and (cue or overlap or sim >= 0.2 or (short and not tf)):
                choice = prev
            elif gap > gap_min and (cue and short or overlap and not structured):
                choice = prev  # a long idle gap alone does not end a task
            if structured and not overlap and sim < 0.2 and not RESUME_RX.search(t["prompt"]):
                choice = None  # a new full brief with no shared artifact starts a new task
            # tuned on the dev corpus (see report): a short prompt that is mostly new words, with no
            # continuation cue and no shared file, is a new request ("rephrase: ...", "what does X mean")
            novelty = len(tw - prev["words"]) / len(tw) if tw else 0.0
            if not cue and not overlap and len(tw) >= 3 and novelty >= 0.6:
                choice = None
        if choice is None:
            choice = {"id": "h{0}".format(len(tasks) + 1), "turns": [], "files": set(), "words": set(),
                      "last": now, "session": t["session"]}
            tasks.append(choice)
        else:  # keep the most recently active task last
            tasks.remove(choice)
            tasks.append(choice)
        choice["turns"].append(t["key"])
        choice["files"] |= tf
        choice["words"] |= tw
        choice["last"] = _ts(t["end"] or t["start"])
        choice["session"] = t["session"]
        out[t["key"]] = choice["id"]
    return out


GROUP_SCHEMA = {
    "type": "object",
    "properties": {"assignments": {"type": "array", "items": {
        "type": "object",
        "properties": {"turn": {"type": "integer"}, "task": {"type": "integer"}},
        "required": ["turn", "task"]}}},
    "required": ["assignments"],
}

GROUP_SYSTEM = """You group a person's AI-agent turns into tasks. A task is one unit of intent: what the person
was trying to get done, not one prompt. One task can span several prompts, sessions, surfaces (Claude Code,
Copilot CLI) and days. Signals: time gaps, shared artifacts (same files), and meaning of the prompts. Short
follow-ups ("still failing", "shorter", "looks good") belong to the task before them. A prompt that resumes
earlier work ("continue X from before") joins that earlier task even across sessions. Two prompts about the
same feature with a different purpose (e.g. writing its design doc, then implementing it) are different
tasks. When unsure, keep tasks separate. Return a task number for every turn; number tasks from 1 in
order of first appearance."""


def model_input(turns):
    lines = []
    for i, t in enumerate(turns, 1):
        files = sorted(turn_files(t))[:6]
        lines.append("{0}. [{1} {2} session={3}] {4!r} files={5}".format(
            i, t["start"][:16], t["surface"], t["session"][:6], t["prompt"][:220], files))
    return "\n".join(lines)


def model(turns, call):
    """call(system, prompt, schema) -> (structured dict, meta). One call for the whole window."""
    res, meta = call(GROUP_SYSTEM, "Turns:\n" + model_input(turns), GROUP_SCHEMA)
    out = {}
    by_index = {a["turn"]: a["task"] for a in (res or {}).get("assignments", [])}
    for i, t in enumerate(turns, 1):
        out[t["key"]] = "m{0}".format(by_index.get(i, "missing-{0}".format(i)))
    return out, meta
