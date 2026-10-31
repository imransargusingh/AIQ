"""Model endpoints for the judge. The person chooses the endpoint; nothing else leaves the machine.

claude-cli   runs the local `claude -p` with no tools, no settings, no MCP and no session
             persistence, and asks for schema-checked JSON (--json-schema). Used in the spike.
openai       any OpenAI-compatible chat endpoint (e.g. a local llama.cpp / Ollama server at
             http://localhost:11434/v1). assume: the server supports response_format json_schema;
             untested in the spike (no local model server in the spike environment).

Every call returns (parsed dict or None, meta) where meta carries latency_ms and, when the
endpoint reports them, tokens and cost. A missing figure is None ("not recorded"), never a guess.
"""
import json
import os
import subprocess
import time
import urllib.request


def claude_cli(model):
    def call(system, prompt, schema):
        t0 = time.time()
        cmd = ["claude", "-p", "--model", model, "--tools", "", "--setting-sources", "", "--strict-mcp-config",
               "--no-session-persistence", "--system-prompt", system, "--output-format", "json",
               "--json-schema", json.dumps(schema)]
        p = subprocess.run(cmd, input=prompt, capture_output=True, text=True, encoding="utf-8",
                           timeout=600, cwd=os.path.expanduser("~"))
        wall = int((time.time() - t0) * 1000)
        meta = {"latency_ms": wall, "api_ms": None, "input_tokens": None, "output_tokens": None,
                "cost_usd": None, "error": None}
        try:
            d = json.loads(p.stdout)
        except ValueError:
            meta["error"] = (p.stderr or p.stdout)[:300]
            return None, meta
        u = d.get("usage") or {}
        meta.update(api_ms=d.get("duration_api_ms"), cost_usd=d.get("total_cost_usd"),
                    input_tokens=(u.get("input_tokens") or 0) + (u.get("cache_read_input_tokens") or 0)
                    + (u.get("cache_creation_input_tokens") or 0),
                    output_tokens=u.get("output_tokens"))
        if d.get("is_error"):
            meta["error"] = str(d.get("result"))[:300]
        return d.get("structured_output"), meta
    return call


def openai_compatible(base_url, model, api_key=None):
    def call(system, prompt, schema):
        t0 = time.time()
        body = {"model": model, "messages": [{"role": "system", "content": system}, {"role": "user", "content": prompt}],
                "response_format": {"type": "json_schema", "json_schema": {"name": "out", "schema": schema}}}
        req = urllib.request.Request(base_url.rstrip("/") + "/chat/completions", data=json.dumps(body).encode(),
                                     headers={"Content-Type": "application/json",
                                              **({"Authorization": "Bearer " + api_key} if api_key else {})})
        meta = {"latency_ms": None, "api_ms": None, "input_tokens": None, "output_tokens": None, "cost_usd": None, "error": None}
        try:
            with urllib.request.urlopen(req, timeout=600) as r:
                d = json.load(r)
            u = d.get("usage") or {}
            meta.update(input_tokens=u.get("prompt_tokens"), output_tokens=u.get("completion_tokens"))
            out = json.loads(d["choices"][0]["message"]["content"])
        except Exception as exc:  # noqa: BLE001 - spike: report any endpoint failure as data
            meta["error"] = str(exc)[:300]
            out = None
        meta["latency_ms"] = int((time.time() - t0) * 1000)
        return out, meta
    return call


def endpoint(spec):
    """'claude-cli:<model>' or 'openai:<base_url>|<model>'."""
    kind, _, rest = spec.partition(":")
    if kind == "claude-cli":
        return claude_cli(rest or "claude-haiku-5-5")
    if kind == "openai":
        url, _, model = rest.partition("|")
        return openai_compatible(url, model, os.environ.get("AIQ_JUDGE_API_KEY"))
    raise SystemExit("unknown endpoint: " + spec)
