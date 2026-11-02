AIQ: a private coach for how you work with AI agents (GitHub Copilot, Claude Code).

Install (no admin rights needed):
  1. Unzip this folder anywhere.
  2. In a normal PowerShell window, in this folder, run:
       powershell -ExecutionPolicy Bypass -File .\install.ps1
  3. AIQ connects to the AI tools it finds, lists every file it changed, turns on judging of your
     finished tasks (stop it in AIQ home's Settings, or: aiq config set judge off), and opens AIQ home.

What leaves your machine: nothing, except the judge calls AIQ makes through your own Copilot or
Claude Code sign-in while judging is on (install turns it on; turn it off in Settings), and a daily check for a newer AIQ release on GitHub
that sends nothing about you. Your prompts stay in %USERPROFILE%\.aiq until you delete them.
Full privacy and alpha terms: https://github.com/imransargusingh/AIQ/blob/main/docs/privacy-and-terms.md

Useful commands (in a new terminal):
  aiq status              what AIQ is doing
  aiq pause | aiq resume  stop or restart AIQ
  aiq export              copy everything AIQ stored to plain files you can read
  aiq status --report     a help file with no prompt text or folder names, to send if you choose
  aiq uninstall --all     disconnect AIQ from every tool (your data stays)

Updates: AIQ updates itself. Once a day it asks GitHub whether a newer release is out, checks the
download's SHA-256, and installs it when AIQ home is closed. Your data and settings stay. Turn it to
"Tell me first" in AIQ home's Settings, or: aiq config set update ask. See what it did: aiq update

Upgrade to the newest release yourself (checks the download's SHA-256, keeps the old version):
  powershell -ExecutionPolicy Bypass -File $env:USERPROFILE\.aiq\bin\upgrade.ps1
Go back to the version before:
  powershell -ExecutionPolicy Bypass -File $env:USERPROFILE\.aiq\bin\upgrade.ps1 -Rollback
  aiq forget --all        delete everything AIQ stored

Check the download: SHA256SUMS from the same release lists each file's SHA-256.
  Get-FileHash .\aiq-<version>-windows-x64.zip -Algorithm SHA256
