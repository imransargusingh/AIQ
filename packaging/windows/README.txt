AIQ: a private coach for how you work with AI agents (GitHub Copilot, Claude Code).

Install (no admin rights needed):
  1. Unzip this folder anywhere.
  2. In a normal PowerShell window, in this folder, run:
       powershell -ExecutionPolicy Bypass -File .\install.ps1
  3. AIQ connects to the AI tools it finds, lists every file it changed, and opens AIQ home.

What leaves your machine: nothing, except the judge calls AIQ makes through your own Copilot or
Claude Code sign-in. Your prompts stay in %USERPROFILE%\.aiq until you delete them.

Useful commands (in a new terminal):
  aiq status              what AIQ is doing
  aiq pause | aiq resume  stop or restart AIQ
  aiq export              copy everything AIQ stored to plain files you can read
  aiq status --report     a help file with no prompt text or folder names, to send if you choose
  aiq uninstall --all     disconnect AIQ from every tool (your data stays)
  aiq forget --all        delete everything AIQ stored

Check the download: SHA256SUMS from the same release lists each file's SHA-256.
  Get-FileHash .\aiq-<version>-windows-x64.zip -Algorithm SHA256
