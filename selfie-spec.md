# Selfie Package Manager Specification

## Overview

Selfie is a meta package manager and dotfile manager that orchestrates package installations across
different package managers and environments. It allows users to define their own package
installation rules per environment, managing dependencies and providing clear feedback about
installation progress.

## Architecture

Selfie follows Hexagonal Architecture with the following components:

### Core Components

1. **Entities (Core Business Logic)**
   - Package definitions
   - Environment configurations
   - Installation states
   - Dependency graphs

2. **Ports (Interfaces)**
   - Command execution
   - File system operations
   - Configuration management
   - Package management
   - Logging

3. **Adapters**
   - Shell command runner
   - File system operations
   - YAML configuration parser
   - Terminal UI
   - Log writer

4. **Services**
   - Package installation service
   - Dependency resolution service
   - Environment management service
   - Command execution service
   - Progress reporting service

### Data Structures

```rust
struct Config {
    environment: String,          // Required
    package_directory: PathBuf,   // Required
    command_timeout: u64,         // Default: 60 seconds
    stop_on_error: bool,         // Default: true
    max_parallel_installations: u32, // Default: 4
    logging: Option<LogConfig>,   // Optional
}

struct LogConfig {
    enabled: bool,               // Default: false
    directory: PathBuf,          // Required if enabled
    max_files: u32,             // Default: 10
    max_size: u64,              // Default: 10 (MB)
}

struct PackageNode {
    name: String,
    version: String,
    path: PathBuf,
    dependencies: Vec<String>,
}

struct DependencyGraph {
    nodes: HashMap<String, PackageNode>,
    cycles: Vec<Vec<String>>,
}

enum InstallationStatus {
    NotStarted,
    Checking,
    NotInstalled,
    Installing,
    Complete,
    Failed(String),
    Skipped(String),
}

struct PackageInstallation {
    name: String,
    version: String,
    path: PathBuf,
    status: InstallationStatus,
    dependencies: Vec<String>,
}

struct CommandOutput {
    region_start: u16,
    region_end: u16,
    content: String,
}

enum Message {
    Warning(Warning),
    Error(Error),
    Status(Status),
}

trait MessageRenderer {
    fn render(&self, message: &Message) -> String;
}
```

## Package Definition Format

```yaml
name: "package-name" # Required
version: "0.1.0" # Required
schema_version: "0.1.0" # Optional
homepage: "https://example.com" # Optional
description: "Package description" # Optional
environments: # Required (at least one)
  environment-name:
    shell: "/bin/bash" # Optional
    check: "which package-name" # Optional
    install: "brew install package-name" # Required
    dependencies: # Optional
      - dependency1
      - dependency2
```

## Configuration File Format

```yaml
environment: "work-mac"
package_directory: "~/.config/selfie/packages"
stop_on_error: true
command_timeout: 60
max_parallel_installations: 4
logging:
  enabled: false
  directory: "~/.config/selfie/logs"
  max_files: 10
  max_size: 10
```

Configuration file location search order:

1. XDG_CONFIG_HOME/selfie/
2. ~/.config/selfie/
3. ~/Library/Application Support/com.turboladen.selfie/ (macOS)

## Command Line Interface

### Core Commands

```bash
selfie package install [OPTIONS] <package-name>
selfie package list
selfie package info <package-name>
selfie package create <package-name>
selfie package validate <package-name>
selfie config validate
selfie environments list [--by-package]
```

### Global Options

```
--environment <name>       Override environment from config
--verbose                 Show detailed output
--no-color               Disable colored output
--log-enable            Enable logging
--log-directory <path>   Override log directory
--log-max-files <n>      Maximum log files to keep
--log-max-size <n>       Maximum log file size in MB
--command-timeout <n>    Command timeout in seconds
--max-parallel <n>       Maximum parallel installations
--no-parallel           Force sequential installation
--min-terminal-width <n> Minimum terminal width (default: 40)
```

## Validation Rules

### Package Validation

1. Quick validation (during installation):
   - Required fields exist and aren't empty
   - Current environment exists in environments list
   - Basic YAML syntax is valid
   - Schema version is valid if provided

2. Full validation (explicit validate command):
   - All quick validation checks
   - Command syntax validation
   - Homepage URL syntax validation
   - Path existence checks
   - Shell validation
   - Similar package name suggestions

### Configuration Validation

- Environment name must be specified
- Package directory must exist and be readable
- Command timeout must be positive
- Max parallel installations must be positive
- Log configuration must be valid if enabled

## Error Handling

### Error Types

1. Configuration Errors
   - Missing required fields
   - Invalid paths
   - Permission issues
   - Invalid values

2. Package Errors
   - Invalid package definitions
   - Missing dependencies
   - Circular dependencies
   - Environment compatibility

3. Installation Errors
   - Command execution failures
   - Timeouts
   - Permission issues
   - Shell execution errors

4. System Errors
   - File system errors
   - Terminal handling errors
   - Signal handling errors

### Error Response Strategy

1. User-facing errors:
   - Clear error messages
   - Suggested solutions
   - Relevant context
   - Color-coded output

2. Logging:
   - Detailed error information
   - Stack traces
   - Timing information
   - System state

## Progress Reporting

### Console Output

- Use indicatif for progress bars
- Color-coded status messages
- Hierarchical installation progress
- Timing information
- Command output streaming

### Progress Bar Template

```rust
"{prefix:.bold} {spinner} {wide_msg} ({elapsed})"
```

### Color Scheme

```rust
error: red().bold()
warning: yellow().bold()
success: green().bold()
info: blue().bold()
spinner: cyan()
elapsed: dimmed()
command: italic()
package_name: magenta().bold()
```

## Logging

### Log Entry Format

```
[timestamp] [level] [source:line] message
```

### Log Levels

- INFO: General progress information
- DEBUG: Detailed execution information
- WARN: Non-fatal issues
- ERROR: Fatal issues

### Log Rotation

- New file per execution
- Max files: 10 (configurable)
- Max size: 10MB (configurable)

## Testing Strategy

### Unit Tests

1. Core Logic
   - Package validation
   - Dependency resolution
   - Configuration parsing
   - Command parsing

2. Ports/Adapters
   - Mock file system operations
   - Mock command execution
   - Mock terminal output

### Integration Tests

1. Docker-based testing
   - Multiple OS environments
   - Real package manager interactions
   - Full installation workflows

2. Terminal handling
   - Color support
   - Unicode support
   - Window resizing

### Test Coverage Requirements

- Minimum 80% code coverage
- All error paths tested
- All configuration combinations tested
- All CLI commands tested

## Implementation Phases

### Phase 1: Core Infrastructure

1. Basic configuration management
2. Package definition parsing
3. Command execution framework
4. Logging infrastructure

### Phase 2: Package Management

1. Package installation
2. Dependency resolution
3. Environment handling
4. Progress reporting

### Phase 3: User Interface

1. Terminal UI
2. Color support
3. Progress indicators
4. Error reporting

### Phase 4: Advanced Features

1. Parallel installation
2. Signal handling
3. Terminal resizing
4. Log rotation

## Performance Requirements

1. Command execution timeout: 60s default
2. Maximum parallel installations: 4 default
3. Log rotation: 10 files, 10MB each
4. Minimum terminal width: 40 characters

## Security Considerations

1. Execute commands in isolated shell processes
2. Validate all file paths
3. Check file permissions
4. Strip ANSI controls from logs
5. Proper signal handling

## Dependencies

1. indicatif: Progress bars and spinners
2. console: Terminal colors and styling
3. serde: YAML parsing
4. chardetng: Command output encoding detection
5. tokio: Async runtime
