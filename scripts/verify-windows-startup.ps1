[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,
    [Parameter(Mandatory = $true)]
    [string]$InstallDirectory,
    [Parameter(Mandatory = $true)]
    [string]$DiagnosticsDirectory,
    [ValidateRange(10, 300)]
    [int]$StartupTimeoutSeconds = 60
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or -not [Environment]::Is64BitProcess) {
    throw 'Run this smoke with 64-bit PowerShell 7 on Windows.'
}

$InstallerPath = (Resolve-Path -LiteralPath $InstallerPath).Path
$InstallDirectory = [IO.Path]::GetFullPath($InstallDirectory)
$DiagnosticsDirectory = [IO.Path]::GetFullPath($DiagnosticsDirectory)
if ($InstallDirectory -notmatch ' ' -or $InstallDirectory.Contains('"')) {
    throw 'Use a fresh installation path containing spaces and no quotation marks.'
}
if (Test-Path -LiteralPath $InstallDirectory) {
    throw "Refusing to overwrite an existing installation: $InstallDirectory"
}
New-Item -ItemType Directory -Path $DiagnosticsDirectory -Force | Out-Null
Start-Transcript -Path (Join-Path $DiagnosticsDirectory 'smoke.log') | Out-Null
$launched = [Collections.Generic.List[object]]::new()
$failure = $null

function Start-SmokeProcess {
    param([string]$Executable, [string]$Arguments, [string]$Label)
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Executable
    $info.Arguments = $Arguments
    $info.WorkingDirectory = [IO.Path]::GetDirectoryName($Executable)
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    if (-not $process.Start()) { throw "Could not start $Label" }
    # Keep the native process handle, including when a short-lived process exits.
    $null = $process.Handle
    $record = [pscustomobject]@{
        Process = $process
        Label = $Label
        Stdout = $process.StandardOutput.ReadToEndAsync()
        Stderr = $process.StandardError.ReadToEndAsync()
    }
    $launched.Add($record)
    Write-Host "Started $Label (PID $($process.Id)): $Executable $Arguments"
    return $process
}

function Assert-AppAlive {
    param([Diagnostics.Process]$Process)
    $Process.Refresh()
    if ($Process.HasExited) {
        throw "Installed desktop exited early with code $($Process.ExitCode); see desktop.stdout.log and desktop.stderr.log."
    }
}

try {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class StartupWindow {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
}
'@

    # NSIS requires /D to be last, with its space-containing path UNQUOTED.
    # /CURRENTUSER selects the per-user mode when the installer supports both.
    $installer = Start-SmokeProcess $InstallerPath "/S /CURRENTUSER /D=$InstallDirectory" 'installer'
    if (-not $installer.WaitForExit(180000)) { throw 'NSIS installation timed out after 180 seconds.' }
    if ($installer.ExitCode -ne 0) { throw "NSIS installation failed with exit code $($installer.ExitCode)." }

    $desktopPath = Join-Path $InstallDirectory 'banana-hand.exe'
    $sidecarPath = Join-Path $InstallDirectory 'banana-hand-native-host.exe'
    foreach ($file in @($desktopPath, $sidecarPath)) {
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Installed executable missing: $file" }
    }
    Get-FileHash -LiteralPath $InstallerPath, $desktopPath, $sidecarPath |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $DiagnosticsDirectory 'binaries.json')

    $desktop = Start-SmokeProcess $desktopPath '' 'desktop'
    $deadline = [DateTime]::UtcNow.AddSeconds($StartupTimeoutSeconds)
    $window = [IntPtr]::Zero
    do {
        Assert-AppAlive $desktop
        $window = $desktop.MainWindowHandle
        if ($window -ne [IntPtr]::Zero -and [StartupWindow]::IsWindowVisible($window)) { break }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($window -eq [IntPtr]::Zero -or -not [StartupWindow]::IsWindowVisible($window)) {
        throw "Installed desktop did not show a visible window within $StartupTimeoutSeconds seconds."
    }
    $rect = [StartupWindow+Rect]::new()
    [uint32]$windowProcessId = 0
    $null = [StartupWindow]::GetWindowThreadProcessId($window, [ref]$windowProcessId)
    if ($windowProcessId -ne $desktop.Id -or -not [StartupWindow]::GetWindowRect($window, [ref]$rect) -or
        $rect.Right -le $rect.Left -or $rect.Bottom -le $rect.Top) {
        throw 'Desktop window has an invalid owner or empty bounds.'
    }
    [pscustomobject]@{
        processId = $desktop.Id
        hwnd = $window.ToInt64()
        title = $desktop.MainWindowTitle
        visible = $true
        left = $rect.Left
        top = $rect.Top
        width = $rect.Right - $rect.Left
        height = $rect.Bottom - $rect.Top
        capturedAtUtc = [DateTime]::UtcNow.ToString('o')
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $DiagnosticsDirectory 'window.json')

    # Hosted runner sessions do not always expose a capturable desktop. The
    # required HWND/bounds evidence above remains available if capture fails.
    $bitmap = $null
    $graphics = $null
    try {
        Add-Type -AssemblyName System.Drawing
        $bitmap = [Drawing.Bitmap]::new($rect.Right - $rect.Left, $rect.Bottom - $rect.Top)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $DiagnosticsDirectory 'desktop.png'), [Drawing.Imaging.ImageFormat]::Png)
    } catch {
        $_.ToString() | Set-Content -LiteralPath (Join-Path $DiagnosticsDirectory 'screenshot-unavailable.log')
        Write-Warning "Screenshot unavailable: $_"
    } finally {
        if ($null -ne $graphics) { $graphics.Dispose() }
        if ($null -ne $bitmap) { $bitmap.Dispose() }
    }

    $hostName = 'dev.bananahand.dispatch_host'
    $registrations = @(
        @{ Browser = 'chrome'; Key = "Software\Google\Chrome\NativeMessagingHosts\$hostName" },
        @{ Browser = 'firefox'; Key = "Software\Mozilla\NativeMessagingHosts\$hostName" }
    )
    $manifestPaths = [Collections.Generic.List[string]]::new()
    foreach ($registration in $registrations) {
        Assert-AppAlive $desktop
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($registration.Key)
        if ($null -eq $key) { throw "Missing HKCU\$($registration.Key)" }
        try {
            if ($key.GetValueKind('') -ne [Microsoft.Win32.RegistryValueKind]::String) {
                throw "HKCU\$($registration.Key) default value must be REG_SZ."
            }
            $manifestPath = [string]$key.GetValue('')
        } finally {
            $key.Dispose()
        }
        if (-not [IO.Path]::IsPathFullyQualified($manifestPath) -or
            -not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
            throw "Registry default must resolve to an absolute existing manifest: $manifestPath"
        }
        $manifestPaths.Add([IO.Path]::GetFullPath($manifestPath))
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -AsHashtable
        Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $DiagnosticsDirectory "$($registration.Browser)-manifest.json")
        [pscustomobject]@{ key = "HKCU\$($registration.Key)"; valueName = ''; kind = 'REG_SZ'; path = $manifestPath } |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $DiagnosticsDirectory "$($registration.Browser)-registry.json")
        if ($manifest.name -cne $hostName -or $manifest.type -cne 'stdio' -or
            -not [IO.Path]::IsPathFullyQualified([string]$manifest.path) -or
            -not [StringComparer]::OrdinalIgnoreCase.Equals([IO.Path]::GetFullPath([string]$manifest.path), $sidecarPath)) {
            throw "$($registration.Browser) manifest must identify the installed stdio native-host sidecar."
        }
        if ($registration.Browser -eq 'firefox') {
            if ($manifest.ContainsKey('allowed_origins') -or
                @($manifest.allowed_extensions) -cnotcontains 'bridge@banana-hand.dev') {
                throw 'Firefox manifest has an invalid extension allowlist.'
            }
        } elseif ($manifest.ContainsKey('allowed_extensions') -or
            @($manifest.allowed_origins) -cnotcontains 'chrome-extension://mooakjhlbkjfbmbmliklkmfmacnomlai/') {
            throw 'Chrome manifest has an invalid extension allowlist.'
        }
    }
    if ([StringComparer]::OrdinalIgnoreCase.Equals($manifestPaths[0], $manifestPaths[1])) {
        throw 'Firefox and Chrome must not share one browser-specific manifest.'
    }

    $selfCheck = Start-SmokeProcess $sidecarPath '--self-check' 'native-host-self-check'
    if (-not $selfCheck.WaitForExit(15000)) { throw 'Installed native-host --self-check timed out after 15 seconds.' }
    if ($selfCheck.ExitCode -ne 0) { throw "Installed native-host --self-check failed with exit code $($selfCheck.ExitCode)." }
    # A visible window alone could precede a delayed startup failure.
    $settleDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Assert-AppAlive $desktop
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $settleDeadline)
    Assert-AppAlive $desktop
    if (-not [StartupWindow]::IsWindowVisible($window)) { throw 'Desktop window disappeared after setup.' }
    'PASS: installed desktop stayed alive with a visible window; Chrome/Firefox registry discovery and native-host --self-check succeeded. No input injection or browser workflow was exercised.' |
        Tee-Object -FilePath (Join-Path $DiagnosticsDirectory 'result.txt')
} catch {
    $failure = $_
    $_.ToString() | Tee-Object -FilePath (Join-Path $DiagnosticsDirectory 'failure.txt') | Write-Host
} finally {
    # Only our retained process objects (and their children), never name-based
    # process killing, uninstalling, registry deletion, or user-data removal.
    foreach ($record in $launched) {
        $process = $record.Process
        try {
            $process.Refresh()
            $smokeStopped = -not $process.HasExited
            if ($smokeStopped) { $process.Kill($true) }
            if (-not $process.WaitForExit(10000)) { throw "$($record.Label) did not stop within 10 seconds." }
            [pscustomobject]@{ processId = $process.Id; exitCode = $process.ExitCode; stoppedBySmoke = $smokeStopped } |
                ConvertTo-Json | Set-Content -LiteralPath (Join-Path $DiagnosticsDirectory "$($record.Label)-exit.json")
            foreach ($stream in @(@{ Name = 'stdout'; Task = $record.Stdout }, @{ Name = 'stderr'; Task = $record.Stderr })) {
                if (-not $stream.Task.Wait(5000)) { throw "$($record.Label) $($stream.Name) capture timed out." }
                $text = $stream.Task.GetAwaiter().GetResult()
                $text | Set-Content -LiteralPath (Join-Path $DiagnosticsDirectory "$($record.Label).$($stream.Name).log")
                if ($text) { Write-Host "$($record.Label) $($stream.Name):`n$text" }
            }
        } catch {
            Write-Warning "Process cleanup/diagnostics failed: $_"
            if ($null -eq $failure) { $failure = $_ }
        } finally {
            $process.Dispose()
        }
    }
    Stop-Transcript | Out-Null
}
if ($null -ne $failure) { throw $failure }
