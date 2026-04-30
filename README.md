# Golem JournAI

A distributed log analysis and monitoring system built on [Golem Cloud](https://golem.cloud), leveraging WebAssembly
components for scalable, intelligent log processing and visualization.

## Overview

Golem JournAI is a cloud-native journald log analysis platform that collects, analyzes, and visualizes system logs using
AI-powered insights. Built with Rust and compiled to WebAssembly, it runs as distributed stateful components on the
Golem Cloud platform.

## Architecture

The system consists of three main WASM components:

### 1. **Collector** (`journai:collector`)

Collects and stores journal entries from monitored hosts.

**Key Features:**

- RESTful API for log ingestion
- Filtering and priority-based storage
- Error spike detection
- Query interface for stored entries

**API Endpoints:**

- `POST /collect/{hostname}` - Ingest journal entries
- `GET /entries/{hostname}` - Query entries with filters (since, priority, contains)
- `GET /errors/{hostname}` - Get detected error spikes

### 2. **Analyzer** (`journai:analyzer`)

Performs AI-powered analysis of log patterns and anomalies.

**Key Features:**

- LLM-powered error spike analysis
- Pattern recognition and root cause analysis
- Support for multiple LLM providers (Anthropic, OpenAI, Grok, Ollama, etc.)
- Structured assertion generation

### 3. **Visualizer** (`journai:visualizer`)

Provides web-based dashboards and visualization interfaces.

**Key Features:**

- Real-time dashboard overview
- Alert monitoring interface
- Analysis queue tracking
- Analysis history and details view

**Dashboard Endpoints:**

- `GET /dashboard/overview` - System overview dashboard
- `GET /dashboard/alerts` - Active alerts view
- `GET /analysis/queue` - Pending analysis queue
- `GET /analysis/history/{hostname}` - Host analysis history
- `GET /analysis/details/{analysis_id}` - Detailed analysis view

## Technology Stack

- **Language:** Rust
- **Runtime:** WebAssembly (WASI)
- **Platform:** [Golem Cloud](https://golem.cloud)
- **Framework:** golem-rust
- **AI Integration:** Golem AI libraries for LLM, Golem RDBMS libraries
- **Build System:** Cargo workspace

## Project Structure

```
golem-journai/
├── components-rust/
│   ├── journai-collector/    # Log collection component
│   ├── journai-analyzer/     # AI analysis component
│   └── journai-visualizer/   # Visualization component
├── common-rust/
│   └── common-lib/            # Shared models and utilities
├── golem.yaml                 # Golem Cloud application manifest
├── Cargo.toml                 # Workspace configuration
└── docker-compose.yml         # Local development setup
```

## Getting Started

### Prerequisites

- Rust toolchain (with `wasm32-wasip1` target)
- [Golem CLI](https://learn.golem.cloud/quickstart)
- Docker (optional, for local development)

### Installation

1. Clone the repository:

```bash
git clone https://github.com/yourusername/golem-journai.git
cd golem-journai
```

2. Configure environment variables (copy and edit `.env`):

```bash
# LLM Provider (e.g. OpenRouter)
OPENROUTER_API_KEY=<your-key>
# other env variables from detailed section below
```

### Building

Build all components:

```bash
golem build
```

### Deployment

#### Local Development

Deploy to local Golem instance:

```bash
golem deploy --environment local
```

#### Golem Cloud

Deploy to Golem Cloud:

```bash
golem deploy --environment cloud
```

#### Presets

To force a preset when deploying (e.g. release) use:

```bash
golem deploy --preset release
```

## Configuration (environment variables)

| Environment Variable               | Description                                     | Default Value (Debug)         | Default Value (Release)      |
|------------------------------------|-------------------------------------------------|-------------------------------|------------------------------|
| `DATABASE_TYPE`                    | Type of database used (supported: `postgresql`) | `postgresql`                  | `postgresql`                 |
| `DATABASE_INIT`                    | Indicates whether to initialize the database    | `"true"`                      | `"true"`                     |
| `DATABASE_HOST`                    | Database host                                   | `host.docker.internal`        | `host.docker.internal`       |
| `DATABASE_USER`                    | Database user                                   | `postgres`                    | `postgres`                   |
| `DATABASE_DB`                      | Database name                                   | `postgres`                    | `postgres`                   |
| `DATABASE_PASSWORD`                | Database password                               | `journai`                     | `journai`                    |
| `DATABASE_QUERY_LOG`               | Enables logging of database queries             | `"false"`                     | `"false"`                    |
| `OPENROUTER_API_KEY`               | API key for OpenRouter                          | `{{ OPENROUTER_API_KEY }}`    | `{{ OPENROUTER_API_KEY }}`   |
| `JOURNAI_LLM_MODEL`                | LLM model used for analysis                     | `"xiaomi/mimo-v2-flash:free"` | `"perplexity/sonar-pro"`     |
| `JOURNAI_LLM_ENTRIES_LIMIT`        | Limit of entries for LLM analysis               | `(not defined, default: 500)` | `"3000"`                     |
| `JOURNAI_LLM_CONTEXT_WINDOW_LIMIT` | Limit of events in the LLM context window       | `(not defined, default: 20)`  | `(not defined, default: 20)` |
| `RUST_LOG`                         | Logging level for Rust                          | `debug`                       | `info`                       |

## Usage

### Collecting Logs

Send journal entries to the collector:

```bash
curl -X POST http://journai.localhost:9006/collect/myhost \
  -H "Content-Type: application/json" \
  -d '{"entries":[
    {
      "boot_id": "b4df5e7d8e4f4b5a8d2a7ab2c1a0ef9a",
      "hostname": "myhost",
      "machine_id": "c1d2e3f4a5b67890c1d2e3f4a5b67890",
      "priority": "3",
      "message": "Error occurred in service",
      "date": 1234567890.0,
      "runtime_scope": "system"
    }
  ]}'
```

This is aimed to be used by [Fluent Bit](https://fluentbit.io), with a configuration similar to:

- config.yaml:

```yaml
service:
  flush: 1
  log_level: info
  parsers_file: parsers.yaml

pipeline:
  inputs:
    - name: systemd
      tag: host.journal
      path: /var/log/journal
      db: /fluent-bit/state/systemd.db
      read_from_tail: off
      strip_underscores: on
      lowercase: on

  filters:
    - name: grep
      match: '*'
      regex: "priority 0|1|2|3"
    - name: lua
      match: '*'
      script: /fluent-bit/etc/normalize.lua
      call: normalize_keys

  outputs:
    - name: http
      match: '*'
      host: journai.localhost
      port: 9006
      uri: /collect/myhost
      format: msgpack
      body_key: $body
      headers_key: $headers
      retry_limit: false
```

- normalize.lua

```lua
local function to_snake(str)
    str = str:gsub("(%l)(%u)", "%1_%2")
    str = str:gsub("-", "_")
    return string.lower(str)
end

local function json_escape(str)
    return str
        :gsub("\\", "\\\\")
        :gsub("\"", "\\\"")
        :gsub("\n", "\\n")
        :gsub("\r", "\\r")
        :gsub("\t", "\\t")
end

local function encode_json(value)
    local value_type = type(value)

    if value_type == "string" then
        return "\"" .. json_escape(value) .. "\""
    end
    if value_type == "number" or value_type == "boolean" then
        return tostring(value)
    end
    if value == nil then
        return "null"
    end
    if value_type == "table" then
        local array_len = #value
        local parts = {}

        if array_len > 0 then
            for i = 1, array_len do
                parts[#parts + 1] = encode_json(value[i])
            end
            return "[" .. table.concat(parts, ",") .. "]"
        end

        for k, v in pairs(value) do
            parts[#parts + 1] = "\"" .. tostring(k) .. "\":" .. encode_json(v)
        end
        return "{" .. table.concat(parts, ",") .. "}"
    end

    return "\"" .. json_escape(tostring(value)) .. "\""
end

function normalize_keys(tag, ts, record)
    local entry = {}
    for k, v in pairs(record) do
        entry[to_snake(k)] = v
    end

    local payload = { entries = { entry } }
    local body = encode_json(payload)

    return 2, ts, {
        body = body,
        headers = {
            ["Content-Type"] = "application/json"
        }
    }
end
```

### Querying Entries

Retrieve stored entries with filters:

```bash
curl "http://journai.localhost:9006/entries/myhost?priority=3&contains=error"
```

### Viewing Dashboards

Access the visualization dashboard:

```bash
open http://journai.localhost:9006/dashboard/overview
```

## Development

### Adding Dependencies

Edit the component's `golem.yaml` to add WASM dependencies
from [Golem AI](https://github.com/golemcloud/golem-ai/releases) (please remember to configure the correct
API key depending on the chosen LLM and the model name).

### Docker Compose Database Defaults

The `common-env` component template in `common-rust/golem.yaml` provides defaults for most environment variables.

If you use the `docker-compose.yml` Postgres service, override the database settings to:

```bash
DATABASE_HOST=postgres
DATABASE_USER=postgres
DATABASE_DB=postgres
DATABASE_PASSWORD=journai
```

## API Reference

Full API documentation available at:

- Collector API: See `components-rust/journai-collector/golem.yaml`
- Visualizer API: See `components-rust/journai-visualizer/golem.yaml`

## Resources

- [Golem Cloud Documentation](https://learn.golem.cloud)
- [Golem Rust SDK](https://github.com/golemcloud/golem-rust)
- [Golem AI Libraries](https://github.com/golemcloud/golem-ai)
