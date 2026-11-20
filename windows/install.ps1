# Meanwhile installer for Windows 10/11. No admin rights needed. ASCII only on purpose (works through `irm | iex`).
#   irm https://github.com/maxjustships/meanwhile/releases/latest/download/install.ps1 | iex
#
# What it does: downloads Meanwhile-win-x64.zip, checks its SHA-256 against SHA256SUMS, unpacks it into
# %LOCALAPPDATA%\Meanwhile, adds a Start menu shortcut, connects Claude Code / Codex hooks if they are
# installed (existing hooks are kept, a backup is written next to each file) and starts the app.
#
# Optional environment variables, set before running:
#   $env:MEANWHILE_VERSION  = 'v0.1.0'   install a specific release (default: latest)
#   $env:MEANWHILE_NO_HOOKS = '1'        do not touch Claude Code / Codex hook files
#   $env:MEANWHILE_NO_LAUNCH = '1'       do not start the app after installing
& {
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

  $repo = 'maxjustships/meanwhile'
  $base = $env:MEANWHILE_BASE_URL
  if (-not $base) {
    $base = if ($env:MEANWHILE_VERSION) { "https://github.com/$repo/releases/download/$($env:MEANWHILE_VERSION)" } else { "https://github.com/$repo/releases/latest/download" }
  }
  $base = $base.TrimEnd('/')
  $dir = Join-Path $env:LOCALAPPDATA 'Meanwhile'
  $exe = Join-Path $dir 'meanwhile.exe'
  $hook = Join-Path $dir 'meanwhile-hook.exe'

  function Get-Url($url, $out) {
    if ($url -like 'file:*') { Copy-Item ([Uri]$url).LocalPath $out -Force }
    else { Invoke-WebRequest $url -OutFile $out -UseBasicParsing }
  }

  $tmp = Join-Path $env:TEMP ('meanwhile-' + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force $tmp | Out-Null
  try {
    $zip = Join-Path $tmp 'Meanwhile-win-x64.zip'
    Write-Host 'Downloading Meanwhile...'
    Get-Url "$base/Meanwhile-win-x64.zip" $zip
    Get-Url "$base/SHA256SUMS" (Join-Path $tmp 'SHA256SUMS')
    $sums = Get-Content (Join-Path $tmp 'SHA256SUMS') -Raw
    $want = $null
    foreach ($line in ($sums -split "`n")) { if ($line -match '^([0-9a-fA-F]{64})\s+\*?Meanwhile-win-x64\.zip\s*$') { $want = $Matches[1].ToLower() } }
    if (-not $want) { throw 'SHA256SUMS has no entry for Meanwhile-win-x64.zip' }
    $got = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
    if ($got -ne $want) { throw "Checksum mismatch: expected $want, got $got. Nothing was installed." }

    # a running copy holds the exe open: stop it (it is only a tray app, nothing is lost)
    Get-Process meanwhile -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe } | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 400

    New-Item -ItemType Directory -Force $dir | Out-Null
    $ex = Join-Path $tmp 'x'
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::ExtractToDirectory($zip, $ex)
    Get-ChildItem $ex -File | ForEach-Object { Copy-Item $_.FullName (Join-Path $dir $_.Name) -Force }
  } finally { Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue }

  # Start menu shortcut (MEANWHILE_START_MENU only exists for tests)
  $menu = if ($env:MEANWHILE_START_MENU) { $env:MEANWHILE_START_MENU } else { [Environment]::GetFolderPath('Programs') }
  New-Item -ItemType Directory -Force $menu | Out-Null
  $sh = New-Object -ComObject WScript.Shell
  $s = $sh.CreateShortcut((Join-Path $menu 'Meanwhile.lnk'))
  $s.TargetPath = $exe; $s.WorkingDirectory = $dir; $s.IconLocation = "$exe,0"
  $s.Description = 'Something to do while your AI thinks'
  $s.Save()

  # agent hooks
  $hooksDone = @()
  if ($env:MEANWHILE_NO_HOOKS -ne '1') {
    $claudeDir = if ($env:CLAUDE_CONFIG_DIR) { $env:CLAUDE_CONFIG_DIR } else { Join-Path $env:USERPROFILE '.claude' }
    $codexDir = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $env:USERPROFILE '.codex' }
    foreach ($a in @(@{ n = 'claude'; d = $claudeDir }, @{ n = 'codex'; d = $codexDir })) {
      if (Test-Path $a.d) {
        $out = & $hook setup $a.n 2>&1
        Write-Host ($out -join "`n")
        if ($LASTEXITCODE -eq 0) { $hooksDone += $a.n }
      }
    }
  }

  if ($env:MEANWHILE_NO_LAUNCH -ne '1') { Start-Process $exe -WorkingDirectory $dir }

  Write-Host ''
  Write-Host "Meanwhile installed in $dir and started (look for the three dots in the tray)."
  Write-Host 'Hotkey:     Ctrl+Alt+Space (change it in the tray menu > Settings)'
  if ($hooksDone.Count) { Write-Host "Hooks:      connected to $($hooksDone -join ', ')" } else { Write-Host 'Hooks:      none connected (open Settings from the tray to connect Claude Code or Codex)' }
  if ($hooksDone -contains 'codex') { Write-Host 'Codex:      new hooks need one approval: /hooks in the CLI, or Review hooks / Settings > Hooks in the desktop app.' }
  Write-Host 'Not signed: if SmartScreen warns, choose More info > Run anyway.'
  Write-Host "Uninstall:  irm https://github.com/$repo/releases/latest/download/uninstall.ps1 | iex"
}
