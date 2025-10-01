# parts

A CLI tool for managing PCB part libraries with automatic data fetching and curation.

## Overview

This tool helps manage a structured database of PCB parts by combining human-curated part lists with automatically fetched data from supplier APIs like Digikey. The goal is to maintain fresh, accurate part information while minimizing manual work.

## Architecture

The system uses a hybrid approach:

- **Human-curated sources** (`db/sources.kdl`) - Single source of truth for parts and series you care about
- **Auto-generated data files** (`db/digikey/`) - Detailed part information fetched from APIs
- **Staleness-aware updates** - Only fetches fresh data when existing data is stale (>24 hours old)
- **Force refresh option** - Override staleness checks when needed

## Data Storage

Data is stored in structured formats in the `db/` directory:

- `db/sources.kdl` - Human-maintained parts and series (edit this manually)
  - Individual parts (ICs, MCUs) with specific MPNs
  - Series patterns for passives (resistors, capacitors, etc.)
- `db/digikey/part_details/` - Auto-generated individual JSON files for each part
  - Each file is named with the MPN (e.g., `STM32H747XIH6.json`)
  - Contains manufacturer, part numbers, status, URLs, and description
  - Excludes pricing and quantity data to focus on static part information

Auto-generated files are managed by the tool and should not be edited manually.

## Quick Start

1. Set up Digikey API credentials:
   ```bash
   export DIGIKEY_CLIENT_ID="your_client_id"
   export DIGIKEY_CLIENT_SECRET="your_client_secret"
   ```

2. Add parts to your sources file:
   ```bash
   # Edit db/sources.kdl manually to add:
   # - Individual parts (ICs, MCUs) with specific MPNs
   # - Series patterns for passives (resistors, capacitors)
   ```

3. Fetch detailed part information:
   ```bash
   cargo run -- gen digikey-part-info
   ```

4. Look up individual parts:
   ```bash
   cargo run -- fetch digikey-part-info STM32H747XIH6
   ```

## Commands

### Generation Commands

- `gen digikey-part-info [--force]` - Update detailed part information for all parts in your curated list (respects staleness)
- `gen resistor-series [--file <path>]` - Generate resistor parts from series definitions (default: `db/sources.kdl`)

### Fetch Commands

- `fetch digikey-part-info <mpn>` - Look up detailed information for a specific manufacturer part number

### Search Commands

- `search <part_number> [--format <format>]` - Search for part details with different output formats
  - `--format json` (default) - Detailed JSON output
  - `--format flat` - Flattened key-value format
  - `--format raw` - Raw API response

## Staleness Management

The tool automatically tracks when data was last fetched and only updates stale data (older than 24 hours). This minimizes API calls and respects rate limits. Use `--force` to override staleness checks.

## API Integration

Currently supports the Digikey Product Information API with OAuth2 client credentials authentication. The tool handles token caching and automatic refresh.
