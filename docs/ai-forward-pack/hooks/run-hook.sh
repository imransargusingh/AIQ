#!/bin/sh
# Run one ai-forward pack hook with a Python 3 interpreter resolved at run time (PLAT-A, PLAT-C).
#
# Antigravity runs hook commands through cmd.exe on Windows, from <repo>/.agents (measured 2026-09-24,
# agy 1.2.10), so its hook config cannot hold POSIX shell syntax. Each agy hook command is therefore one
# plain `git` invocation, parsed alike by cmd.exe and sh, that runs this script as a git alias:
#   git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh <hook>.py --host agy ...
# git runs `!` aliases through its own sh, from the top of the working tree, and passes stdin and the
# exit status through, so a hook's JSON payload and its blocking exit code both reach the host unchanged.
#
# Codex on Windows runs hook commands through `pwsh -Command` (measured 2026-09-24, codex 0.156), so its
# ownership hook uses the same form. Codex resolves a relative patch path against the hook's process cwd,
# so its command passes --caller-cwd: the launcher returns to the directory git was called from
# ($GIT_PREFIX, which git sets for `!` aliases) before it runs the hook:
#   git -c alias.aif-hook=!sh aif-hook docs/ai-forward-pack/hooks/run-hook.sh --caller-cwd ../scripts/coord-core.py hook --host codex
# The hook is then named RELATIVE to that directory (one ../ per $GIT_PREFIX segment), never by the
# absolute working directory: Git for Windows' sh passes an absolute /c/... argument that holds ' ` or ;
# to a native python.exe unconverted, and python cannot open it (measured 2026-09-24, git 2.55.0.windows.2).
#
# Usage: run-hook.sh [--caller-cwd] <file under docs/ai-forward-pack/hooks/> [hook arguments...]
up=
if [ "$1" = "--caller-cwd" ]; then
  shift
  rest=$GIT_PREFIX
  # each pass strips one "segment/" from rest, so the loop ends when no "/" is left
  while [ "${rest#*/}" != "$rest" ]; do up="../$up"; rest=${rest#*/}; done
  cd "./$GIT_PREFIX" || exit 2
fi
script="${up}docs/ai-forward-pack/hooks/$1"
shift
# The interpreter is resolved at run time (PLAT-A/PLAT-B). Windows sets OS=Windows_NT for every
# process. There python.org ships no python3.exe, and the python3 on PATH is a Store alias that
# exits 9009 after ~200 ms (measured 2026-10-14, on every hook call), so Windows asks python first.
first=python3; second=python
if [ "$OS" = "Windows_NT" ]; then first=python; second=python3; fi
py=$($first -c 'import sys;print(sys.executable if sys.version_info >= (3, 10) else "")' 2>/dev/null)
[ -x "$py" ] || py=$($second -c 'import sys;print(sys.executable if sys.version_info >= (3, 10) else "")' 2>/dev/null)
# The portable bootstrap may supply Python through uv without adding Python to PATH.
# Keep native Python preferred; uv isolates this hook from the project's dependencies.
if [ ! -x "$py" ] && command -v uv >/dev/null 2>&1; then
  if [ -n "${UV_PYTHON:-}" ]; then
    requested=$UV_PYTHON
    uv_py=$(UV_PYTHON= uv python find --no-config --no-project "$requested" 2>/dev/null)
    if [ -x "$uv_py" ] && "$uv_py" -c 'import sys;raise SystemExit(0 if sys.version_info >= (3, 10) else 1)' >/dev/null 2>&1; then
      exec uv run --no-config --no-project --python "$uv_py" "$script" "$@"
    fi
  fi
  exec uv run --no-config --no-project --python '>=3.10' "$script" "$@"
fi
exec "$py" "$script" "$@"
