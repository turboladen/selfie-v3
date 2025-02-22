# Selfie Output Format Specification

## Standard Command Outputs

### Package Installation

Basic installation:
```
Installing ripgrep (v0.1.0) from ~/.config/selfie/packages
  ✓ Checking installation status: Not installed (0.2s)
  ⌛ Installing...
  ✓ Installation complete (45.3s)
Total time: 45.5s
```

Installation with dependencies:
```
Installing ripgrep (v0.1.0) from ~/.config/selfie/packages
  Dependencies:
    Installing rust (v0.1.0)
      ✓ Checking installation status: Not installed (0.2s)
      ⌛ Installing...
      ✓ Installation complete (182.5s)
    Installing cargo-binstall (v0.1.0)
      ✓ Checking installation status: Already installed (0.1s)
  ✓ Checking installation status: Not installed (0.2s)
  ⌛ Installing...
  ✓ Installation complete (45.3s)

Total time: 228.3s
Dependencies: 182.8s
Package: 45.5s
```

### Parallel Installation

```
Installing 2 packages in parallel:

ripgrep (v0.1.0):
  ✓ Checking installation status: Not installed (0.2s)
  ⌛ Installing...
  ✓ Installation complete (45.3s)

rust-analyzer (v0.1.0):
  ✓ Checking installation status: Not installed (0.3s)
  ⌛ Installing...
  ✓ Installation complete (62.1s)

Total wall time: 62.1s
Individual times:
  • ripgrep: 45.5s
  • rust-analyzer: 62.4s
```

### Package Listing

```
Available packages:
  ripgrep (v0.1.0) - Compatible with current environment
  rust (v0.1.0) - Not compatible with current environment
  typos-cli (v0.2.0) - Compatible with current environment
```

### Package Information

```
Package: ripgrep
Version: v0.1.0
Homepage: https://github.com/BurntSushi/ripgrep
Description: Fast line-oriented search tool, recursively searches directories for a regex pattern

Environment Compatibility:
  • work-mac (current) - Compatible
  • home-mac - Compatible
  • arch-linux - Not configured

Dependencies:
  • rust
  • cargo-binstall

Package file is valid
```

### Environment Listing

By environment:
```
Available environments across all packages:

work-mac
  Used in packages:
    • ripgrep
    • rust
    • cargo-binstall

home-mac
  Used in packages:
    • ripgrep
    • neovim
    • typos-cli

arch-linux
  Used in packages:
    • ripgrep
    • rust-analyzer
```

By package:
```
Environment usage by package:

ripgrep:
  • work-mac
  • home-mac
  • arch-linux

rust:
  • work-mac
  • arch-linux

cargo-binstall:
  • work-mac
```

## Error Messages

### Configuration Errors

Missing required configuration:
```
Error: Missing required configuration

The following required values are not set:
  • environment: Must specify current environment name
  • package_directory: Must specify path to package definitions

Set these values either in:
  1. Config file (~/.config/selfie/config.yaml), or
  2. Command line flags (--environment, --package-directory)
```

Invalid configuration values:
```
Error: Invalid configuration in ~/.config/selfie/config.yaml

Invalid values:
  • command_timeout: "invalid" (must be a positive number)
  • max_parallel_installations: -1 (must be a positive number)

Missing required values:
  • environment: Must specify current environment name

Note: Numbers must be positive integers without quotes
```

### Package Errors

Package not found:
```
Error: Package 'neovim' not found

Did you mean to install the package group 'neovim'?
If so, run: selfie group install neovim

Available similar packages:
  • neovim-utils
  • nvim-config
```

Invalid package file:
```
Validation failed for package: ripgrep
Schema errors:
  • Line 3: Required field 'version' is missing
  • Line 7: Field 'environments' must contain at least one environment

Command syntax errors:
  • Line 12: Invalid shell syntax in install command: Unclosed quote
  • Line 15: Invalid shell syntax in check command: Invalid pipe usage

You can find the package file at: ~/.config/selfie/packages/ripgrep.yaml
```

### Installation Errors

Command failure:
```
Error: Installation command failed
Package: ripgrep
Command: 'brew install ripgrep'
Exit Code: 1

Error output:
  Error: Failed to download package
  Error: Connection timed out

Note: Only exit code 0 indicates successful installation.
      Check the package manager's error output above for details.
```

Unknown exit code:
```
Error: Installation command failed
Package: ripgrep
Command: 'brew install ripgrep'
Exit Code: 137

stdout:
  [no output]

stderr:
  [no output]

The command exited with a non-zero status.
Check the package manager's documentation for exit code meanings.
```

Dependency error:
```
Installing ripgrep (v0.1.0) from ~/.config/selfie/packages
  Dependencies:
    ✗ Error: Package 'cargo-binstall' does not support environment 'work-mac'
  Installation aborted due to incompatible dependency

Note: The package 'cargo-binstall' needs installation rules for the 'work-mac' 
      environment before it can be installed as a dependency
```

Installation interrupted:
```
^C
Installation interrupted. The following operations were in progress:
  • Installing: cargo-binstall
  • Pending: ripgrep, rust-analyzer

Some packages may be partially installed.
```

### Permission Errors

Package directory:
```
Error: Cannot read package directory
Path: /home/user/.config/selfie/packages
Reason: Permission denied

Please check the directory permissions:
  1. Verify you have read permissions: ls -l /home/user/.config/selfie/packages
  2. Fix permissions if needed: chmod +r /home/user/.config/selfie/packages
  3. Make sure you own the directory or are in the correct group
```

Log directory:
```
Error: Log directory not found
Directory: ~/.config/selfie/logs

Logging is enabled but the log directory does not exist.
Please create the directory and ensure it is writable:

  mkdir -p ~/.config/selfie/logs
  chmod 755 ~/.config/selfie/logs

You can also specify a different log directory:
  --log-directory <path>
```

Log permissions:
```
Error: Logging permission denied

Directory ~/.config/selfie/logs:
  ✗ Missing write permission
  Current: drwxr-xr-x (755)
  Need: drwxrwxr-x (775)

Log file ~/.config/selfie/logs/selfie_20250221.log:
  ✗ Missing write permission
  Current: -rw-r--r-- (644)
  Need: -rw-rw-r-- (664)

Please fix permissions with:
  chmod 775 ~/.config/selfie/logs
  chmod 664 ~/.config/selfie/logs/*.log
```

### Shell Errors

Shell not found:
```
Error: Shell not found
Package: ripgrep
Environment: work-mac
Specified shell: /bin/zsh
Command: brew install ripgrep

The shell specified in the package definition was not found.
Please verify:
  1. The shell path is correct for this environment
  2. The shell is installed on this system
  3. You have the correct environment selected
```

Command execution:
```
Error: Failed to execute check command
Package: ripgrep
Command: 'which ripgrep'
Error: Permission denied (os error 13)

Installation cannot proceed when the check command fails.
Please verify the command can be executed in your shell.
```

### Display Errors

Terminal too narrow:
```
! Terminal too narrow for full output
! Min width: 40, Current: 30

Installing ripg...
  ⌛ Install...
    stdout: Bui...
    stdout: Tes...
```

## Verbose Output Format

Command output display:
```
Installing ripgrep (v0.1.0)
  ⌛ Installing...
    stdout: Building ripgrep from source...
    stdout: Testing installation
    stderr: warning: build will be slower, using debug mode
  ✓ Installation complete
```

Partial line output:
```
Installing ripgrep (v0.1.0)
  ⌛ Installing...
    stdout: Building ri[partial line, no newline]
    stderr: warning: sl[partial line, no newline]
    stdout: Building ripgrep from source...
    stderr: warning: slow build due to debug mode
    stdout: Done.
```

## Log File Format

Standard log entries:
```
[2025-02-21T14:30:00.123Z] [INFO] [package_installer.rs:45] Starting installation of package 'ripgrep'
[2025-02-21T14:30:00.234Z] [DEBUG] [command_runner.rs:78] Executing command: 'which ripgrep'
[2025-02-21T14:30:00.345Z] [INFO] [package_installer.rs:67] Package check returned: not installed
[2025-02-21T14:30:00.456Z] [DEBUG] [command_runner.rs:78] Command output:
[2025-02-21T14:30:00.457Z] [DEBUG] [command_runner.rs:79] stdout: Installing ripgrep
[2025-02-21T14:30:00.458Z] [DEBUG] [command_runner.rs:80] stdout: Downloading ripgrep-13.0.0.tar.gz
```

Shutdown sequence:
```
[2025-02-21T14:30:45.567Z] [INFO] [signal_handler.rs:45] Received signal: SIGTERM
[2025-02-21T14:30:45.568Z] [INFO] [shutdown.rs:23] Beginning shutdown sequence
[2025-02-21T14:30:45.569Z] [INFO] [shutdown.rs:25] Stopping package installation queue (2ms)
[2025-02-21T14:30:45.570Z] [INFO] [shutdown.rs:27] Sending termination signal to child processes (1ms)
[2025-02-21T14:30:46.071Z] [INFO] [shutdown.rs:30] Force killing remaining processes (501ms)
[2025-02-21T14:30:46.072Z] [INFO] [shutdown.rs:32] Cleanup complete (1ms)
[2025-02-21T14:30:46.073Z] [INFO] [shutdown.rs:34] Selfie shutting down (Total: 506ms)
```

Failed shutdown:
```
[2025-02-21T14:30:45.567Z] [WARN] [shutdown.rs:45] Failed to terminate process 1234 (brew)
[2025-02-21T14:30:45.568Z] [INFO] [shutdown.rs:46] Abandoning cleanup after 500ms
[2025-02-21T14:30:45.569Z] [INFO] [shutdown.rs:47] Selfie exiting with incomplete cleanup
```

## Version Information

```
selfie v0.1.0

Supported schema versions:
  • 0.1.0 (current)

Config file: ~/.config/selfie/config.yaml
Package directory: ~/.config/selfie/packages
Current environment: work-mac

Package counts:
  • Total valid packages: 12
  • Packages for work-mac: 8
  • Invalid package files: 2

Note: Run 'selfie package validate' on failing packages for details.
```