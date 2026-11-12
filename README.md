# name-generator

An Axum API and MCP server that returns random first names for a given gender
and birth year, optionally weighted by popularity, using the ONS
[Top 100 baby names in England and Wales: historical data](https://www.ons.gov.uk/peoplepopulationandcommunity/birthsdeathsandmarriages/livebirths/datasets/babynamesenglandandwalestop100babynameshistoricaldata)
(1904–2024).

```sh
API_KEY=secret cargo run     # REST + MCP over HTTP on 0.0.0.0:3000 (override with BIND_ADDR)
cargo run -- --stdio         # MCP over stdio, no API key
cargo test
```

## Docker

```sh
docker run --rm -p 3000:3000 -e API_KEY=secret ghcr.io/ikadmium/name-generator
```

The image is a single static binary on `scratch`, published for `linux/amd64`
and `linux/arm64`. The Dockerfile cross-compiles on the build host with
`rust-lld`, so no emulation is needed.

```sh
docker buildx build --platform linux/amd64,linux/arm64 .
```

## MCP

The same name generation is available as an MCP tool, `generate_names`. Its
arguments are the same as the `POST /names` body below, and it returns the
same JSON as structured content.

### Streamable HTTP

The HTTP server also serves MCP at `/mcp`. It needs the same `X-API-Key`
header as the REST API. For example, in Claude Code:

```sh
claude mcp add --transport http names https://your-host/mcp --header "X-API-Key: secret"
```

The endpoint is stateless (no MCP sessions), so it can run behind a load
balancer with any number of replicas.

### stdio

For local clients, run the binary or image with `--stdio`. No API key is
needed because nothing is exposed on the network:

```json
{
  "mcpServers": {
    "names": {
      "command": "docker",
      "args": ["run", "-i", "--rm", "ghcr.io/ikadmium/name-generator", "--stdio"]
    }
  }
}
```

## CI

`.github/workflows/ci.yml` runs on pull requests and pushes to `main`:

- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`
- multi-arch Docker build; on `main` it is pushed to GHCR as `latest` and
  `sha-<short sha>`

## API

### Authentication

The server refuses to start unless the `API_KEY` environment variable is set.
Every request, including to `/mcp`, must send that value in the `X-API-Key` header, or it gets
`401`.

### `POST /names`

```sh
$ curl localhost:3000/names -H 'x-api-key: secret' -H 'content-type: application/json' -d '{
    "people": [
      {"gender": "female", "year": 1987},
      {"gender": "male", "year": 2026}
    ],
    "forbidden": ["Sarah", "Emma"]
  }'
{"names":[
  {"name":"Laura","gender":"female","year":1987,"dataset_year":1984},
  {"name":"Theo","gender":"male","year":2026,"dataset_year":2024}
]}
```

| field       | required | notes                                                          |
|-------------|----------|----------------------------------------------------------------|
| `people`    | yes      | 1–100 `{gender, year}` objects; one name is returned for each, in order |
| `gender`    |          | `male` / `female` (also accepts `m`, `boy`, `f`, `girl`)       |
| `year`      |          | birth year                                                     |
| `forbidden` | no       | names to exclude (case-insensitive)                            |
| `weighted`  | no       | `true` to weight names by popularity; default `false` (uniform) |

All returned names are distinct. Each name picked is added to the forbidden
list before the next person's name is drawn, across genders and years.

### Errors

Errors return JSON `{"error": "..."}`:

- `401` when the `X-API-Key` header is missing or wrong
- `400` for malformed JSON
- `415` when the `Content-Type` isn't `application/json`
- `422` when fields are missing or invalid, or the forbidden list leaves no
  names for a person

## How the data is used

- **Years**: the ONS publishes rankings only every 10 years (1904, 1914, …,
  2024). Each request uses the dataset year nearest to `year` (ties go to the
  earlier year). Years outside the range are clamped. The year actually used
  is returned as `dataset_year`.
- **Weighting**: off by default, so each of the year's top 100 names is
  equally likely. With `"weighted": true`, names are weighted by popularity.
  The dataset gives ranks, not counts, so weighting uses Zipf's law,
  `weight = 1 / rank`: the #1 name is 100× as likely as the #100 name.

## Data pipeline

`data/names.csv` (`gender,year,rank,name`) is generated once from the ONS
workbook and compiled into the binary with `include_str!`:

```sh
pip install openpyxl
python3 scripts/convert_xlsx.py historicalnames2024.xlsx > data/names.csv
```

The script trims trailing whitespace from names, skips blank cells, and keeps
tied ranks (e.g. 2004 boys has two names at rank 100).
