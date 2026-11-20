# Meanwhile uninstaller for Windows. ASCII only (works through `irm | iex`).
#   irm https://github.com/maxjustships/meanwhile/releases/latest/download/uninstall.ps1 | iex
# Removes the app, its Start menu shortcut, its autostart entry and its Claude Code / Codex hooks
# (other hooks are left alone). Settings in %APPDATA%\Meanwhile are kept.
& {
  $ErrorActionPreference = 'Stop'
  $dir = Join-Path $env:LOCALAPPDATA 'Meanwhile'
  $exe = Join-Path $dir 'meanwhile.exe'
  $hook = Join-Path $dir 'meanwhile-hook.exe'

  if (Test-Path $hook) {
    foreach ($a in 'claude', 'codex') {
      $out = & $hook setup $a --remove 2>&1
      Write-Host ($out -join "`n")
    }
  }
  Get-Process meanwhile -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe } | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 500

  $run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
  foreach ($name in 'Meanwhile', 'meanwhile') {
    if ((Get-ItemProperty $run -ErrorAction SilentlyContinue).$name) { Remove-ItemProperty $run -Name $name -ErrorAction SilentlyContinue }
  }
  $menu = if ($env:MEANWHILE_START_MENU) { $env:MEANWHILE_START_MENU } else { [Environment]::GetFolderPath('Programs') }
  Remove-Item (Join-Path $menu 'Meanwhile.lnk') -Force -ErrorAction SilentlyContinue
  Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
  Write-Host 'Meanwhile is uninstalled. Settings in %APPDATA%\Meanwhile were kept (delete that folder to forget them).'
}
