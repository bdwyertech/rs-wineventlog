# rs-wineventlog

Windows Event Log monitor and exporter written in Rust.

[![Release](https://img.shields.io/github/v/release/bdwyertech/rs-wineventlog)](https://github.com/bdwyertech/rs-wineventlog/releases)
[![Build Provenance](https://img.shields.io/badge/provenance-attested-blue)](https://github.com/bdwyertech/rs-wineventlog/attestations)

## Features

- Real-time Windows Event Log monitoring
- JSON output (stdout or file)
- Pattern matching for channel selection
- Configurable batch processing
- Graceful shutdown handling
- Build provenance attestations

## Installation

Download the latest release from [GitHub Releases](https://github.com/bdwyertech/rs-wineventlog/releases).

## Configuration

Create a `config.yaml` file:

```yaml
# Optional: Write to file instead of stdout
# output_file: events.log

# Optional: Number of events to fetch per batch (default: 10)
# batch_size: 10

# Required: List of channels to monitor
channels:
  - Application
  - System
  - Security  # Requires elevated privileges
```

## Usage

```bash
# Monitor with default config
rs-wineventlog

# Use custom config
rs-wineventlog --config /path/to/config.yaml

# Pretty-print JSON
rs-wineventlog --pretty-json

# List available channels
rs-wineventlog list-channels

# Show version
rs-wineventlog --version
```

## Environment Variables

Override config values with environment variables:

```bash
WINEVENTLOG_BATCH_SIZE=50 rs-wineventlog
WINEVENTLOG_OUTPUT_FILE=events.log rs-wineventlog
```

## Verification

All releases include build provenance attestations and signed checksums.

### Verify Build Provenance

```bash
gh attestation verify wineventlog.exe --owner bdwyertech
```

### Verify Cosign Signature

```bash
cosign verify-blob checksums.txt \
  --bundle checksums.txt.sigstore.json \
  --certificate-identity-regexp=https://github.com/bdwyertech \
  --certificate-oidc-issuer=https://token.actions.githubusercontent.com
```

## License

See LICENSE file.
